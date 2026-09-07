package main

import (
	"bytes"
	"crypto/sha256"
	"flag"
	"fmt"
	"math/big"
	"os"
	"path/filepath"
	"strings"
	"time"

	curve "github.com/consensys/gnark-crypto/ecc/bls12-377"
	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	backendwitness "github.com/consensys/gnark/backend/witness"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"

	"github.com/mizufinance/shieldd/tools/gnark/internal/abi"
	"github.com/mizufinance/shieldd/tools/gnark/internal/artifacts"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}

	var err error
	switch os.Args[1] {
	case "setup":
		err = runSetup(os.Args[2:])
	case "export-circuit":
		err = runExportCircuit(os.Args[2:])
	case "export-wiring-transcript":
		err = runExportWiringTranscript(os.Args[2:])
	case "prove":
		err = runProve(os.Args[2:])
	case "replay":
		err = runReplay(os.Args[2:])
	case "verify-bench":
		err = runVerifyBench(os.Args[2:])
	case "check-vk-json":
		err = runCheckVKJSON(os.Args[2:])
	default:
		usage()
		os.Exit(2)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: gnarkctl <setup|export-circuit|export-wiring-transcript|prove|replay|verify-bench|check-vk-json> [flags]")
}

const setupProvenanceSchema = "shieldd.gnark.setup_provenance"

type setupGenerationSelfTest struct {
	ProofCase                  string `json:"proof_case"`
	WitnessSHA256Hex           string `json:"witness_sha256_hex"`
	ProvedAndVerifiedInProcess bool   `json:"proved_and_verified_in_process"`
}

type setupProvenance struct {
	Schema                      string                    `json:"schema"`
	Curve                       string                    `json:"curve"`
	Circuit                     string                    `json:"circuit"`
	Mode                        string                    `json:"mode"`
	SR1CSSHA256Hex              string                    `json:"sr1cs_sha256_hex"`
	ProvingKeySHA256Hex         string                    `json:"proving_key_sha256_hex"`
	VerifyingKeyBinarySHA256Hex string                    `json:"verifying_key_binary_sha256_hex"`
	VerifyingKeyJSONSHA256Hex   string                    `json:"verifying_key_json_sha256_hex"`
	GenerationSelfTests         []setupGenerationSelfTest `json:"generation_self_tests"`
	SetupTranscript             string                    `json:"setup_transcript"`
	ToxicWasteErasure           string                    `json:"toxic_waste_erasure"`
}

type repeatedStringFlag []string

func (values *repeatedStringFlag) String() string {
	return strings.Join(*values, ",")
}

func (values *repeatedStringFlag) Set(value string) error {
	if value == "" {
		return fmt.Errorf("value must be nonempty")
	}
	*values = append(*values, value)
	return nil
}

func isLowerSnakeIdentifier(value string) bool {
	if value == "" {
		return false
	}
	for _, character := range value {
		if (character < 'a' || character > 'z') &&
			(character < '0' || character > '9') &&
			character != '_' {
			return false
		}
	}
	return true
}

func runExportCircuit(args []string) error {
	fs := flag.NewFlagSet("export-circuit", flag.ContinueOnError)
	circuit := fs.String("circuit", "", "registered circuit label")
	sr1csPath := fs.String("sr1cs-out", "", "output SR1CS path")
	manifestPath := fs.String("manifest-out", "", "output semantic manifest path")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *circuit == "" || *sr1csPath == "" || *manifestPath == "" {
		return fmt.Errorf("--circuit, --sr1cs-out, and --manifest-out are required")
	}
	ccs, manifest, err := compileCircuitForExport(*circuit)
	if err != nil {
		return err
	}
	for _, path := range []string{*sr1csPath, *manifestPath} {
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return fmt.Errorf("create output directory: %w", err)
		}
	}
	if err := artifacts.WriteConstraintSystem(*sr1csPath, ccs); err != nil {
		return err
	}
	manifest.SR1CSSHA256Hex, err = artifacts.SHA256HexFile(*sr1csPath)
	if err != nil {
		return err
	}
	if err := circuits.WriteConstraintManifest(*manifestPath, manifest); err != nil {
		return err
	}
	fmt.Fprintf(os.Stderr, "wrote circuit export for %s (%d constraints)\n", *circuit, manifest.NbConstraints)
	return nil
}

func compileCircuitForExport(circuit string) (constraint.ConstraintSystem, *circuits.ConstraintManifest, error) {
	if circuit == "note_seizure" {
		return circuits.CompileNoteSeizureForExport()
	}
	if family, ok := generated.NoteReshapeFamilyByLabel(circuit); ok {
		return circuits.CompileNoteReshapeForExport(family.Label, family.NIn, family.NOut)
	}
	if _, ok := generated.TransferFamilyByLabel(circuit); ok {
		return circuits.CompileTransferForExport()
	}
	if family, ok := generated.ShieldedIcs20WithdrawalFamilyByLabel(circuit); ok {
		return circuits.CompileShieldedIcs20WithdrawalForExport(family.Label, family.NIn)
	}
	return nil, nil, fmt.Errorf("unsupported circuit export %q", circuit)
}

