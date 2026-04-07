package primitives

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/generated"
)

type spendStatementHashCircuit struct {
	Fields               [17]frontend.Variable
	ClaimedStatementHash frontend.Variable `gnark:",public"`
}

type transferStatementHashCircuit struct {
	NIn                  int `gnark:"-"`
	NOut                 int `gnark:"-"`
	Fields               []frontend.Variable
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

func (c *transferStatementHashCircuit) Define(api frontend.API) error {
	fields := make([]frontend.Variable, len(c.Fields))
	for i := range c.Fields {
		fields[i] = c.Fields[i]
	}
	computed, err := TransferStatementHashForShape(api, c.NIn, c.NOut, fields)
	if err != nil {
		return err
	}
	api.AssertIsEqual(computed, c.ClaimedStatementHash)
	return nil
}

func loadStatementHashAssignment(t *testing.T) spendStatementHashCircuit {
	t.Helper()

	fixture, err := LoadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	var assignment spendStatementHashCircuit
	for i := range assignment.Fields {
		assignment.Fields[i] = MustBigInt(fixture.StatementFields[i])
	}
	assignment.ClaimedStatementHash = MustBigInt(fixture.ClaimedStatementHash)
	return assignment
}

func TestSpendStatementHashMatchesRustFixture(t *testing.T) {
	fixture, err := LoadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	fields := make([]*big.Int, len(fixture.StatementFields))
	for i, value := range fixture.StatementFields {
		fields[i] = MustBigInt(value)
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
	assignment.Fields[8] = MustBigInt("123456789")

	fullWitness, err := frontend.NewWitness(&assignment, ScalarField())
	if err != nil {
		t.Fatalf("build witness: %v", err)
	}
	if _, err := groth16.Prove(ccs, pk, fullWitness); err == nil {
		t.Fatalf("expected prove failure after mutating a public-bound witness field")
	}
}

func TestTransferStatementHashGroth16RoundTrip(t *testing.T) {
	for _, family := range generated.TransferFamilies {
		t.Run(family.Label, func(t *testing.T) {
			fieldCount := transferStatementFieldCount(family.NIn, family.NOut)
			assignment := &transferStatementHashCircuit{
				NIn:    family.NIn,
				NOut:   family.NOut,
				Fields: make([]frontend.Variable, fieldCount),
			}
			fields := make([]*big.Int, fieldCount)
			for i := 0; i < fieldCount; i++ {
				value := big.NewInt(int64(i + 1))
				fields[i] = value
				assignment.Fields[i] = value
			}
			hash, err := TransferStatementHashNativeForShape(fields, family.NIn, family.NOut)
			if err != nil {
				t.Fatalf("native %s statement hash: %v", family.Label, err)
			}
			assignment.ClaimedStatementHash = hash

			template := &transferStatementHashCircuit{
				NIn:    family.NIn,
				NOut:   family.NOut,
				Fields: make([]frontend.Variable, fieldCount),
			}
			ccs, err := frontend.Compile(ScalarField(), r1cs.NewBuilder, template)
			if err != nil {
				t.Fatalf("compile %s circuit: %v", family.Label, err)
			}
			pk, vk, err := groth16.Setup(ccs)
			if err != nil {
				t.Fatalf("setup %s: %v", family.Label, err)
			}
			fullWitness, err := frontend.NewWitness(assignment, ScalarField())
			if err != nil {
				t.Fatalf("build %s witness: %v", family.Label, err)
			}
			proof, err := groth16.Prove(ccs, pk, fullWitness)
			if err != nil {
				t.Fatalf("prove %s: %v", family.Label, err)
			}
			publicWitness, err := fullWitness.Public()
			if err != nil {
				t.Fatalf("public witness %s: %v", family.Label, err)
			}
			if err := groth16.Verify(proof, vk, publicWitness); err != nil {
				t.Fatalf("verify %s: %v", family.Label, err)
			}
		})
	}
}
