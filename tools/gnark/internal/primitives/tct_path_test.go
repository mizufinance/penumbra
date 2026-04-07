package primitives

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/test"
)

type stateCommitmentPathCircuit struct {
	Commitment frontend.Variable
	Position   frontend.Variable
	Path       [24][3]frontend.Variable

	ExpectedRoot frontend.Variable `gnark:",public"`
}

func (c *stateCommitmentPathCircuit) Define(api frontend.API) error {
	path := make([][3]frontend.Variable, len(c.Path))
	copy(path, c.Path[:])
	root, err := VerifyStateCommitmentPath(api, c.Commitment, c.Position, path)
	if err != nil {
		return err
	}
	api.AssertIsEqual(root, c.ExpectedRoot)
	return nil
}

func TestStateCommitmentPathNativeMatchesRust(t *testing.T) {
	fixture, err := LoadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	root, err := VerifyStateCommitmentPathNative(fixture)
	if err != nil {
		t.Fatalf("verify state commitment path: %v", err)
	}

	if got, want := root.String(), fixture.Public.Anchor; got != want {
		t.Fatalf("state commitment root mismatch: got %s want %s", got, want)
	}
}

func TestStateCommitmentPathCircuitMatchesFixture(t *testing.T) {
	fixture, err := LoadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	var path [24][3]frontend.Variable
	for i, siblings := range fixture.Private.StateCommitmentProof.AuthPath {
		for j, sibling := range siblings {
			path[i][j] = sibling
		}
	}

	assignment := &stateCommitmentPathCircuit{
		Commitment:   fixture.Private.StateCommitmentProof.Commitment,
		Position:     fixture.Private.StateCommitmentProof.Position,
		Path:         path,
		ExpectedRoot: fixture.Public.Anchor,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&stateCommitmentPathCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestStateCommitmentPathCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &stateCommitmentPathCircuit{})
	if err != nil {
		t.Fatalf("compile state commitment path circuit: %v", err)
	}
}