func runExportWiringTranscript(args []string) error {
	fs := flag.NewFlagSet("export-wiring-transcript", flag.ContinueOnError)
	circuit := fs.String("circuit", "", "supported circuit label")
	outPath := fs.String("out", "", "output canonical wiring transcript path")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *circuit == "" || *outPath == "" {
		return fmt.Errorf("--circuit and --out are required")
	}
	var out string
	var err error
	if family, ok := generated.NoteReshapeFamilyByLabel(*circuit); ok {
		out, err = circuits.ExportNoteReshapeWiringTranscript(family.Label, family.NIn, family.NOut)
	} else if *circuit == "transfer" {
		out, err = circuits.ExportTransferWiringTranscript()
	} else {
		return fmt.Errorf("unsupported wiring transcript circuit %q", *circuit)
	}
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(*outPath), 0o755); err != nil {
		return fmt.Errorf("create output dir: %w", err)
	}
	if err := os.WriteFile(*outPath, []byte(out), 0o644); err != nil {
		return fmt.Errorf("write %s: %w", *outPath, err)
	}
	fmt.Fprintf(os.Stderr, "wrote %s\n", *outPath)
	return nil
}

func runSetup(args []string) error {
	fs := flag.NewFlagSet("setup", flag.ContinueOnError)
	circuit := fs.String("circuit", "", "transferNxM, note reshape, or shielded-ics20-withdrawal family label")
	outDir := fs.String("out-dir", "", "output directory")
	reuseExistingKeys := fs.Bool(
		"reuse-existing-keys",
		false,
		"refresh metadata for a byte-identical SR1CS after proving with existing keys",
	)
	var proofCases repeatedStringFlag
	var witnessPaths repeatedStringFlag
	fs.Var(&proofCases, "proof-case", "canonical generation self-test case; repeat once per --witness")
	fs.Var(&witnessPaths, "witness", "canonical generation self-test witness; repeat once per --proof-case")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *circuit == "" || *outDir == "" {
		return fmt.Errorf("--circuit and --out-dir are required")
	}
	if err := validateSetupSelfTestFlags(proofCases, witnessPaths); err != nil {
		return err
	}
	if err := os.MkdirAll(*outDir, 0o755); err != nil {
		return fmt.Errorf("create output dir: %w", err)
	}
	stageDir, err := os.MkdirTemp(*outDir, ".setup-")
	if err != nil {
		return fmt.Errorf("create setup staging dir: %w", err)
	}
	defer os.RemoveAll(stageDir)

	ccs, compileMS, err := compileCircuit(*circuit)
	if err != nil {
		return err
	}

	sr1csPath := filepath.Join(stageDir, *circuit+".sr1cs")
	if err := artifacts.WriteConstraintSystem(sr1csPath, ccs); err != nil {
		return fmt.Errorf("write constraint system: %w", err)
	}

	var pk *groth16bls.ProvingKey
	var vk *groth16bls.VerifyingKey
	setupMS := 0.0
	mode := "fresh_setup"
	pkPath := filepath.Join(stageDir, "proving_key.bin")
	vkPath := filepath.Join(stageDir, "verifying_key.bin")
	vkJSONPath := filepath.Join(stageDir, "verifying_key.json")
	if *reuseExistingKeys {
		mode = "reused_existing_keys"
		deployedSR1CSPath := filepath.Join(*outDir, *circuit+".sr1cs")
		equal, err := filesEqual(sr1csPath, deployedSR1CSPath)
		if err != nil {
			return fmt.Errorf("compare compiled and deployed SR1CS: %w", err)
		}
		if !equal {
			return fmt.Errorf(
				"--reuse-existing-keys requires compiled SR1CS bytes to equal %s",
				deployedSR1CSPath,
			)
		}
		pkPath = filepath.Join(*outDir, "proving_key.bin")
		vkPath = filepath.Join(*outDir, "verifying_key.bin")
		vkJSONPath = filepath.Join(*outDir, "verifying_key.json")
		pk, _, err = loadPK(pkPath)
		if err != nil {
			return fmt.Errorf("load existing proving key: %w", err)
		}
		vk, _, err = loadVK(vkPath)
		if err != nil {
			return fmt.Errorf("load existing verifying key: %w", err)
		}
		if err := validateVerifyingKeyJSON(vkJSONPath, vk); err != nil {
			return fmt.Errorf("validate existing verifying key JSON: %w", err)
		}
	} else {
		setupStart := time.Now()
		pkIface, vkIface, err := groth16.Setup(ccs)
		if err != nil {
			return fmt.Errorf("setup: %w", err)
		}
		setupMS = time.Since(setupStart).Seconds() * 1000
		pk = pkIface.(*groth16bls.ProvingKey)
		vk = vkIface.(*groth16bls.VerifyingKey)
		if err := writePK(pkPath, pk); err != nil {
			return err
		}
		if err := writeVK(vkPath, vk); err != nil {
			return err
		}
		if err := artifacts.WriteJSON(
			vkJSONPath,
			artifacts.EncodeVerifyingKeyJSON(vk),
		); err != nil {
			return fmt.Errorf("write verifying key json: %w", err)
		}
	}

	generationSelfTests, err := runSetupSelfTests(
		ccs,
		pk,
		vk,
		*circuit,
		proofCases,
		witnessPaths,
	)
	if err != nil {
		return err
	}

	pkSize, err := artifacts.FileSize(pkPath)
	if err != nil {
		return fmt.Errorf("stat proving key: %w", err)
	}
	vkSize, err := artifacts.FileSize(vkPath)
	if err != nil {
		return fmt.Errorf("stat verifying key: %w", err)
	}

	sr1csHash, err := artifacts.SHA256HexFile(sr1csPath)
	if err != nil {
		return fmt.Errorf("hash constraint system: %w", err)
	}
	pkHash, err := artifacts.SHA256HexFile(pkPath)
	if err != nil {
		return fmt.Errorf("hash proving key: %w", err)
	}
	vkBinaryHash, err := artifacts.SHA256HexFile(vkPath)
	if err != nil {
		return fmt.Errorf("hash verifying key binary: %w", err)
	}
	vkJSONHash, err := artifacts.SHA256HexFile(vkJSONPath)
	if err != nil {
		return fmt.Errorf("hash verifying key json: %w", err)
	}

	provenance := setupProvenance{
		Schema:                      setupProvenanceSchema,
		Curve:                       "bls12-377",
		Circuit:                     *circuit,
		Mode:                        mode,
		SR1CSSHA256Hex:              sr1csHash,
		ProvingKeySHA256Hex:         pkHash,
		VerifyingKeyBinarySHA256Hex: vkBinaryHash,
		VerifyingKeyJSONSHA256Hex:   vkJSONHash,
		GenerationSelfTests:         generationSelfTests,
		SetupTranscript:             "not_recorded",
		ToxicWasteErasure:           "not_mechanically_verified",
	}
	provenancePath := filepath.Join(stageDir, "setup_provenance.json")
	if err := artifacts.WriteJSON(provenancePath, provenance); err != nil {
		return fmt.Errorf("write setup provenance: %w", err)
	}
	provenanceHash, err := artifacts.SHA256HexFile(provenancePath)
	if err != nil {
		return fmt.Errorf("hash setup provenance: %w", err)
	}

	metadata := artifacts.CircuitMetadataJSON{
		Schema:                      artifacts.CircuitMetadataSchema,
		Curve:                       "bls12-377",
		Circuit:                     *circuit,
		ProvingKeySize:              pkSize,
		VerifyingKeySize:            vkSize,
		SR1CSSHA256Hex:              sr1csHash,
		SetupProvenanceSHA256Hex:    provenanceHash,
		ProvingKeySHA256Hex:         pkHash,
		VerifyingKeyBinarySHA256Hex: vkBinaryHash,
		VerifyingKeyJSONSHA256Hex:   vkJSONHash,
	}
	artifacts.FillCircuitMetadataShape(&metadata, ccs)
	metadataPath := filepath.Join(stageDir, "circuit_metadata.json")
	if err := artifacts.WriteJSON(metadataPath, metadata); err != nil {
		return fmt.Errorf("write circuit metadata: %w", err)
	}

	// Publish metadata last so it marks a complete artifact set.
	publishNames := []string{"setup_provenance.json", "circuit_metadata.json"}
	if !*reuseExistingKeys {
		publishNames = []string{
			*circuit + ".sr1cs",
			"proving_key.bin",
			"verifying_key.bin",
			"verifying_key.json",
			"setup_provenance.json",
			"circuit_metadata.json",
		}
	}
	for _, name := range publishNames {
		if err := publishSetupArtifact(
			filepath.Join(stageDir, name),
			filepath.Join(*outDir, name),
		); err != nil {
			return fmt.Errorf("publish setup artifact %s: %w", name, err)
		}
	}

	fmt.Fprintf(
		os.Stderr,
		"wrote %s/%s (mode %s, compile %.2fms, setup %.2fms, generation self-tests %d, pk %d bytes, vk %d bytes)\n",
		*outDir,
		*circuit,
		mode,
		compileMS,
		setupMS,
		len(generationSelfTests),
		pkSize,
		vkSize,
	)
	return nil
}

