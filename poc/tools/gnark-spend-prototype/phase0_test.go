package prototype

import (
	"fmt"
	"strings"
	"testing"

	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	stdhash "github.com/consensys/gnark/std/hash"
	_ "github.com/consensys/gnark/std/hash/all"

	"github.com/consensys/gnark-crypto/ecc"
	te377 "github.com/consensys/gnark-crypto/ecc/bls12-377/twistededwards"
)

type mulCircuit struct {
	X frontend.Variable
	Y frontend.Variable
	Z frontend.Variable `gnark:",public"`
}

func (c *mulCircuit) Define(api frontend.API) error {
	api.AssertIsEqual(api.Mul(c.X, c.Y), c.Z)
	return nil
}

func TestPhase0Groth16Bls12377RoundTrip(t *testing.T) {
	ccs, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &mulCircuit{})
	if err != nil {
		t.Fatalf("compile circuit: %v", err)
	}

	pk, vk, err := groth16.Setup(ccs)
	if err != nil {
		t.Fatalf("setup groth16: %v", err)
	}

	assignment := &mulCircuit{
		X: 3,
		Y: 5,
		Z: 15,
	}

	fullWitness, err := frontend.NewWitness(assignment, ecc.BLS12_377.ScalarField())
	if err != nil {
		t.Fatalf("build witness: %v", err)
	}

	proof, err := groth16.Prove(ccs, pk, fullWitness)
	if err != nil {
		t.Fatalf("prove: %v", err)
	}

	publicWitness, err := fullWitness.Public()
	if err != nil {
		t.Fatalf("extract public witness: %v", err)
	}

	if err := groth16.Verify(proof, vk, publicWitness); err != nil {
		t.Fatalf("verify: %v", err)
	}
}

func TestPhase05CompanionCurveMatchesDecaf377(t *testing.T) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	curve := te377.GetEdwardsCurve()

	if got, want := curve.A.String(), vectors.Decaf377CompanionCurve.A; got != want {
		t.Fatalf("curve A mismatch: got %s want %s", got, want)
	}
	if got, want := curve.D.String(), vectors.Decaf377CompanionCurve.D; got != want {
		t.Fatalf("curve D mismatch: got %s want %s", got, want)
	}
	if got, want := curve.Order.String(), vectors.Decaf377CompanionCurve.Order; got != want {
		t.Fatalf("curve order mismatch: got %s want %s", got, want)
	}
}

type missingPoseidon377Circuit struct{}

func (c *missingPoseidon377Circuit) Define(api frontend.API) error {
	_, err := stdhash.GetFieldHasher("POSEIDON377", api)
	return err
}

func TestPhase05Poseidon377IsNotNativelyRegistered(t *testing.T) {
	assertMissing := func(msg string) {
		t.Helper()
		if !strings.Contains(msg, "POSEIDON377") || !strings.Contains(msg, "not registered") {
			t.Fatalf("unexpected missing-hash message: %q", msg)
		}
	}

	defer func() {
		r := recover()
		if r != nil {
			assertMissing(fmt.Sprint(r))
		}
	}()

	_, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		&missingPoseidon377Circuit{},
	)
	if err == nil {
		t.Fatalf("expected compile failure because POSEIDON377 is not registered")
	}
	assertMissing(err.Error())
}
