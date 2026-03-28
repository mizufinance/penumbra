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
	outDir := flag.String("out-dir", "", "output directory")
	flag.Parse()
	if *outDir == "" {
		fmt.Fprintln(os.Stderr, "--out-dir is required")
		os.Exit(2)
	}
	if err := os.MkdirAll(*outDir, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "create output dir: %v\n", err)
		os.Exit(1)
	}

	compileStart := time.Now()
	ccs, err := frontend.Compile(prototype.ScalarField(), r1cs.NewBuilder, &prototype.SpendCircuit{})
	if err != nil {
		fmt.Fprintf(os.Stderr, "compile circuit: %v\n", err)
		os.Exit(1)
	}
	compileMS := time.Since(compileStart).Seconds() * 1000

	setupStart := time.Now()
	pkIface, vkIface, err := groth16.Setup(ccs)
	if err != nil {
		fmt.Fprintf(os.Stderr, "setup: %v\n", err)
		os.Exit(1)
	}
	setupMS := time.Since(setupStart).Seconds() * 1000

	pk, ok := pkIface.(*groth16bls.ProvingKey)
	if !ok {
		fmt.Fprintf(os.Stderr, "unexpected proving key type %T\n", pkIface)
		os.Exit(1)
	}
	vk, ok := vkIface.(*groth16bls.VerifyingKey)
	if !ok {
		fmt.Fprintf(os.Stderr, "unexpected verifying key type %T\n", vkIface)
		os.Exit(1)
	}

	pkPath := filepath.Join(*outDir, "proving_key.bin")
	pkFile, err := os.Create(pkPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create proving key file: %v\n", err)
		os.Exit(1)
	}
	if _, err := pk.WriteTo(pkFile); err != nil {
		_ = pkFile.Close()
		fmt.Fprintf(os.Stderr, "write proving key: %v\n", err)
		os.Exit(1)
	}
	if err := pkFile.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "close proving key file: %v\n", err)
		os.Exit(1)
	}

	vkBinPath := filepath.Join(*outDir, "verifying_key.bin")
	vkBinFile, err := os.Create(vkBinPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create verifying key file: %v\n", err)
		os.Exit(1)
	}
	if _, err := vk.WriteTo(vkBinFile); err != nil {
		_ = vkBinFile.Close()
		fmt.Fprintf(os.Stderr, "write verifying key: %v\n", err)
		os.Exit(1)
	}
	if err := vkBinFile.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "close verifying key file: %v\n", err)
		os.Exit(1)
	}

	vkJSONPath := filepath.Join(*outDir, "verifying_key.json")
	vkJSONFile, err := os.Create(vkJSONPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create verifying key json: %v\n", err)
		os.Exit(1)
	}
	enc := json.NewEncoder(vkJSONFile)
	enc.SetIndent("", "  ")
	if err := enc.Encode(prototype.EncodeVerifyingKeyJSON(vk)); err != nil {
		_ = vkJSONFile.Close()
		fmt.Fprintf(os.Stderr, "encode verifying key json: %v\n", err)
		os.Exit(1)
	}
	if err := vkJSONFile.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "close verifying key json: %v\n", err)
		os.Exit(1)
	}

	pkSize, err := prototype.FileSize(pkPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "stat proving key: %v\n", err)
		os.Exit(1)
	}
	vkSize, err := prototype.FileSize(vkBinPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "stat verifying key: %v\n", err)
		os.Exit(1)
	}

	metadata := prototype.CircuitMetadataJSON{
		Curve:            "bls12-377",
		Circuit:          "spend",
		CompileMS:        compileMS,
		SetupMS:          setupMS,
		ProvingKeySize:   pkSize,
		VerifyingKeySize: vkSize,
	}
	prototype.FillCircuitMetadataShape(&metadata, ccs)
	metadataPath := filepath.Join(*outDir, "circuit_metadata.json")
	metadataFile, err := os.Create(metadataPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create circuit metadata: %v\n", err)
		os.Exit(1)
	}
	metadataEnc := json.NewEncoder(metadataFile)
	metadataEnc.SetIndent("", "  ")
	if err := metadataEnc.Encode(metadata); err != nil {
		_ = metadataFile.Close()
		fmt.Fprintf(os.Stderr, "encode circuit metadata: %v\n", err)
		os.Exit(1)
	}
	if err := metadataFile.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "close circuit metadata: %v\n", err)
		os.Exit(1)
	}

	fmt.Fprintf(
		os.Stderr,
		"wrote %s (compile %.2fms, setup %.2fms, pk %d bytes, vk %d bytes)\n",
		*outDir,
		compileMS,
		setupMS,
		pkSize,
		vkSize,
	)
}