func validateSetupSelfTestFlags(proofCases, witnessPaths []string) error {
	if len(proofCases) == 0 || len(proofCases) != len(witnessPaths) {
		return fmt.Errorf(
			"--proof-case and --witness must have equal nonzero counts",
		)
	}
	seenCases := make(map[string]struct{}, len(proofCases))
	seenWitnesses := make(map[string]struct{}, len(witnessPaths))
	for index := range proofCases {
		if !isLowerSnakeIdentifier(proofCases[index]) {
			return fmt.Errorf("invalid proof case %q", proofCases[index])
		}
		if _, exists := seenCases[proofCases[index]]; exists {
			return fmt.Errorf("duplicate proof case %q", proofCases[index])
		}
		if _, exists := seenWitnesses[witnessPaths[index]]; exists {
			return fmt.Errorf("duplicate witness path %q", witnessPaths[index])
		}
		seenCases[proofCases[index]] = struct{}{}
		seenWitnesses[witnessPaths[index]] = struct{}{}
	}
	return nil
}

func runSetupSelfTests(
	ccs constraint.ConstraintSystem,
	pk *groth16bls.ProvingKey,
	vk *groth16bls.VerifyingKey,
	circuit string,
	proofCases, witnessPaths []string,
) ([]setupGenerationSelfTest, error) {
	tests := make([]setupGenerationSelfTest, 0, len(proofCases))
	for index, proofCase := range proofCases {
		payload, err := os.ReadFile(witnessPaths[index])
		if err != nil {
			return nil, fmt.Errorf("%s: read setup witness: %w", proofCase, err)
		}
		assignment, _, err := witnessAssignment(circuit, payload)
		if err != nil {
			return nil, fmt.Errorf("%s: decode setup witness: %w", proofCase, err)
		}
		fullWitness, err := frontend.NewWitness(assignment, primitives.ScalarField())
		if err != nil {
			return nil, fmt.Errorf("%s: construct setup witness: %w", proofCase, err)
		}
		if err := ccs.IsSolved(fullWitness); err != nil {
			return nil, fmt.Errorf("%s: setup witness is unsatisfied: %w", proofCase, err)
		}
		publicWitness, err := fullWitness.Public()
		if err != nil {
			return nil, fmt.Errorf("%s: public setup witness: %w", proofCase, err)
		}
		proof, err := groth16.Prove(ccs, pk, fullWitness)
		if err != nil {
			return nil, fmt.Errorf("%s: setup self-test prove: %w", proofCase, err)
		}
		if err := groth16.Verify(proof, vk, publicWitness); err != nil {
			return nil, fmt.Errorf("%s: setup self-test verify: %w", proofCase, err)
		}
		witnessHash := sha256.Sum256(payload)
		tests = append(tests, setupGenerationSelfTest{
			ProofCase:                  proofCase,
			WitnessSHA256Hex:           fmt.Sprintf("%x", witnessHash[:]),
			ProvedAndVerifiedInProcess: true,
		})
	}
	return tests, nil
}

