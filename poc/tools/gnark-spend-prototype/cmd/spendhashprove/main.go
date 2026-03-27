package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"math/big"
	"os"
	"time"

	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"

	"github.com/penumbra-zone/penumbra/tools/gnark-spend-prototype"
)

type spendStatementHashCircuit struct {
	Fields               [17]frontend.Variable
	ClaimedStatementHash frontend.Variable `gnark:",public"`
}

func (c *spendStatementHashCircuit) Define(api frontend.API) error {
	fields := make([]frontend.Variable, len(c.Fields))
	for i := range c.Fields {
		fields[i] = c.Fields[i]
	}
	computed, err := prototype.SpendStatementHash(api, fields)
	if err != nil {
		return err
	}
	api.AssertIsEqual(computed, c.ClaimedStatementHash)
	return nil
}

type g1PointJSON struct {
	X string `json:"x"`
	Y string `json:"y"`
}

type fq2JSON struct {
	A0 string `json:"a0"`
	A1 string `json:"a1"`
}

type g2PointJSON struct {
	X fq2JSON `json:"x"`
	Y fq2JSON `json:"y"`
}

type proofJSON struct {
	A g1PointJSON `json:"a"`
	B g2PointJSON `json:"b"`
	C g1PointJSON `json:"c"`
}

type verifyingKeyJSON struct {
	AlphaG1    g1PointJSON   `json:"alpha_g1"`
	BetaG2     g2PointJSON   `json:"beta_g2"`
	GammaG2    g2PointJSON   `json:"gamma_g2"`
	DeltaG2    g2PointJSON   `json:"delta_g2"`
	GammaABCG1 []g1PointJSON `json:"gamma_abc_g1"`
}

type timingsJSON struct {
	CompileMS float64 `json:"compile_ms"`
	SetupMS   float64 `json:"setup_ms"`
	ProveMS   float64 `json:"prove_ms"`
	VerifyMS  float64 `json:"verify_ms"`
}

type artifactJSON struct {
	Curve                string           `json:"curve"`
	Circuit              string           `json:"circuit"`
	PublicInputs         []string         `json:"public_inputs"`
	StatementFields      []string         `json:"statement_fields"`
	ClaimedStatementHash string           `json:"claimed_statement_hash"`
	Proof                proofJSON        `json:"proof"`
	VerifyingKey         verifyingKeyJSON `json:"verifying_key"`
	Timings              timingsJSON      `json:"timings"`
}

func mustBigInt(dec string) *big.Int {
	value, ok := new(big.Int).SetString(dec, 10)
	if !ok {
		panic("invalid decimal: " + dec)
	}
	return value
}

func encodeProof(proof *groth16bls.Proof) proofJSON {
	return proofJSON{
		A: g1PointJSON{X: proof.Ar.X.String(), Y: proof.Ar.Y.String()},
		B: g2PointJSON{
			X: fq2JSON{A0: proof.Bs.X.A0.String(), A1: proof.Bs.X.A1.String()},
			Y: fq2JSON{A0: proof.Bs.Y.A0.String(), A1: proof.Bs.Y.A1.String()},
		},
		C: g1PointJSON{X: proof.Krs.X.String(), Y: proof.Krs.Y.String()},
	}
}

func encodeVerifyingKey(vk *groth16bls.VerifyingKey) verifyingKeyJSON {
	k := make([]g1PointJSON, len(vk.G1.K))
	for i := range vk.G1.K {
		k[i] = g1PointJSON{
			X: vk.G1.K[i].X.String(),
			Y: vk.G1.K[i].Y.String(),
		}
	}
	return verifyingKeyJSON{
		AlphaG1: g1PointJSON{X: vk.G1.Alpha.X.String(), Y: vk.G1.Alpha.Y.String()},
		BetaG2: g2PointJSON{
			X: fq2JSON{A0: vk.G2.Beta.X.A0.String(), A1: vk.G2.Beta.X.A1.String()},
			Y: fq2JSON{A0: vk.G2.Beta.Y.A0.String(), A1: vk.G2.Beta.Y.A1.String()},
		},
		GammaG2: g2PointJSON{
			X: fq2JSON{A0: vk.G2.Gamma.X.A0.String(), A1: vk.G2.Gamma.X.A1.String()},
			Y: fq2JSON{A0: vk.G2.Gamma.Y.A0.String(), A1: vk.G2.Gamma.Y.A1.String()},
		},
		DeltaG2: g2PointJSON{
			X: fq2JSON{A0: vk.G2.Delta.X.A0.String(), A1: vk.G2.Delta.X.A1.String()},
			Y: fq2JSON{A0: vk.G2.Delta.Y.A0.String(), A1: vk.G2.Delta.Y.A1.String()},
		},
		GammaABCG1: k,
	}
}

func main() {
	outPath := flag.String("out", "", "output JSON path")
	flag.Parse()
	if *outPath == "" {
		fmt.Fprintln(os.Stderr, "--out is required")
		os.Exit(2)
	}

	fixture, err := prototype.LoadSpendFixtureForCLI()
	if err != nil {
		fmt.Fprintf(os.Stderr, "load spend fixture: %v\n", err)
		os.Exit(1)
	}

	var assignment spendStatementHashCircuit
	statementFields := make([]string, len(fixture.StatementFields))
	copy(statementFields, fixture.StatementFields)
	for i := range assignment.Fields {
		assignment.Fields[i] = mustBigInt(fixture.StatementFields[i])
	}
	assignment.ClaimedStatementHash = mustBigInt(fixture.ClaimedStatementHash)

	compileStart := time.Now()
	ccs, err := frontend.Compile(prototype.ScalarField(), r1cs.NewBuilder, &spendStatementHashCircuit{})
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

	fullWitness, err := frontend.NewWitness(&assignment, prototype.ScalarField())
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
	proofIface, err := groth16.Prove(ccs, pkIface, fullWitness)
	if err != nil {
		fmt.Fprintf(os.Stderr, "prove: %v\n", err)
		os.Exit(1)
	}
	proveMS := time.Since(proveStart).Seconds() * 1000

	verifyStart := time.Now()
	if err := groth16.Verify(proofIface, vkIface, publicWitness); err != nil {
		fmt.Fprintf(os.Stderr, "gnark verify: %v\n", err)
		os.Exit(1)
	}
	verifyMS := time.Since(verifyStart).Seconds() * 1000

	proof, ok := proofIface.(*groth16bls.Proof)
	if !ok {
		fmt.Fprintf(os.Stderr, "unexpected proof type %T\n", proofIface)
		os.Exit(1)
	}
	vk, ok := vkIface.(*groth16bls.VerifyingKey)
	if !ok {
		fmt.Fprintf(os.Stderr, "unexpected vk type %T\n", vkIface)
		os.Exit(1)
	}

	artifacts := artifactJSON{
		Curve:                "bls12-377",
		Circuit:              "spend-statement-hash",
		PublicInputs:         []string{fixture.ClaimedStatementHash},
		StatementFields:      statementFields,
		ClaimedStatementHash: fixture.ClaimedStatementHash,
		Proof:                encodeProof(proof),
		VerifyingKey:         encodeVerifyingKey(vk),
		Timings: timingsJSON{
			CompileMS: compileMS,
			SetupMS:   setupMS,
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
		"wrote %s (compile %.2fms, setup %.2fms, prove %.2fms, verify %.2fms)\n",
		*outPath,
		compileMS,
		setupMS,
		proveMS,
		verifyMS,
	)
}
