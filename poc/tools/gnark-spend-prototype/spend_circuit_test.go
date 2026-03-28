package prototype

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/test"
)

func TestFullSpendCircuitMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}
	assignment, err := NewSpendCircuitAssignmentFromFixture(fixture)
	if err != nil {
		t.Fatalf("build assignment: %v", err)
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&SpendCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestFullSpendCircuitMatchesWitnessBinary(t *testing.T) {
	assignment, err := NewSpendCircuitAssignmentFromWitnessV1(loadSpendWitnessV1())
	if err != nil {
		t.Fatalf("build assignment from witness binary: %v", err)
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&SpendCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestFullSpendCircuitRejectsWrongStatementHash(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}
	assignment, err := NewSpendCircuitAssignmentFromFixture(fixture)
	if err != nil {
		t.Fatalf("build assignment: %v", err)
	}
	assignment.ClaimedStatementHash = fixture.WrongClaimedStatementHash

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&SpendCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithInvalidAssignment(assignment),
	)
}

func TestFullSpendCircuitRejectsMutatedComplianceField(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}
	assignment, err := NewSpendCircuitAssignmentFromFixture(fixture)
	if err != nil {
		t.Fatalf("build assignment: %v", err)
	}
	assignment.DleqC = mutateFieldByOne(assignment.DleqC)

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&SpendCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithInvalidAssignment(assignment),
	)
}

func TestFullSpendCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &SpendCircuit{})
	if err != nil {
		t.Fatalf("compile full spend circuit: %v", err)
	}
}
