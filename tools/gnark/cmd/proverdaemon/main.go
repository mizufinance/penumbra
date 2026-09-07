package main

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"time"

	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/logger"

	"github.com/mizufinance/shieldd/tools/gnark/internal/abi"
	"github.com/mizufinance/shieldd/tools/gnark/internal/artifacts"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

const (
	daemonReadyMagic    = "PGDR"
	daemonRequestMagic  = "PGRQ"
	daemonResponseMagic = "PGRS"
	daemonOpProve       = 1
	daemonOpShutdown    = 2
	// Witness payloads are a few KB; cap at 4 MiB to prevent unbounded allocation.
	maxRequestSize = 4 * 1024 * 1024
)

type daemonReady struct {
	Magic                    string `json:"magic"`
	Status                   string `json:"status"`
	Circuit                  string `json:"circuit"`
	Curve                    string `json:"curve"`
	MetadataSHA256Hex        string `json:"metadata_sha256_hex"`
	VerifyingKeyBinarySHA256 string `json:"verifying_key_binary_sha256_hex"`
	VerifyingKeyJSONSHA256   string `json:"verifying_key_json_sha256_hex"`
	ProvingKeySHA256         string `json:"proving_key_sha256_hex"`
}

type proveFunc func([]byte) ([]byte, error)

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
				assignment, witnessFamily, err := abi.NewTransferCircuitAssignmentFromWitness(payload)
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
	for _, family := range generated.NoteReshapeFamilies {
		family := family
		circuitConfigs[family.Label] = circuitConfig{
			name: family.Label,
			template: func() frontend.Circuit {
				return circuits.NewNoteReshapeCircuit(family.Label, family.NIn, family.NOut)
			},
			newAssignment: func(payload []byte) (frontend.Circuit, error) {
				assignment, witnessFamily, err := abi.NewNoteReshapeCircuitAssignmentFromWitness(payload)
				if err != nil {
					return nil, err
				}
				if witnessFamily.ID != family.ID {
					return nil, fmt.Errorf(
						"note reshape witness family mismatch: got %s (%d), expected %s (%d)",
						witnessFamily.Label,
						witnessFamily.ID,
						family.Label,
						family.ID,
					)
				}
				return assignment, nil
			},
			packResult: packNoteReshapeProofResult,
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
				assignment, witnessFamily, err := abi.NewShieldedIcs20WithdrawalCircuitAssignmentFromWitness(payload)
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
	circuitConfigs["note_seizure"] = circuitConfig{
		name: "note_seizure",
		template: func() frontend.Circuit {
			return circuits.NewNoteSeizureCircuit()
		},
		newAssignment: func(payload []byte) (frontend.Circuit, error) {
			return abi.NewNoteSeizureCircuitAssignmentFromWitness(payload)
		},
		packResult: packNoteSeizureProofResult,
	}
}

func main() {
	logger.Disable()

	circuit := flag.String("circuit", "", "transfer, note-reshape, shielded-ics20-withdrawal, or note-seizure family label")
	artifactDir := flag.String("artifact-dir", "", "directory containing gnark artifacts")
	flag.Parse()

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

	pkBytes, err := os.ReadFile(filepath.Join(artifactDir, "proving_key.bin"))
	if err != nil {
		return nil, nil, fmt.Errorf("read proving key: %w", err)
	}
	if err := artifacts.ValidateProvingKeyBytes(metadata, pkBytes); err != nil {
		return nil, nil, err
	}
	pk, err := artifacts.ReadProvingKeyStrict(bytes.NewReader(pkBytes))
	if err != nil {
		return nil, nil, err
	}
	return metadata, pk, nil
}

func buildReady(circuit, artifactDir string, metadata *artifacts.CircuitMetadataJSON) (*daemonReady, error) {
	metadataHash, err := artifacts.SHA256HexFile(filepath.Join(artifactDir, "circuit_metadata.json"))
	if err != nil {
		return nil, fmt.Errorf("hash metadata: %w", err)
	}
	return &daemonReady{
		Magic:                    daemonReadyMagic,
		Status:                   "ready",
		Circuit:                  circuit,
		Curve:                    metadata.Curve,
		MetadataSHA256Hex:        metadataHash,
		VerifyingKeyBinarySHA256: metadata.VerifyingKeyBinarySHA256Hex,
		VerifyingKeyJSONSHA256:   metadata.VerifyingKeyJSONSHA256Hex,
		ProvingKeySHA256:         metadata.ProvingKeySHA256Hex,
	}, nil
}

func readRequest(reader *bufio.Reader) (uint32, []byte, error) {
	var header [12]byte
	if _, err := io.ReadFull(reader, header[:]); err != nil {
		return 0, nil, err
	}
	if string(header[:4]) != daemonRequestMagic {
		return 0, nil, fmt.Errorf("invalid daemon request magic")
	}
	totalLen := binary.LittleEndian.Uint32(header[4:8])
	if totalLen < 12 {
		return 0, nil, fmt.Errorf("invalid daemon request length %d", totalLen)
	}
	payloadLen := int(totalLen) - 12
	if payloadLen > maxRequestSize {
		return 0, nil, fmt.Errorf("daemon request payload %d bytes exceeds limit %d", payloadLen, maxRequestSize)
	}
	op := binary.LittleEndian.Uint32(header[8:12])
	payload := make([]byte, payloadLen)
	if _, err := io.ReadFull(reader, payload); err != nil {
		return 0, nil, err
	}
	return op, payload, nil
}

func writeResponse(writer *bufio.Writer, status uint32, payload []byte) error {
	totalLen := uint32(12 + len(payload))
	var header [12]byte
	copy(header[:4], []byte(daemonResponseMagic))
	binary.LittleEndian.PutUint32(header[4:8], totalLen)
	binary.LittleEndian.PutUint32(header[8:12], status)
	if _, err := writer.Write(header[:]); err != nil {
		return err
	}
	if _, err := writer.Write(payload); err != nil {
		return err
	}
	return writer.Flush()
}

func packShieldedIcs20WithdrawalProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, _, err := abi.DecodeShieldedIcs20WithdrawalWitness(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode shielded ICS-20 withdrawal witness: %w", err)
	}
	return packProofResult("PIPR", witness.ClaimedStatementHash, proof, proveMS)
}

func packTransferProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, _, err := abi.DecodeTransferWitness(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode transfer witness: %w", err)
	}
	return packProofResult("PTPR", witness.ClaimedStatementHash, proof, proveMS)
}

func packNoteReshapeProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, _, err := abi.DecodeNoteReshapeWitness(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode note reshape witness: %w", err)
	}
	return packProofResult("PNRP", witness.ClaimedStatementHash, proof, proveMS)
}

func packNoteSeizureProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, err := abi.DecodeNoteSeizureWitness(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode note seizure witness: %w", err)
	}
	return packProofResult("PNSP", witness.ClaimedStatementHash, proof, proveMS)
}

func packProofResult(magic string, claimedStatementHash [32]byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	buf := bytes.NewBuffer(make([]byte, 0, 4+4+4+8+32+384))
	buf.WriteString(magic)
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
	binary.LittleEndian.PutUint32(out[4:8], uint32(len(out)))
	return out, nil
}