func filesEqual(first, second string) (bool, error) {
	firstBytes, err := os.ReadFile(first)
	if err != nil {
		return false, err
	}
	secondBytes, err := os.ReadFile(second)
	if err != nil {
		return false, err
	}
	return bytes.Equal(firstBytes, secondBytes), nil
}

func validateVerifyingKeyJSON(path string, vk *groth16bls.VerifyingKey) error {
	want, err := artifacts.EncodeCanonicalJSON(
		artifacts.EncodeVerifyingKeyJSON(vk),
	)
	if err != nil {
		return err
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	parsed, err := artifacts.DecodeCanonicalVerifyingKeyJSON(raw)
	if err != nil {
		return err
	}
	got, err := artifacts.EncodeCanonicalJSON(parsed)
	if err != nil {
		return err
	}
	if !bytes.Equal(want, got) {
		return fmt.Errorf(
			"verifying_key.json does not encode the same key as verifying_key.bin",
		)
	}
	return nil
}

func publishSetupArtifact(source, target string) error {
	return os.Rename(source, target)
}

func runProve(args []string) error {
	fs := flag.NewFlagSet("prove", flag.ContinueOnError)
	circuit := fs.String("circuit", "", "transferNxM, note reshape, or shielded-ics20-withdrawal family label")
	witnessPath := fs.String("witness", "", "witness binary path")
	artifactDir := fs.String("artifact-dir", "", "artifact directory")
	outPath := fs.String("out", "", "output artifacts JSON path")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *circuit == "" || *witnessPath == "" || *artifactDir == "" || *outPath == "" {
		return fmt.Errorf("--circuit, --witness, --artifact-dir, and --out are required")
	}

	witnessPayload, err := os.ReadFile(*witnessPath)
	if err != nil {
		return fmt.Errorf("read witness: %w", err)
	}

	ccs, compileMS, err := compileCircuit(*circuit)
	if err != nil {
		return err
	}
	metadata, err := artifacts.LoadCircuitMetadata(*artifactDir)
	if err != nil {
		return fmt.Errorf("load circuit metadata: %w", err)
	}
	if err := artifacts.ValidateCircuitMetadataForCircuit(metadata, *circuit, ccs); err != nil {
		return err
	}

	pk, loadPKMS, err := loadPK(filepath.Join(*artifactDir, "proving_key.bin"))
	if err != nil {
		return err
	}
	vk, loadVKMS, err := loadVK(filepath.Join(*artifactDir, "verifying_key.bin"))
	if err != nil {
		return err
	}

	assignment, witnessSummary, err := witnessAssignment(*circuit, witnessPayload)
	if err != nil {
		return err
	}
	fullWitness, err := frontend.NewWitness(assignment, primitives.ScalarField())
	if err != nil {
		return fmt.Errorf("full witness: %w", err)
	}
	publicWitness, err := fullWitness.Public()
	if err != nil {
		return fmt.Errorf("public witness: %w", err)
	}

	proveStart := time.Now()
	proofIface, err := groth16.Prove(ccs, pk, fullWitness)
	if err != nil {
		return fmt.Errorf("prove: %w", err)
	}
	proveMS := time.Since(proveStart).Seconds() * 1000

	verifyStart := time.Now()
	if err := groth16.Verify(proofIface, vk, publicWitness); err != nil {
		return fmt.Errorf("gnark verify: %w", err)
	}
	verifyMS := time.Since(verifyStart).Seconds() * 1000

	proof := proofIface.(*groth16bls.Proof)
	artifactJSON := artifacts.ArtifactJSON{
		Curve:                "bls12-377",
		Circuit:              *circuit,
		PublicInputs:         []string{witnessSummary.ClaimedStatementHash},
		StatementFields:      witnessSummary.StatementFields,
		ClaimedStatementHash: witnessSummary.ClaimedStatementHash,
		Proof:                artifacts.EncodeProofJSON(proof),
		VerifyingKey:         artifacts.EncodeVerifyingKeyJSON(vk),
		Timings: artifacts.TimingsJSON{
			CompileMS: compileMS,
			LoadPKMS:  loadPKMS,
			LoadVKMS:  loadVKMS,
			ProveMS:   proveMS,
			VerifyMS:  verifyMS,
		},
	}
	if err := artifacts.WriteJSON(*outPath, artifactJSON); err != nil {
		return fmt.Errorf("write artifacts: %w", err)
	}
	return nil
}

// runCheckVKJSON verifies verifying_key.json is a faithful re-encoding of
// verifying_key.bin, so the metadata pin over the JSON binds the deployed bytes.
func runCheckVKJSON(args []string) error {
	fs := flag.NewFlagSet("check-vk-json", flag.ContinueOnError)
	artifactDir := fs.String("artifact-dir", "", "artifact directory containing verifying_key.{bin,json}")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *artifactDir == "" {
		return fmt.Errorf("check-vk-json: --artifact-dir required")
	}
	vk, _, err := loadVK(filepath.Join(*artifactDir, "verifying_key.bin"))
	if err != nil {
		return err
	}
	want, err := artifacts.EncodeCanonicalJSON(artifacts.EncodeVerifyingKeyJSON(vk))
	if err != nil {
		return err
	}
	raw, err := os.ReadFile(filepath.Join(*artifactDir, "verifying_key.json"))
	if err != nil {
		return err
	}
	parsed, err := artifacts.DecodeCanonicalVerifyingKeyJSON(raw)
	if err != nil {
		return fmt.Errorf("parse verifying_key.json: %w", err)
	}
	got, err := artifacts.EncodeCanonicalJSON(parsed)
	if err != nil {
		return err
	}
	if !bytes.Equal(want, got) {
		return fmt.Errorf("verifying_key.json does not encode the same key as verifying_key.bin")
	}
	fmt.Println("verifying_key.json matches verifying_key.bin")
	return nil
}

func runReplay(args []string) error {
	fs := flag.NewFlagSet("replay", flag.ContinueOnError)
	circuit := fs.String("circuit", "transfer", "transfer, note reshape, or shielded-ics20-withdrawal family label")
	witnessPath := fs.String("witness", "", "witness binary path")
	artifactDir := fs.String("artifact-dir", "", "artifact directory for prove mode")
	mode := fs.String("mode", "decode", "decode, solve, or prove")
	rawOut := fs.String("raw-out", "", "optional file for raw dump")
	assignmentOut := fs.String("assignment-out", "", "optional file for assignment dump")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *witnessPath == "" {
		return fmt.Errorf("--witness is required")
	}

	payload, err := os.ReadFile(*witnessPath)
	if err != nil {
		return fmt.Errorf("read witness: %w", err)
	}
	switch *circuit {
	default:
		if *circuit != "note_seizure" {
			if _, ok := generated.TransferFamilyByLabel(*circuit); !ok {
				if _, ok := generated.NoteReshapeFamilyByLabel(*circuit); !ok {
					if _, ok := generated.ShieldedIcs20WithdrawalFamilyByLabel(*circuit); !ok {
						return fmt.Errorf("unsupported --circuit %q", *circuit)
					}
				}
			}
		}
		if *rawOut != "" {
			return fmt.Errorf("--raw-out is not implemented yet for --circuit %s", *circuit)
		}
		assignment, _, err := witnessAssignment(*circuit, payload)
		if err != nil {
			return err
		}
		if err := writeOrStdout(*assignmentOut, fmt.Sprintf("%#v\n", assignment)); err != nil {
			return err
		}
	}
	if *mode == "decode" {
		return nil
	}
	if *mode != "solve" && *mode != "prove" {
		return fmt.Errorf("unsupported --mode %q", *mode)
	}

	compileStart := time.Now()
	var (
		assignment frontend.Circuit
		ccs        constraint.ConstraintSystem
	)
	switch *circuit {
	default:
		if *circuit == "note_seizure" {
			assignment, err = abi.NewNoteSeizureCircuitAssignmentFromWitness(payload)
			if err != nil {
				return err
			}
			ccs, err = frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, circuits.NewNoteSeizureCircuit())
			break
		}
		if _, ok := generated.TransferFamilyByLabel(*circuit); ok {
			assignment, _, err = abi.NewTransferCircuitAssignmentFromWitness(payload)
			if err != nil {
				return err
			}
			ccs, err = frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, circuits.NewTransferCircuit())
			break
		}
		if family, ok := generated.NoteReshapeFamilyByLabel(*circuit); ok {
			assignment, _, err = abi.NewNoteReshapeCircuitAssignmentFromWitness(payload)
			if err != nil {
				return err
			}
			ccs, err = frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, circuits.NewNoteReshapeCircuit(family.Label, family.NIn, family.NOut))
			break
		}
		if family, ok := generated.ShieldedIcs20WithdrawalFamilyByLabel(*circuit); ok {
			assignment, _, err = abi.NewShieldedIcs20WithdrawalCircuitAssignmentFromWitness(payload)
			if err != nil {
				return err
			}
			ccs, err = frontend.Compile(
				primitives.ScalarField(),
				r1cs.NewBuilder,
				circuits.NewShieldedIcs20WithdrawalCircuit(family.NIn),
			)
			break
		}
		return fmt.Errorf("unsupported --circuit %q", *circuit)
	}
	if err != nil {
		return err
	}
	compileMS := time.Since(compileStart).Seconds() * 1000
	fullWitness, err := frontend.NewWitness(assignment, primitives.ScalarField())
	if err != nil {
		return err
	}
	solveStart := time.Now()
	if err := ccs.IsSolved(fullWitness); err != nil {
		return fmt.Errorf("solve failed after %.2fms: %w", time.Since(solveStart).Seconds()*1000, err)
	}
	if *mode == "solve" {
		fmt.Fprintf(os.Stderr, "solve ok (compile %.2fms, solve %.2fms)\n", compileMS, time.Since(solveStart).Seconds()*1000)
		return nil
	}
	if *artifactDir == "" {
		return fmt.Errorf("--artifact-dir is required for --mode prove")
	}

	pk, _, err := loadPK(filepath.Join(*artifactDir, "proving_key.bin"))
	if err != nil {
		return err
	}
	vk, _, err := loadVK(filepath.Join(*artifactDir, "verifying_key.bin"))
	if err != nil {
		return err
	}
	publicWitness, err := fullWitness.Public()
	if err != nil {
		return err
	}
	proofIface, err := groth16.Prove(ccs, pk, fullWitness)
	if err != nil {
		return err
	}
	if err := groth16.Verify(proofIface, vk, publicWitness); err != nil {
		return err
	}
	return nil
}

