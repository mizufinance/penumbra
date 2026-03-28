package main

import (
	"flag"
	"fmt"
	"math/big"
	"os"
	"time"

	curve "github.com/consensys/gnark-crypto/ecc/bls12-377"
	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	backendwitness "github.com/consensys/gnark/backend/witness"

	prototype "github.com/penumbra-zone/penumbra/tools/gnark-spend-prototype"
)

func main() {
	artifactPath := flag.String("artifacts", "", "path to spendprove artifact JSON")
	outPath := flag.String("out", "", "output verifier benchmark JSON path")
	warmupIterations := flag.Int("warmup-iterations", 3, "number of untimed verify warmup iterations")
	measuredIterations := flag.Int("measured-iterations", 20, "number of measured verify iterations")
	flag.Parse()

	if *artifactPath == "" || *outPath == "" {
		fmt.Fprintln(os.Stderr, "--artifacts and --out are required")
		os.Exit(2)
	}
	if *warmupIterations < 0 || *measuredIterations <= 0 {
		fmt.Fprintln(os.Stderr, "--warmup-iterations must be >= 0 and --measured-iterations must be > 0")
		os.Exit(2)
	}

	loadStart := time.Now()
	artifacts, err := prototype.LoadArtifactJSON(*artifactPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "load artifacts: %v\n", err)
		os.Exit(1)
	}
	if artifacts.Curve != "bls12-377" {
		fmt.Fprintf(os.Stderr, "unexpected curve %q\n", artifacts.Curve)
		os.Exit(1)
	}
	if artifacts.Circuit != "spend" {
		fmt.Fprintf(os.Stderr, "unexpected circuit %q\n", artifacts.Circuit)
		os.Exit(1)
	}
	if len(artifacts.PublicInputs) != 1 {
		fmt.Fprintf(os.Stderr, "expected exactly one public input, got %d\n", len(artifacts.PublicInputs))
		os.Exit(1)
	}
	if artifacts.PublicInputs[0] != artifacts.ClaimedStatementHash {
		fmt.Fprintln(os.Stderr, "public input does not match claimed statement hash")
		os.Exit(1)
	}

	proof, err := decodeProof(artifacts.Proof)
	if err != nil {
		fmt.Fprintf(os.Stderr, "decode proof: %v\n", err)
		os.Exit(1)
	}
	vk, err := decodeVerifyingKey(artifacts.VerifyingKey)
	if err != nil {
		fmt.Fprintf(os.Stderr, "decode verifying key: %v\n", err)
		os.Exit(1)
	}
	publicWitness, err := buildPublicWitness(artifacts.PublicInputs)
	if err != nil {
		fmt.Fprintf(os.Stderr, "build public witness: %v\n", err)
		os.Exit(1)
	}
	loadOrDecodeMS := time.Since(loadStart).Seconds() * 1000

	prepareStart := time.Now()
	if err := vk.Precompute(); err != nil {
		fmt.Fprintf(os.Stderr, "precompute verifying key: %v\n", err)
		os.Exit(1)
	}
	prepareMS := time.Since(prepareStart).Seconds() * 1000

	for i := 0; i < *warmupIterations; i++ {
		if err := groth16.Verify(proof, vk, publicWitness); err != nil {
			fmt.Fprintf(os.Stderr, "warmup verify %d failed: %v\n", i, err)
			os.Exit(1)
		}
	}

	verifySamples := make([]float64, 0, *measuredIterations)
	for i := 0; i < *measuredIterations; i++ {
		verifyStart := time.Now()
		if err := groth16.Verify(proof, vk, publicWitness); err != nil {
			fmt.Fprintf(os.Stderr, "measured verify %d failed: %v\n", i, err)
			os.Exit(1)
		}
		verifySamples = append(verifySamples, time.Since(verifyStart).Seconds()*1000)
	}
	verifyMeanMS, verifyMedianMS, verifyMinMS, verifyMaxMS := prototype.ComputeDurationStats(verifySamples)

	report := prototype.VerifyBenchResultJSON{
		Curve:                artifacts.Curve,
		Circuit:              artifacts.Circuit,
		ClaimedStatementHash: artifacts.ClaimedStatementHash,
		LoadOrDecodeMS:       loadOrDecodeMS,
		PrepareMS:            prepareMS,
		VerifyWarmupIters:    *warmupIterations,
		VerifyMeasuredIters:  *measuredIterations,
		VerifyMeanMS:         verifyMeanMS,
		VerifyMedianMS:       verifyMedianMS,
		VerifyMinMS:          verifyMinMS,
		VerifyMaxMS:          verifyMaxMS,
	}

	if err := prototype.WriteJSON(*outPath, &report); err != nil {
		fmt.Fprintf(os.Stderr, "write report: %v\n", err)
		os.Exit(1)
	}
	fmt.Fprintf(
		os.Stderr,
		"wrote %s (decode %.2fms, prepare %.2fms, verify mean %.2fms, median %.2fms)\n",
		*outPath,
		loadOrDecodeMS,
		prepareMS,
		verifyMeanMS,
		verifyMedianMS,
	)
}

func buildPublicWitness(publicInputs []string) (backendwitness.Witness, error) {
	publicWitness, err := backendwitness.New(prototype.ScalarField())
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

func decodeProof(proofJSON prototype.ProofJSON) (*groth16bls.Proof, error) {
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

func decodeVerifyingKey(vkJSON prototype.VerifyingKeyJSON) (*groth16bls.VerifyingKey, error) {
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

func setG1Affine(dst *curve.G1Affine, point prototype.G1PointJSON) error {
	if _, err := dst.X.SetString(point.X); err != nil {
		return err
	}
	if _, err := dst.Y.SetString(point.Y); err != nil {
		return err
	}
	return nil
}

func setG2Affine(dst *curve.G2Affine, point prototype.G2PointJSON) error {
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
