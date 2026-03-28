package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"

	prototype "github.com/penumbra-zone/penumbra/tools/gnark-spend-prototype"
)

func main() {
	witnessPath := flag.String("witness", "", "SpendWitnessV1 binary path")
	artifactDir := flag.String("artifact-dir", "", "directory containing proving_key.bin and verifying_key.bin for prove mode")
	mode := flag.String("mode", "decode", "decode, solve, or prove")
	rawOut := flag.String("raw-out", "", "optional file for raw decoded witness dump")
	assignmentOut := flag.String("assignment-out", "", "optional file for assignment dump")
	crosscheckOut := flag.String("crosscheck-out", "", "optional file for derived-value cross-checks")
	flag.Parse()

	if *witnessPath == "" {
		fmt.Fprintln(os.Stderr, "--witness is required")
		os.Exit(2)
	}

	payload, err := os.ReadFile(*witnessPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "read witness: %v\n", err)
		os.Exit(1)
	}

	rawDump, err := prototype.DecodeSpendWitnessRawDumpV1(payload)
	if err != nil {
		fmt.Fprintf(os.Stderr, "decode raw witness dump: %v\n", err)
		os.Exit(1)
	}
	if err := writeOrStdout(*rawOut, rawDump); err != nil {
		fmt.Fprintf(os.Stderr, "write raw dump: %v\n", err)
		os.Exit(1)
	}

	assignmentDump, err := prototype.DumpSpendCircuitAssignmentFromWitnessV1(payload)
	if err != nil {
		fmt.Fprintf(os.Stderr, "dump assignment: %v\n", err)
		os.Exit(1)
	}
	if err := writeOrStdout(*assignmentOut, assignmentDump); err != nil {
		fmt.Fprintf(os.Stderr, "write assignment dump: %v\n", err)
		os.Exit(1)
	}

	crosscheckDump, err := prototype.CrossCheckRandomizedVerificationKeyWitnessV1(payload)
	if err != nil {
		fmt.Fprintf(os.Stderr, "cross-check randomized verification key: %v\n", err)
		os.Exit(1)
	}
	if err := writeOrStdout(*crosscheckOut, crosscheckDump); err != nil {
		fmt.Fprintf(os.Stderr, "write cross-check dump: %v\n", err)
		os.Exit(1)
	}

	switch *mode {
	case "decode":
		return
	case "solve", "prove":
	default:
		fmt.Fprintf(os.Stderr, "unsupported --mode %q\n", *mode)
		os.Exit(2)
	}

	assignment, err := prototype.NewSpendCircuitAssignmentFromWitnessV1(payload)
	if err != nil {
		fmt.Fprintf(os.Stderr, "build assignment from witness: %v\n", err)
		os.Exit(1)
	}

	compileStart := time.Now()
	ccs, err := frontend.Compile(prototype.ScalarField(), r1cs.NewBuilder, &prototype.SpendCircuit{})
	if err != nil {
		fmt.Fprintf(os.Stderr, "compile circuit: %v\n", err)
		os.Exit(1)
	}
	compileMS := time.Since(compileStart).Seconds() * 1000

	fullWitness, err := frontend.NewWitness(assignment, prototype.ScalarField())
	if err != nil {
		fmt.Fprintf(os.Stderr, "full witness: %v\n", err)
		os.Exit(1)
	}

	solveStart := time.Now()
	if err := ccs.IsSolved(fullWitness); err != nil {
		fmt.Fprintf(os.Stderr, "solve failed after %.2fms: %v\n", time.Since(solveStart).Seconds()*1000, err)
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "solve ok (compile %.2fms, solve %.2fms)\n", compileMS, time.Since(solveStart).Seconds()*1000)

	if *mode == "solve" {
		return
	}

	if *artifactDir == "" {
		fmt.Fprintln(os.Stderr, "--artifact-dir is required for --mode prove")
		os.Exit(2)
	}

	pkFile, err := os.Open(filepath.Join(*artifactDir, "proving_key.bin"))
	if err != nil {
		fmt.Fprintf(os.Stderr, "open proving key: %v\n", err)
		os.Exit(1)
	}
	defer pkFile.Close()
	pk := new(groth16bls.ProvingKey)
	if _, err := pk.ReadFrom(pkFile); err != nil {
		fmt.Fprintf(os.Stderr, "read proving key: %v\n", err)
		os.Exit(1)
	}

	vkFile, err := os.Open(filepath.Join(*artifactDir, "verifying_key.bin"))
	if err != nil {
		fmt.Fprintf(os.Stderr, "open verifying key: %v\n", err)
		os.Exit(1)
	}
	defer vkFile.Close()
	vk := new(groth16bls.VerifyingKey)
	if _, err := vk.ReadFrom(vkFile); err != nil {
		fmt.Fprintf(os.Stderr, "read verifying key: %v\n", err)
		os.Exit(1)
	}

	publicWitness, err := fullWitness.Public()
	if err != nil {
		fmt.Fprintf(os.Stderr, "public witness: %v\n", err)
		os.Exit(1)
	}

	proveStart := time.Now()
	proofIface, err := groth16.Prove(ccs, pk, fullWitness)
	if err != nil {
		fmt.Fprintf(os.Stderr, "prove failed after %.2fms: %v\n", time.Since(proveStart).Seconds()*1000, err)
		os.Exit(1)
	}
	verifyStart := time.Now()
	if err := groth16.Verify(proofIface, vk, publicWitness); err != nil {
		fmt.Fprintf(os.Stderr, "gnark verify failed after %.2fms: %v\n", time.Since(verifyStart).Seconds()*1000, err)
		os.Exit(1)
	}
	fmt.Fprintf(
		os.Stderr,
		"prove ok (compile %.2fms, solve %.2fms, prove %.2fms, verify %.2fms)\n",
		compileMS,
		time.Since(solveStart).Seconds()*1000,
		time.Since(proveStart).Seconds()*1000,
		time.Since(verifyStart).Seconds()*1000,
	)
}

func writeOrStdout(path string, contents string) error {
	if path == "" {
		fmt.Print(contents)
		return nil
	}
	return os.WriteFile(path, []byte(contents), 0o644)
}