func runVerifyBench(args []string) error {
	fs := flag.NewFlagSet("verify-bench", flag.ContinueOnError)
	artifactPath := fs.String("artifacts", "", "path to proof artifact JSON")
	outPath := fs.String("out", "", "output verifier benchmark JSON path")
	warmupIterations := fs.Int("warmup-iterations", 3, "number of untimed verify warmup iterations")
	measuredIterations := fs.Int("measured-iterations", 20, "number of measured verify iterations")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *artifactPath == "" || *outPath == "" {
		return fmt.Errorf("--artifacts and --out are required")
	}
	if *warmupIterations < 0 || *measuredIterations <= 0 {
		return fmt.Errorf("--warmup-iterations must be >= 0 and --measured-iterations must be > 0")
	}

	loadStart := time.Now()
	artifactsJSON, err := artifacts.LoadArtifactJSON(*artifactPath)
	if err != nil {
		return err
	}
	if artifactsJSON.Curve != "bls12-377" {
		return fmt.Errorf("unexpected curve %q", artifactsJSON.Curve)
	}
	if len(artifactsJSON.PublicInputs) != 1 {
		return fmt.Errorf("expected exactly one public input, got %d", len(artifactsJSON.PublicInputs))
	}
	if artifactsJSON.PublicInputs[0] != artifactsJSON.ClaimedStatementHash {
		return fmt.Errorf("public input does not match claimed statement hash")
	}

	proof, err := decodeProof(artifactsJSON.Proof)
	if err != nil {
		return fmt.Errorf("decode proof: %w", err)
	}
	vk, err := decodeVerifyingKey(artifactsJSON.VerifyingKey)
	if err != nil {
		return fmt.Errorf("decode verifying key: %w", err)
	}
	publicWitness, err := buildPublicWitness(artifactsJSON.PublicInputs)
	if err != nil {
		return fmt.Errorf("build public witness: %w", err)
	}
	loadOrDecodeMS := time.Since(loadStart).Seconds() * 1000

	prepareStart := time.Now()
	if err := vk.Precompute(); err != nil {
		return fmt.Errorf("precompute verifying key: %w", err)
	}
	prepareMS := time.Since(prepareStart).Seconds() * 1000

	for i := 0; i < *warmupIterations; i++ {
		if err := groth16.Verify(proof, vk, publicWitness); err != nil {
			return fmt.Errorf("warmup verify %d failed: %w", i, err)
		}
	}

	verifySamples := make([]float64, 0, *measuredIterations)
	for i := 0; i < *measuredIterations; i++ {
		verifyStart := time.Now()
		if err := groth16.Verify(proof, vk, publicWitness); err != nil {
			return fmt.Errorf("measured verify %d failed: %w", i, err)
		}
		verifySamples = append(verifySamples, time.Since(verifyStart).Seconds()*1000)
	}
	verifyMeanMS, verifyMedianMS, verifyMinMS, verifyMaxMS := artifacts.ComputeDurationStats(verifySamples)

	report := artifacts.VerifyBenchResultJSON{
		Curve:                artifactsJSON.Curve,
		Circuit:              artifactsJSON.Circuit,
		ClaimedStatementHash: artifactsJSON.ClaimedStatementHash,
		LoadOrDecodeMS:       loadOrDecodeMS,
		PrepareMS:            prepareMS,
		VerifyWarmupIters:    *warmupIterations,
		VerifyMeasuredIters:  *measuredIterations,
		VerifyMeanMS:         verifyMeanMS,
		VerifyMedianMS:       verifyMedianMS,
		VerifyMinMS:          verifyMinMS,
		VerifyMaxMS:          verifyMaxMS,
	}
	return artifacts.WriteJSON(*outPath, &report)
}

