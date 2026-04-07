package main

import (
	"flag"
	"fmt"
	"math/big"
	"os"
	"path/filepath"
	"time"

	curve "github.com/consensys/gnark-crypto/ecc/bls12-377"
	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	backendwitness "github.com/consensys/gnark/backend/witness"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"

	"github.com/penumbra-zone/penumbra/tools/gnark/internal/abi"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/artifacts"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/circuits"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/generated"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
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
	case "prove":
		err = runProve(os.Args[2:])
	case "replay":
		err = runReplay(os.Args[2:])
	case "verify-bench":
		err = runVerifyBench(os.Args[2:])
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
	fmt.Fprintln(os.Stderr, "usage: gnarkctl <setup|prove|replay|verify-bench> [flags]")
}

func runSetup(args []string) error {
	fs := flag.NewFlagSet("setup", flag.ContinueOnError)
	circuit := fs.String("circuit", "", "spend, output, or transferNxM family label")
	outDir := fs.String("out-dir", "", "output directory")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *circuit == "" || *outDir == "" {
		return fmt.Errorf("--circuit and --out-dir are required")
	}
	if err := os.MkdirAll(*outDir, 0o755); err != nil {
		return fmt.Errorf("create output dir: %w", err)
	}

	ccs, compileMS, err := compileCircuit(*circuit)
	if err != nil {
		return err
	}

	setupStart := time.Now()
	pkIface, vkIface, err := groth16.Setup(ccs)
	if err != nil {
		return fmt.Errorf("setup: %w", err)
	}
	setupMS := time.Since(setupStart).Seconds() * 1000

	pk := pkIface.(*groth16bls.ProvingKey)
	vk := vkIface.(*groth16bls.VerifyingKey)

	pkPath := filepath.Join(*outDir, "proving_key.bin")
	if err := writePK(pkPath, pk); err != nil {
		return err
	}
	vkPath := filepath.Join(*outDir, "verifying_key.bin")
	if err := writeVK(vkPath, vk); err != nil {
		return err
	}
	vkJSONPath := filepath.Join(*outDir, "verifying_key.json")
	if err := artifacts.WriteJSON(vkJSONPath, artifacts.EncodeVerifyingKeyJSON(vk)); err != nil {
		return fmt.Errorf("write verifying key json: %w", err)
	}

	pkSize, err := artifacts.FileSize(pkPath)
	if err != nil {
		return fmt.Errorf("stat proving key: %w", err)
	}
	vkSize, err := artifacts.FileSize(vkPath)
	if err != nil {
		return fmt.Errorf("stat verifying key: %w", err)
	}

	metadata := artifacts.CircuitMetadataJSON{
		Curve:            "bls12-377",
		Circuit:          *circuit,
		CompileMS:        compileMS,
		SetupMS:          setupMS,
		ProvingKeySize:   pkSize,
		VerifyingKeySize: vkSize,
	}
	artifacts.FillCircuitMetadataShape(&metadata, ccs)
	metadata.ProvingKeySHA256Hex, err = artifacts.SHA256HexFile(pkPath)
	if err != nil {
		return fmt.Errorf("hash proving key: %w", err)
	}
	metadata.VerifyingKeySHA256Hex, err = artifacts.SHA256HexFile(vkJSONPath)
	if err != nil {
		return fmt.Errorf("hash verifying key json: %w", err)
	}
	if err := artifacts.WriteJSON(filepath.Join(*outDir, "circuit_metadata.json"), metadata); err != nil {
		return fmt.Errorf("write circuit metadata: %w", err)
	}

	fmt.Fprintf(
		os.Stderr,
		"wrote %s/%s (compile %.2fms, setup %.2fms, pk %d bytes, vk %d bytes)\n",
		*outDir,
		*circuit,
		compileMS,
		setupMS,
		pkSize,
		vkSize,
	)
	return nil
}

