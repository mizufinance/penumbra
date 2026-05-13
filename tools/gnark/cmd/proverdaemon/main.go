package main

import (
	"bufio"
	"bytes"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/logger"

	"github.com/mizufinance/penumbra/tools/gnark/internal/abi"
	"github.com/mizufinance/penumbra/tools/gnark/internal/artifacts"
	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

const (
	daemonReadyMagic      = "PGDR"
	daemonReadyVersion    = 1
	daemonRequestMagic    = "PGRQ"
	daemonResponseMagic   = "PGRS"
	daemonProtocolVersion = 1
	daemonOpProve         = 1
	daemonOpShutdown      = 2
	// Witness payloads are a few KB; cap at 4 MiB to prevent unbounded allocation.
	maxRequestSize = 4 * 1024 * 1024
)

type daemonReady struct {
	Magic              string `json:"magic"`
	Version            int    `json:"version"`
	Status             string `json:"status"`
	Circuit            string `json:"circuit"`
	Curve              string `json:"curve"`
	MetadataSHA256Hex  string `json:"metadata_sha256_hex"`
	VerifyingKeySHA256 string `json:"verifying_key_sha256_hex"`
	ProvingKeySHA256   string `json:"proving_key_sha256_hex"`
	VerifyingKeyID     string `json:"verifying_key_id,omitempty"`
}

type proveFunc func([]byte) ([]byte, error)

type proveResponse struct {
	Family           string  `json:"family"`
	Result           string  `json:"result"`
	Proof            string  `json:"proof"`
	ProveMs          float64 `json:"proveMs"`
	VerifyingKeyID   string  `json:"verifyingKeyId"`
	ProvingKeySHA256 string  `json:"provingKeySHA256"`
}

type proverEntry struct {
	ready *daemonReady
	prove proveFunc
}

type circuitConfig struct {
	name          string
	template      func() frontend.Circuit
	newAssignment func([]byte) (frontend.Circuit, error)
	packResult    func([]byte, *groth16bls.Proof, float64) ([]byte, error)
}

var circuitConfigs = map[string]circuitConfig{}

func init() {
	for _, family := range generated.TransferFamilies {
		family := family
		circuitConfigs[family.Label] = circuitConfig{
			name: family.Label,
			template: func() frontend.Circuit {
				return circuits.NewTransferCircuit()
			},
			newAssignment: func(payload []byte) (frontend.Circuit, error) {
				assignment, witnessFamily, err := abi.NewTransferCircuitAssignmentFromWitnessV1(payload)
				if err != nil {
					return nil, err
				}
				if witnessFamily.Label != family.Label {
					return nil, fmt.Errorf(
						"transfer witness family mismatch: got %s, expected %s",
						witnessFamily.Label,
						family.Label,
					)
				}
				return assignment, nil
			},
			packResult: packTransferProofResult,
		}
	}
	for _, family := range generated.ConsolidateFamilies {
		family := family
		circuitConfigs[family.Label] = circuitConfig{
			name: family.Label,
			template: func() frontend.Circuit {
				return circuits.NewConsolidateCircuit(family.NIn)
			},
			newAssignment: func(payload []byte) (frontend.Circuit, error) {
				assignment, witnessFamily, err := abi.NewConsolidateCircuitAssignmentFromWitnessV1(payload)
				if err != nil {
					return nil, err
				}
				if witnessFamily.ID != family.ID {
					return nil, fmt.Errorf(
						"consolidate witness family mismatch: got %s (%d), expected %s (%d)",
						witnessFamily.Label,
						witnessFamily.ID,
						family.Label,
						family.ID,
					)
				}
				return assignment, nil
			},
			packResult: packConsolidateProofResult,
		}
	}
	for _, family := range generated.SplitFamilies {
		family := family
		circuitConfigs[family.Label] = circuitConfig{
			name: family.Label,
			template: func() frontend.Circuit {
				return circuits.NewSplitCircuit(family.NOut)
			},
			newAssignment: func(payload []byte) (frontend.Circuit, error) {
				assignment, witnessFamily, err := abi.NewSplitCircuitAssignmentFromWitnessV1(payload)
				if err != nil {
					return nil, err
				}
				if witnessFamily.ID != family.ID {
					return nil, fmt.Errorf(
						"split witness family mismatch: got %s (%d), expected %s (%d)",
						witnessFamily.Label,
						witnessFamily.ID,
						family.Label,
						family.ID,
					)
				}
				return assignment, nil
			},
			packResult: packSplitProofResult,
		}
	}
	for _, family := range generated.ShieldedIcs20WithdrawalFamilies {
		family := family
		circuitConfigs[family.Label] = circuitConfig{
			name: family.Label,
			template: func() frontend.Circuit {
				return circuits.NewShieldedIcs20WithdrawalCircuit(family.NIn)
			},
			newAssignment: func(payload []byte) (frontend.Circuit, error) {
				assignment, witnessFamily, err := abi.NewShieldedIcs20WithdrawalCircuitAssignmentFromWitnessV1(payload)
				if err != nil {
					return nil, err
				}
				if witnessFamily.ID != family.ID {
					return nil, fmt.Errorf(
						"shielded ICS-20 withdrawal witness family mismatch: got %s (%d), expected %s (%d)",
						witnessFamily.Label,
						witnessFamily.ID,
						family.Label,
						family.ID,
					)
				}
				return assignment, nil
			},
			packResult: packShieldedIcs20WithdrawalProofResult,
		}
	}
}

func main() {
	logger.Disable()

	circuit := flag.String("circuit", "", "transferNxM, consolidateN, splitN, or shielded-ics20-withdrawalN family label")
	artifactDir := flag.String("artifact-dir", "", "directory containing gnark artifacts")
	artifactRoot := flag.String("artifact-root", "/usr/share/penumbra/gnark/artifacts", "root directory containing per-family gnark artifact directories for HTTP mode")
	httpListen := flag.String("http-listen", "", "optional address for local dev HTTP proof service")
	flag.Parse()

	if *httpListen != "" {
		if err := serveHTTP(*httpListen, *artifactRoot); err != nil {
			fmt.Fprintf(os.Stderr, "http prover: %v\n", err)
			os.Exit(1)
		}
		return
	}

	if *circuit == "" || *artifactDir == "" {
		fmt.Fprintln(os.Stderr, "--circuit and --artifact-dir are required")
		os.Exit(2)
	}

	ready, prove, err := initProver(*circuit, *artifactDir)
	if err != nil {
		fmt.Fprintf(os.Stderr, "init prover: %v\n", err)
		os.Exit(1)
	}
	if err := json.NewEncoder(os.Stdout).Encode(ready); err != nil {
		fmt.Fprintf(os.Stderr, "write ready handshake: %v\n", err)
		os.Exit(1)
	}

	reader := bufio.NewReader(os.Stdin)
	writer := bufio.NewWriter(os.Stdout)
	for {
		op, payload, err := readRequest(reader)
		if err != nil {
			if err == io.EOF {
				return
			}
			if writeResponse(writer, 1, []byte(err.Error())) != nil {
				return
			}
			continue
		}

		switch op {
		case daemonOpProve:
			response, err := prove(payload)
			if err != nil {
				if writeResponse(writer, 1, []byte(err.Error())) != nil {
					return
				}
				continue
			}
			if writeResponse(writer, 0, response) != nil {
				return
			}
		case daemonOpShutdown:
			_ = writeResponse(writer, 0, nil)
			return
		default:
			if writeResponse(writer, 1, []byte(fmt.Sprintf("unsupported daemon op %d", op))) != nil {
				return
			}
		}
	}
}

func serveHTTP(addr, artifactRoot string) error {
	var mu sync.Mutex
	cache := map[string]proverEntry{}

	getProver := func(family string) (proverEntry, error) {
		mu.Lock()
		defer mu.Unlock()
		if entry, ok := cache[family]; ok {
			return entry, nil
		}
		if _, ok := circuitConfigs[family]; !ok {
			return proverEntry{}, fmt.Errorf("unsupported circuit %q", family)
		}
		artifactDir := filepath.Join(artifactRoot, family)
		ready, prove, err := initProver(family, artifactDir)
		if err != nil {
			return proverEntry{}, err
		}
		entry := proverEntry{ready: ready, prove: prove}
		cache[family] = entry
		return entry, nil
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})
	mux.HandleFunc("/prove", func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, map[string]string{"error": "method not allowed"})
			return
		}
		var req struct {
			Family  string `json:"family"`
			Witness string `json:"witness"`
		}
		if err := json.NewDecoder(http.MaxBytesReader(w, r.Body, maxRequestSize)).Decode(&req); err != nil {
			writeJSON(w, http.StatusBadRequest, map[string]string{"error": fmt.Sprintf("decode request: %v", err)})
			return
		}
		witness, err := base64.StdEncoding.DecodeString(req.Witness)
		if err != nil {
			writeJSON(w, http.StatusBadRequest, map[string]string{"error": fmt.Sprintf("decode witness: %v", err)})
			return
		}
		entry, err := getProver(req.Family)
		if err != nil {
			writeJSON(w, http.StatusBadRequest, map[string]string{"error": err.Error()})
			return
		}
		start := time.Now()
		result, err := entry.prove(witness)
		if err != nil {
			writeJSON(w, http.StatusInternalServerError, map[string]string{"error": err.Error()})
			return
		}
		encodedProof := base64.StdEncoding.EncodeToString(result)
		writeJSON(w, http.StatusOK, proveResponse{
			Family:           req.Family,
			Result:           encodedProof,
			Proof:            encodedProof,
			ProveMs:          time.Since(start).Seconds() * 1000,
			VerifyingKeyID:   entry.ready.VerifyingKeyID,
			ProvingKeySHA256: entry.ready.ProvingKeySHA256,
		})
	})

	fmt.Fprintf(os.Stderr, "penumbra gnark HTTP prover listening on %s with artifacts %s\n", addr, artifactRoot)
	return http.ListenAndServe(addr, withDevCORS(mux))
}