func compileCircuit(circuit string) (constraint.ConstraintSystem, float64, error) {
	started := time.Now()
	var instance frontend.Circuit
	if _, ok := generated.TransferFamilyByLabel(circuit); ok {
		instance = circuits.NewTransferCircuit()
	} else if family, ok := generated.NoteReshapeFamilyByLabel(circuit); ok {
		instance = circuits.NewNoteReshapeCircuit(family.Label, family.NIn, family.NOut)
	} else if family, ok := generated.ShieldedIcs20WithdrawalFamilyByLabel(circuit); ok {
		instance = circuits.NewShieldedIcs20WithdrawalCircuit(family.NIn)
	} else if circuit == "note_seizure" {
		instance = circuits.NewNoteSeizureCircuit()
	} else {
		return nil, 0, fmt.Errorf("unsupported circuit %q", circuit)
	}
	ccs, err := frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, instance)
	return ccs, time.Since(started).Seconds() * 1000, err
}

func witnessAssignment(circuit string, witnessPayload []byte) (frontend.Circuit, witnessSummary, error) {
	switch circuit {
	case "note_seizure":
		decoded, err := abi.DecodeNoteSeizureWitness(witnessPayload)
		if err != nil {
			return nil, witnessSummary{}, err
		}
		statementFields, err := abi.ReconstructedNoteSeizureStatementFieldsFromWitness(decoded)
		if err != nil {
			return nil, witnessSummary{}, err
		}
		assignment, err := abi.NewNoteSeizureCircuitAssignmentFromWitness(witnessPayload)
		return assignment, witnessSummary{
			ClaimedStatementHash: primitives.LittleEndianBytesToBigInt(decoded.ClaimedStatementHash[:]).String(),
			StatementFields:      vec32Strings(statementFields),
		}, err
	default:
		if _, ok := generated.TransferFamilyByLabel(circuit); ok {
			decoded, _, err := abi.DecodeTransferWitness(witnessPayload)
			if err != nil {
				return nil, witnessSummary{}, err
			}
			statementFields, err := abi.ReconstructedTransferStatementFieldsFromWitness(decoded)
			if err != nil {
				return nil, witnessSummary{}, err
			}
			assignment, _, err := abi.NewTransferCircuitAssignmentFromWitness(witnessPayload)
			return assignment, witnessSummary{
				ClaimedStatementHash: primitives.LittleEndianBytesToBigInt(decoded.ClaimedStatementHash[:]).String(),
				StatementFields:      vec32Strings(statementFields),
			}, err
		}
		if _, ok := generated.NoteReshapeFamilyByLabel(circuit); ok {
			decoded, _, err := abi.DecodeNoteReshapeWitness(witnessPayload)
			if err != nil {
				return nil, witnessSummary{}, err
			}
			statementFields, err := abi.ReconstructedNoteReshapeStatementFieldsFromWitness(decoded)
			if err != nil {
				return nil, witnessSummary{}, err
			}
			assignment, _, err := abi.NewNoteReshapeCircuitAssignmentFromWitness(witnessPayload)
			return assignment, witnessSummary{
				ClaimedStatementHash: primitives.LittleEndianBytesToBigInt(decoded.ClaimedStatementHash[:]).String(),
				StatementFields:      vec32Strings(statementFields),
			}, err
		}
		if _, ok := generated.ShieldedIcs20WithdrawalFamilyByLabel(circuit); ok {
			decoded, _, err := abi.DecodeShieldedIcs20WithdrawalWitness(witnessPayload)
			if err != nil {
				return nil, witnessSummary{}, err
			}
			statementFields, err := abi.ReconstructedShieldedIcs20WithdrawalStatementFieldsFromWitness(decoded)
			if err != nil {
				return nil, witnessSummary{}, err
			}
			assignment, _, err := abi.NewShieldedIcs20WithdrawalCircuitAssignmentFromWitness(witnessPayload)
			return assignment, witnessSummary{
				ClaimedStatementHash: primitives.LittleEndianBytesToBigInt(decoded.ClaimedStatementHash[:]).String(),
				StatementFields:      vec32Strings(statementFields),
			}, err
		}
		return nil, witnessSummary{}, fmt.Errorf("unsupported circuit %q", circuit)
	}
}