func runProve(args []string) error {
	fs := flag.NewFlagSet("prove", flag.ContinueOnError)
	circuit := fs.String("circuit", "", "spend, output, or transferNxM family label")
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

func runReplay(args []string) error {
	fs := flag.NewFlagSet("replay", flag.ContinueOnError)
	circuit := fs.String("circuit", "spend", "spend, output, or transferNxM family label")
	witnessPath := fs.String("witness", "", "witness binary path")
	artifactDir := fs.String("artifact-dir", "", "artifact directory for prove mode")
	mode := fs.String("mode", "decode", "decode, solve, or prove")
	rawOut := fs.String("raw-out", "", "optional file for raw dump")
	assignmentOut := fs.String("assignment-out", "", "optional file for assignment dump")
	crosscheckOut := fs.String("crosscheck-out", "", "optional file for spend-only crosschecks")
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
	case "spend":
		rawDump, err := abi.DecodeSpendWitnessRawDumpV1(payload)
		if err != nil {
			return err
		}
		if err := writeOrStdout(*rawOut, rawDump); err != nil {
			return err
		}
		assignmentDump, err := abi.DumpSpendCircuitAssignmentFromWitnessV1(payload)
		if err != nil {
			return err
		}
		if err := writeOrStdout(*assignmentOut, assignmentDump); err != nil {
			return err
		}
		crosscheckDump, err := abi.CrossCheckRandomizedVerificationKeyWitnessV1(payload)
		if err != nil {
			return err
		}
		if err := writeOrStdout(*crosscheckOut, crosscheckDump); err != nil {
			return err
		}
	case "output":
		rawDump, err := abi.DecodeOutputWitnessRawDumpV1(payload)
		if err != nil {
			return err
		}
		if err := writeOrStdout(*rawOut, rawDump); err != nil {
			return err
		}
		assignmentDump, err := abi.DumpOutputCircuitAssignmentFromWitnessV1(payload)
		if err != nil {
			return err
		}
		if err := writeOrStdout(*assignmentOut, assignmentDump); err != nil {
			return err
		}
		if *crosscheckOut != "" {
			return fmt.Errorf("--crosscheck-out is only supported for --circuit spend")
		}
	default:
		if _, ok := generated.TransferFamilyByLabel(*circuit); !ok {
			return fmt.Errorf("unsupported --circuit %q", *circuit)
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
		if *crosscheckOut != "" {
			return fmt.Errorf("--crosscheck-out is only supported for --circuit spend")
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
	case "spend":
		assignment, err = abi.NewSpendCircuitAssignmentFromWitnessV1(payload)
		if err != nil {
			return err
		}
		ccs, err = frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, &circuits.SpendCircuit{})
	case "output":
		assignment, err = abi.NewOutputCircuitAssignmentFromWitnessV1(payload)
		if err != nil {
			return err
		}
		ccs, err = frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, &circuits.OutputCircuit{})
	default:
		if family, ok := generated.TransferFamilyByLabel(*circuit); ok {
			assignment, _, err = abi.NewTransferCircuitAssignmentFromWitnessV1(payload)
			if err != nil {
				return err
			}
			ccs, err = frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, circuits.NewTransferCircuit(family.NIn, family.NOut))
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
	compileStart := time.Now()
	switch circuit {
	case "spend":
		ccs, err := frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, &circuits.SpendCircuit{})
		return ccs, time.Since(compileStart).Seconds() * 1000, err
	case "output":
		ccs, err := frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, &circuits.OutputCircuit{})
		return ccs, time.Since(compileStart).Seconds() * 1000, err
	default:
		if family, ok := generated.TransferFamilyByLabel(circuit); ok {
			ccs, err := frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, circuits.NewTransferCircuit(family.NIn, family.NOut))
			return ccs, time.Since(compileStart).Seconds() * 1000, err
		}
		return nil, 0, fmt.Errorf("unsupported circuit %q", circuit)
	}
}

func witnessAssignment(circuit string, witnessPayload []byte) (frontend.Circuit, witnessSummary, error) {
	switch circuit {
	case "spend":
		decoded, err := abi.DecodeSpendWitnessV1(witnessPayload)
		if err != nil {
			return nil, witnessSummary{}, err
		}
		assignment, err := abi.NewSpendCircuitAssignmentFromWitnessV1(witnessPayload)
		return assignment, witnessSummary{
			ClaimedStatementHash: primitives.LittleEndianBytesToBigInt(decoded.ClaimedStatementHash[:]).String(),
			StatementFields:      vec32Strings(decoded.StatementFields),
		}, err
	case "output":
		decoded, err := abi.DecodeOutputWitnessV1(witnessPayload)
		if err != nil {
			return nil, witnessSummary{}, err
		}
		assignment, err := abi.NewOutputCircuitAssignmentFromWitnessV1(witnessPayload)
		return assignment, witnessSummary{
			ClaimedStatementHash: primitives.LittleEndianBytesToBigInt(decoded.ClaimedStatementHash[:]).String(),
			StatementFields:      vec32Strings(decoded.StatementFields),
		}, err
	default:
		if _, ok := generated.TransferFamilyByLabel(circuit); ok {
			decoded, _, err := abi.DecodeTransferWitnessV1(witnessPayload)
			if err != nil {
				return nil, witnessSummary{}, err
			}
			assignment, _, err := abi.NewTransferCircuitAssignmentFromWitnessV1(witnessPayload)
			return assignment, witnessSummary{
				ClaimedStatementHash: primitives.LittleEndianBytesToBigInt(decoded.ClaimedStatementHash[:]).String(),
				StatementFields:      vec32Strings(decoded.StatementFields),
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
	file, err := os.Open(path)
	if err != nil {
		return nil, 0, fmt.Errorf("open proving key: %w", err)
	}
	defer file.Close()
	pk := new(groth16bls.ProvingKey)
	start := time.Now()
	if _, err := pk.ReadFrom(file); err != nil {
		return nil, 0, fmt.Errorf("read proving key: %w", err)
	}
	return pk, time.Since(start).Seconds() * 1000, nil
}

func loadVK(path string) (*groth16bls.VerifyingKey, float64, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, 0, fmt.Errorf("open verifying key: %w", err)
	}
	defer file.Close()
	vk := new(groth16bls.VerifyingKey)
	start := time.Now()
	if _, err := vk.ReadFrom(file); err != nil {
		return nil, 0, fmt.Errorf("read verifying key: %w", err)
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
