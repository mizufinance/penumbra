package circuits_test

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/test"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/circuits"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

func TestFullSpendCircuitMatchesFixture(t *testing.T) {
	fixture, err := primitives.LoadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}
	assignment, err := circuits.NewSpendCircuitAssignmentFromFixture(fixture)
	if err != nil {
		t.Fatalf("build assignment: %v", err)
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&circuits.SpendCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}