func vec32Strings(fields [][32]byte) []string {
	values := make([]string, len(fields))
	for i := range fields {
		values[i] = primitives.LittleEndianBytesToBigInt(fields[i][:]).String()
	}
	return values
}

type witnessSummary struct {
	ClaimedStatementHash string
	StatementFields      []string
}

func writePK(path string, pk *groth16bls.ProvingKey) error {
	file, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("create proving key file: %w", err)
	}
	defer file.Close()
	if _, err := pk.WriteTo(file); err != nil {
		return fmt.Errorf("write proving key: %w", err)
	}
	return nil
}

func writeVK(path string, vk *groth16bls.VerifyingKey) error {
	file, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("create verifying key file: %w", err)
	}
	defer file.Close()
	if _, err := vk.WriteTo(file); err != nil {
		return fmt.Errorf("write verifying key: %w", err)
	}
	return nil
}

func loadPK(path string) (*groth16bls.ProvingKey, float64, error) {
	start := time.Now()
	pk, err := artifacts.LoadProvingKeyStrict(path)
	if err != nil {
		return nil, 0, err
	}
	return pk, time.Since(start).Seconds() * 1000, nil
}

func loadVK(path string) (*groth16bls.VerifyingKey, float64, error) {
	start := time.Now()
	vk, err := artifacts.LoadVerifyingKeyStrict(path)
	if err != nil {
		return nil, 0, err
	}
	return vk, time.Since(start).Seconds() * 1000, nil
}