func withDevCORS(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("access-control-allow-origin", "*")
		w.Header().Set("access-control-allow-methods", "GET, POST, OPTIONS")
		w.Header().Set("access-control-allow-headers", "content-type")
		next.ServeHTTP(w, r)
	})
}

func writeJSON(w http.ResponseWriter, status int, payload any) {
	w.Header().Set("content-type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(payload)
}

func initProver(circuit, artifactDir string) (*daemonReady, proveFunc, error) {
	config, ok := circuitConfigs[circuit]
	if !ok {
		return nil, nil, fmt.Errorf("unsupported circuit %q", circuit)
	}

	ccs, err := frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, config.template())
	if err != nil {
		return nil, nil, fmt.Errorf("compile %s circuit: %w", config.name, err)
	}
	metadata, pk, err := loadContext(artifactDir, config.name, ccs)
	if err != nil {
		return nil, nil, err
	}
	ready, err := buildReady(config.name, artifactDir, metadata)
	if err != nil {
		return nil, nil, err
	}

	return ready, func(witnessPayload []byte) ([]byte, error) {
		assignment, err := config.newAssignment(witnessPayload)
		if err != nil {
			return nil, fmt.Errorf("decode %s witness: %w", config.name, err)
		}
		fullWitness, err := frontend.NewWitness(assignment, primitives.ScalarField())
		if err != nil {
			return nil, fmt.Errorf("construct %s witness: %w", config.name, err)
		}
		proveStart := time.Now()
		proofIface, err := groth16.Prove(ccs, pk, fullWitness)
		if err != nil {
			return nil, fmt.Errorf("prove %s: %w", config.name, err)
		}
		proof, ok := proofIface.(*groth16bls.Proof)
		if !ok {
			return nil, fmt.Errorf("unexpected %s proof type %T", config.name, proofIface)
		}
		return config.packResult(witnessPayload, proof, time.Since(proveStart).Seconds()*1000)
	}, nil
}

