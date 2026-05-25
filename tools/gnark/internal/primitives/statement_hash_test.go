package primitives

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
)

type transferStatementHashCircuit struct {
	NIn                  int `gnark:"-"`
	NOut                 int `gnark:"-"`
	Fields               []frontend.Variable
	ClaimedStatementHash frontend.Variable `gnark:",public"`
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

func makeTransferStatementAssignment(t *testing.T, family generated.TransferFamilySpec) *transferStatementHashCircuit {
	t.Helper()

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
	return assignment
}

func TestTransferStatementHashGroth16RoundTrip(t *testing.T) {
	for _, family := range generated.TransferFamilies {
		t.Run(family.Label, func(t *testing.T) {
			assignment := makeTransferStatementAssignment(t, family)
			template := &transferStatementHashCircuit{
				NIn:    family.NIn,
				NOut:   family.NOut,
				Fields: make([]frontend.Variable, len(assignment.Fields)),
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

func TestTransferStatementHashRejectsMutatedWitnessField(t *testing.T) {
	family := generated.TransferFamilies[0]
	template := &transferStatementHashCircuit{
		NIn:    family.NIn,
		NOut:   family.NOut,
		Fields: make([]frontend.Variable, transferStatementFieldCount(family.NIn, family.NOut)),
	}
	ccs, err := frontend.Compile(ScalarField(), r1cs.NewBuilder, template)
	if err != nil {
		t.Fatalf("compile %s circuit: %v", family.Label, err)
	}

	pk, _, err := groth16.Setup(ccs)
	if err != nil {
		t.Fatalf("setup %s: %v", family.Label, err)
	}

	assignment := makeTransferStatementAssignment(t, family)
	assignment.Fields[0] = big.NewInt(999999)
	fullWitness, err := frontend.NewWitness(assignment, ScalarField())
	if err != nil {
		t.Fatalf("build %s witness: %v", family.Label, err)
	}
	if _, err := groth16.Prove(ccs, pk, fullWitness); err == nil {
		t.Fatalf("expected prove failure after mutating a public-bound transfer witness field")
	}
}