func writeOrStdout(path string, contents string) error {
	if path == "" {
		fmt.Print(contents)
		return nil
	}
	return os.WriteFile(path, []byte(contents), 0o644)
}

func buildPublicWitness(publicInputs []string) (backendwitness.Witness, error) {
	publicWitness, err := backendwitness.New(primitives.ScalarField())
	if err != nil {
		return nil, err
	}
	values := make(chan any, len(publicInputs))
	for _, input := range publicInputs {
		value, ok := new(big.Int).SetString(input, 10)
		if !ok {
			return nil, fmt.Errorf("invalid public input %q", input)
		}
		values <- value
	}
	close(values)
	if err := publicWitness.Fill(len(publicInputs), 0, values); err != nil {
		return nil, err
	}
	return publicWitness, nil
}

func decodeProof(proofJSON artifacts.ProofJSON) (*groth16bls.Proof, error) {
	proof := new(groth16bls.Proof)
	if err := setG1Affine(&proof.Ar, proofJSON.A); err != nil {
		return nil, fmt.Errorf("proof.a: %w", err)
	}
	if err := setG2Affine(&proof.Bs, proofJSON.B); err != nil {
		return nil, fmt.Errorf("proof.b: %w", err)
	}
	if err := setG1Affine(&proof.Krs, proofJSON.C); err != nil {
		return nil, fmt.Errorf("proof.c: %w", err)
	}
	if !proof.Ar.IsOnCurve() || !proof.Ar.IsInSubGroup() {
		return nil, fmt.Errorf("proof.a is invalid")
	}
	if !proof.Bs.IsOnCurve() || !proof.Bs.IsInSubGroup() {
		return nil, fmt.Errorf("proof.b is invalid")
	}
	if !proof.Krs.IsOnCurve() || !proof.Krs.IsInSubGroup() {
		return nil, fmt.Errorf("proof.c is invalid")
	}
	return proof, nil
}

func decodeVerifyingKey(vkJSON artifacts.VerifyingKeyJSON) (*groth16bls.VerifyingKey, error) {
	vk := new(groth16bls.VerifyingKey)
	if err := setG1Affine(&vk.G1.Alpha, vkJSON.AlphaG1); err != nil {
		return nil, fmt.Errorf("alpha_g1: %w", err)
	}
	if err := setG2Affine(&vk.G2.Beta, vkJSON.BetaG2); err != nil {
		return nil, fmt.Errorf("beta_g2: %w", err)
	}
	if err := setG2Affine(&vk.G2.Gamma, vkJSON.GammaG2); err != nil {
		return nil, fmt.Errorf("gamma_g2: %w", err)
	}
	if err := setG2Affine(&vk.G2.Delta, vkJSON.DeltaG2); err != nil {
		return nil, fmt.Errorf("delta_g2: %w", err)
	}
	vk.G1.K = make([]curve.G1Affine, len(vkJSON.GammaABCG1))
	for i := range vkJSON.GammaABCG1 {
		if err := setG1Affine(&vk.G1.K[i], vkJSON.GammaABCG1[i]); err != nil {
			return nil, fmt.Errorf("gamma_abc_g1[%d]: %w", i, err)
		}
	}
	return vk, nil
}

func setG1Affine(dst *curve.G1Affine, point artifacts.G1PointJSON) error {
	if _, err := dst.X.SetString(point.X); err != nil {
		return err
	}
	if _, err := dst.Y.SetString(point.Y); err != nil {
		return err
	}
	return nil
}

func setG2Affine(dst *curve.G2Affine, point artifacts.G2PointJSON) error {
	if _, err := dst.X.A0.SetString(point.X.A0); err != nil {
		return fmt.Errorf("x.a0: %w", err)
	}
	if _, err := dst.X.A1.SetString(point.X.A1); err != nil {
		return fmt.Errorf("x.a1: %w", err)
	}
	if _, err := dst.Y.A0.SetString(point.Y.A0); err != nil {
		return fmt.Errorf("y.a0: %w", err)
	}
	if _, err := dst.Y.A1.SetString(point.Y.A1); err != nil {
		return fmt.Errorf("y.a1: %w", err)
	}
	return nil
}