func loadContext(artifactDir, circuit string, ccs constraint.ConstraintSystem) (*artifacts.CircuitMetadataJSON, *groth16bls.ProvingKey, error) {
	metadata, err := artifacts.LoadCircuitMetadata(artifactDir)
	if err != nil {
		return nil, nil, fmt.Errorf("load circuit metadata: %w", err)
	}
	if err := artifacts.ValidateCircuitMetadataForCircuit(metadata, circuit, ccs); err != nil {
		return nil, nil, err
	}

	pkFile, err := os.Open(filepath.Join(artifactDir, "proving_key.bin"))
	if err != nil {
		return nil, nil, fmt.Errorf("open proving key: %w", err)
	}
	defer pkFile.Close()
	pk := new(groth16bls.ProvingKey)
	if _, err := pk.ReadFrom(pkFile); err != nil {
		return nil, nil, fmt.Errorf("read proving key: %w", err)
	}
	return metadata, pk, nil
}

func buildReady(circuit, artifactDir string, metadata *artifacts.CircuitMetadataJSON) (*daemonReady, error) {
	metadataHash, err := artifacts.SHA256HexFile(filepath.Join(artifactDir, "circuit_metadata.json"))
	if err != nil {
		return nil, fmt.Errorf("hash metadata: %w", err)
	}
	return &daemonReady{
		Magic:              daemonReadyMagic,
		Version:            daemonReadyVersion,
		Status:             "ready",
		Circuit:            circuit,
		Curve:              metadata.Curve,
		MetadataSHA256Hex:  metadataHash,
		VerifyingKeySHA256: metadata.VerifyingKeySHA256Hex,
		ProvingKeySHA256:   metadata.ProvingKeySHA256Hex,
		VerifyingKeyID:     metadata.VerifyingKeyID,
	}, nil
}

