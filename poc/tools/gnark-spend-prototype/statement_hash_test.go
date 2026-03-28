package prototype

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
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
	computed, err := SpendStatementHash(api, fields)
	if err != nil {
		return err
	}
	api.AssertIsEqual(computed, c.ClaimedStatementHash)
	return nil
}

func loadStatementHashAssignment(t *testing.T) spendStatementHashCircuit {
	t.Helper()

	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	var assignment spendStatementHashCircuit
	for i := range assignment.Fields {
		assignment.Fields[i] = mustBigInt(fixture.StatementFields[i])
	}
	assignment.ClaimedStatementHash = mustBigInt(fixture.ClaimedStatementHash)
	return assignment
}

func TestSpendStatementHashMatchesRustFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	fields := make([]*big.Int, len(fixture.StatementFields))
	for i, value := range fixture.StatementFields {
		fields[i] = mustBigInt(value)
	}

	got, err := SpendStatementHashNative(fields)
	if err != nil {
		t.Fatalf("native spend statement hash: %v", err)
	}
	if got.String() != fixture.ClaimedStatementHash {
		t.Fatalf("statement hash mismatch: got %s want %s", got.String(), fixture.ClaimedStatementHash)
	}
}

func TestSpendStatementHashGroth16RoundTrip(t *testing.T) {
	ccs, err := frontend.Compile(ScalarField(), r1cs.NewBuilder, &spendStatementHashCircuit{})
	if err != nil {
		t.Fatalf("compile circuit: %v", err)
	}

	pk, vk, err := groth16.Setup(ccs)
	if err != nil {
		t.Fatalf("setup: %v", err)
	}

	assignment := loadStatementHashAssignment(t)
	fullWitness, err := frontend.NewWitness(&assignment, ScalarField())
	if err != nil {
		t.Fatalf("build witness: %v", err)
	}
	proof, err := groth16.Prove(ccs, pk, fullWitness)
	if err != nil {
		t.Fatalf("prove: %v", err)
	}
	publicWitness, err := fullWitness.Public()
	if err != nil {
		t.Fatalf("public witness: %v", err)
	}
	if err := groth16.Verify(proof, vk, publicWitness); err != nil {
		t.Fatalf("verify: %v", err)
	}
}

func TestSpendStatementHashRejectsMutatedPublicField(t *testing.T) {
	ccs, err := frontend.Compile(ScalarField(), r1cs.NewBuilder, &spendStatementHashCircuit{})
	if err != nil {
		t.Fatalf("compile circuit: %v", err)
	}

	pk, _, err := groth16.Setup(ccs)
	if err != nil {
		t.Fatalf("setup: %v", err)
	}

	assignment := loadStatementHashAssignment(t)
	assignment.Fields[8] = mustBigInt("123456789")

	fullWitness, err := frontend.NewWitness(&assignment, ScalarField())
	if err != nil {
		t.Fatalf("build witness: %v", err)
	}
	if _, err := groth16.Prove(ccs, pk, fullWitness); err == nil {
		t.Fatalf("expected prove failure after mutating a public-bound witness field")
	}
}
