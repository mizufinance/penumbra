package main

import (
	"encoding/json"
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
	artifactDir := flag.String("artifact-dir", "", "directory containing proving_key.bin and verifying_key.bin/json")
	outPath := flag.String("out", "", "output artifacts JSON path")
	flag.Parse()
	if *witnessPath == "" || *artifactDir == "" || *outPath == "" {
		fmt.Fprintln(os.Stderr, "--witness, --artifact-dir, and --out are required")
		os.Exit(2)
	}

	witnessPayload, err := os.ReadFile(*witnessPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "read witness: %v\n", err)
		os.Exit(1)
	}
	summary, err := prototype.DecodeSpendWitnessSummaryV1(witnessPayload)
	if err != nil {
		fmt.Fprintf(os.Stderr, "decode witness summary: %v\n", err)
		os.Exit(1)
	}
	assignment, err := prototype.NewSpendCircuitAssignmentFromWitnessV1(witnessPayload)
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
	metadata, err := prototype.LoadCircuitMetadata(*artifactDir)
	if err != nil {
		fmt.Fprintf(os.Stderr, "load circuit metadata: %v\n", err)
		os.Exit(1)
	}
	if err := prototype.ValidateCircuitMetadata(metadata, ccs); err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}

	pkPath := filepath.Join(*artifactDir, "proving_key.bin")
	pkFile, err := os.Open(pkPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "open proving key: %v\n", err)
		os.Exit(1)
	}
	defer pkFile.Close()
	pk := new(groth16bls.ProvingKey)
	loadPKStart := time.Now()
	if _, err := pk.ReadFrom(pkFile); err != nil {
		fmt.Fprintf(os.Stderr, "read proving key: %v\n", err)
		os.Exit(1)
	}
	loadPKMS := time.Since(loadPKStart).Seconds() * 1000

	vkPath := filepath.Join(*artifactDir, "verifying_key.bin")
	vkFile, err := os.Open(vkPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "open verifying key: %v\n", err)
		os.Exit(1)
	}
	defer vkFile.Close()
	vk := new(groth16bls.VerifyingKey)
	loadVKStart := time.Now()
	if _, err := vk.ReadFrom(vkFile); err != nil {
		fmt.Fprintf(os.Stderr, "read verifying key: %v\n", err)
		os.Exit(1)
	}
	loadVKMS := time.Since(loadVKStart).Seconds() * 1000

	fullWitness, err := frontend.NewWitness(assignment, prototype.ScalarField())
	if err != nil {
		fmt.Fprintf(os.Stderr, "full witness: %v\n", err)
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
		fmt.Fprintf(os.Stderr, "prove: %v\n", err)
		os.Exit(1)
	}
	proveMS := time.Since(proveStart).Seconds() * 1000

	verifyStart := time.Now()
	if err := groth16.Verify(proofIface, vk, publicWitness); err != nil {
		fmt.Fprintf(os.Stderr, "gnark verify: %v\n", err)
		os.Exit(1)
	}
	verifyMS := time.Since(verifyStart).Seconds() * 1000

	proof, ok := proofIface.(*groth16bls.Proof)
	if !ok {
		fmt.Fprintf(os.Stderr, "unexpected proof type %T\n", proofIface)
		os.Exit(1)
	}

	artifacts := prototype.ArtifactJSON{
		Curve:                "bls12-377",
		Circuit:              "spend",
		PublicInputs:         []string{summary.ClaimedStatementHash},
		StatementFields:      summary.StatementFields,
		ClaimedStatementHash: summary.ClaimedStatementHash,
		Proof:                prototype.EncodeProofJSON(proof),
		VerifyingKey:         prototype.EncodeVerifyingKeyJSON(vk),
		Timings: prototype.TimingsJSON{
			CompileMS: compileMS,
			LoadPKMS:  loadPKMS,
			LoadVKMS:  loadVKMS,
			ProveMS:   proveMS,
			VerifyMS:  verifyMS,
		},
	}

	file, err := os.Create(*outPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create output: %v\n", err)
		os.Exit(1)
	}
	defer file.Close()
	encoder := json.NewEncoder(file)
	encoder.SetIndent("", "  ")
	if err := encoder.Encode(&artifacts); err != nil {
		fmt.Fprintf(os.Stderr, "encode artifacts: %v\n", err)
		os.Exit(1)
	}

	fmt.Fprintf(
		os.Stderr,
		"wrote %s (compile %.2fms, load-pk %.2fms, load-vk %.2fms, prove %.2fms, verify %.2fms)\n",
		*outPath,
		compileMS,
		loadPKMS,
		loadVKMS,
		proveMS,
		verifyMS,
	)
}