func readRequest(reader *bufio.Reader) (uint32, []byte, error) {
	var header [16]byte
	if _, err := io.ReadFull(reader, header[:]); err != nil {
		return 0, nil, err
	}
	if string(header[:4]) != daemonRequestMagic {
		return 0, nil, fmt.Errorf("invalid daemon request magic")
	}
	version := binary.LittleEndian.Uint32(header[4:8])
	if version != daemonProtocolVersion {
		return 0, nil, fmt.Errorf("unsupported daemon request version %d", version)
	}
	totalLen := binary.LittleEndian.Uint32(header[8:12])
	if totalLen < 16 {
		return 0, nil, fmt.Errorf("invalid daemon request length %d", totalLen)
	}
	payloadLen := int(totalLen) - 16
	if payloadLen > maxRequestSize {
		return 0, nil, fmt.Errorf("daemon request payload %d bytes exceeds limit %d", payloadLen, maxRequestSize)
	}
	op := binary.LittleEndian.Uint32(header[12:16])
	payload := make([]byte, payloadLen)
	if _, err := io.ReadFull(reader, payload); err != nil {
		return 0, nil, err
	}
	return op, payload, nil
}

func writeResponse(writer *bufio.Writer, status uint32, payload []byte) error {
	totalLen := uint32(16 + len(payload))
	var header [16]byte
	copy(header[:4], []byte(daemonResponseMagic))
	binary.LittleEndian.PutUint32(header[4:8], daemonProtocolVersion)
	binary.LittleEndian.PutUint32(header[8:12], totalLen)
	binary.LittleEndian.PutUint32(header[12:16], status)
	if _, err := writer.Write(header[:]); err != nil {
		return err
	}
	if _, err := writer.Write(payload); err != nil {
		return err
	}
	return writer.Flush()
}

func packShieldedIcs20WithdrawalProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, _, err := abi.DecodeShieldedIcs20WithdrawalWitnessV1(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode shielded ICS-20 withdrawal witness: %w", err)
	}
	return packProofResult("PIPR", witness.ClaimedStatementHash, proof, proveMS)
}

func packTransferProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, _, err := abi.DecodeTransferWitnessV1(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode transfer witness: %w", err)
	}
	return packProofResult("PTPR", witness.ClaimedStatementHash, proof, proveMS)
}

func packConsolidateProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, _, err := abi.DecodeConsolidateWitnessV1(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode consolidate witness: %w", err)
	}
	return packProofResult("PCPR", witness.ClaimedStatementHash, proof, proveMS)
}

func packSplitProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, _, err := abi.DecodeSplitWitnessV1(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode split witness: %w", err)
	}
	return packProofResult("PLPR", witness.ClaimedStatementHash, proof, proveMS)
}

func packProofResult(magic string, claimedStatementHash [32]byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	buf := bytes.NewBuffer(make([]byte, 0, 4+4+4+4+8+32+384))
	buf.WriteString(magic)
	_ = binary.Write(buf, binary.LittleEndian, uint32(1))
	_ = binary.Write(buf, binary.LittleEndian, uint32(0))
	_ = binary.Write(buf, binary.LittleEndian, uint32(0))
	_ = binary.Write(buf, binary.LittleEndian, uint64(proveMS*1000))
	buf.Write(claimedStatementHash[:])

	ax := proof.Ar.X.Bytes()
	ay := proof.Ar.Y.Bytes()
	bxa0 := proof.Bs.X.A0.Bytes()
	bxa1 := proof.Bs.X.A1.Bytes()
	bya0 := proof.Bs.Y.A0.Bytes()
	bya1 := proof.Bs.Y.A1.Bytes()
	cx := proof.Krs.X.Bytes()
	cy := proof.Krs.Y.Bytes()
	buf.Write(ax[:])
	buf.Write(ay[:])
	buf.Write(bxa0[:])
	buf.Write(bxa1[:])
	buf.Write(bya0[:])
	buf.Write(bya1[:])
	buf.Write(cx[:])
	buf.Write(cy[:])

	out := buf.Bytes()
	binary.LittleEndian.PutUint32(out[8:12], uint32(len(out)))
	return out, nil
}
