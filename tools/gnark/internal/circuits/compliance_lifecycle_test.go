package circuits

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/test"
)

type activeComplianceLifecycleCircuit struct {
	Lifecycle frontend.Variable
	Enabled   frontend.Variable
}

func (c *activeComplianceLifecycleCircuit) Define(api frontend.API) error {
	api.AssertIsBoolean(c.Enabled)
	AssertActiveComplianceLifecycle(api, c.Lifecycle, c.Enabled)
	return nil
}

func packedComplianceLifecycle(status, generation, frozenSince uint64) *big.Int {
	value := new(big.Int).SetUint64(frozenSince)
	value.Lsh(value, 64)
	value.Add(value, new(big.Int).SetUint64(generation))
	value.Lsh(value, 3)
	return value.Add(value, new(big.Int).SetUint64(status))
}

func TestActiveComplianceLifecycleAuthenticatesGeneration(t *testing.T) {
	valid := []*activeComplianceLifecycleCircuit{
		{Lifecycle: packedComplianceLifecycle(1, 0, 0), Enabled: 1},
		{Lifecycle: packedComplianceLifecycle(1, 7, 0), Enabled: 1},
		{Lifecycle: packedComplianceLifecycle(2, 1, 40), Enabled: 0},
		{Lifecycle: packedComplianceLifecycle(3, 1, 40), Enabled: 0},
	}
	for index, assignment := range valid {
		if err := test.IsSolved(
			&activeComplianceLifecycleCircuit{},
			assignment,
			ecc.BLS12_377.ScalarField(),
		); err != nil {
			t.Fatalf("valid lifecycle %d: %v", index, err)
		}
	}

	invalid := []*activeComplianceLifecycleCircuit{
		{Lifecycle: packedComplianceLifecycle(2, 1, 0), Enabled: 1},
		{Lifecycle: packedComplianceLifecycle(1, 1, 40), Enabled: 1},
	}
	for index, assignment := range invalid {
		if err := test.IsSolved(
			&activeComplianceLifecycleCircuit{},
			assignment,
			ecc.BLS12_377.ScalarField(),
		); err == nil {
			t.Fatalf("invalid lifecycle %d must fail", index)
		}
	}
}
