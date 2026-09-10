package circuits_test

import (
	"fmt"
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
)

func withdrawalIdentityAssignmentSolved(
	ccs constraint.ConstraintSystem,
	assignment *circuits.ShieldedWithdrawalCircuit,
) error {
	witness, err := frontend.NewWitness(assignment, ecc.BLS12_377.ScalarField())
	if err != nil {
		return fmt.Errorf("build witness: %w", err)
	}
	return ccs.IsSolved(witness)
}

func TestWithdrawalCircuitRejectsIdentityAuthorizationAndDiversifiedGenerator(
	t *testing.T,
) {
	_, canonical, nIn := loadWithdrawalFixture(t)
	ccs, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		circuits.NewShieldedWithdrawalCircuit(nIn),
	)
	if err != nil {
		t.Fatalf("compile Withdrawal circuit: %v", err)
	}
	if err := withdrawalIdentityAssignmentSolved(ccs, canonical); err != nil {
		t.Fatalf("canonical witness must satisfy nonidentity guards: %v", err)
	}

	minusOne := new(big.Int).Sub(ecc.BLS12_377.ScalarField(), big.NewInt(1))
	// These are whole-relation drift probes. The IVK mutation also disturbs
	// its KDF decomposition, so isolated guard evidence lives in the
	// primitive and exact-trace tests.
	for _, mutation := range []struct {
		name   string
		mutate func(*circuits.ShieldedWithdrawalCircuit)
	}{
		{
			name: "authorization_key_positive_representative",
			mutate: func(c *circuits.ShieldedWithdrawalCircuit) {
				c.Auth.AK = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "authorization_key_negative_representative",
			mutate: func(c *circuits.ShieldedWithdrawalCircuit) {
				c.Auth.AK = circuits.Point2D{X: 0, Y: minusOne}
			},
		},
		{
			name: "sender_diversified_generator_positive_representative",
			mutate: func(c *circuits.ShieldedWithdrawalCircuit) {
				c.Sender.DivGen = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "sender_diversified_generator_negative_representative",
			mutate: func(c *circuits.ShieldedWithdrawalCircuit) {
				c.Sender.DivGen = circuits.Point2D{X: 0, Y: minusOne}
			},
		},
		{
			name: "incoming_viewing_key_zero",
			mutate: func(c *circuits.ShieldedWithdrawalCircuit) {
				c.Auth.IVKReduced = 0
			},
		},
	} {
		t.Run(mutation.name, func(t *testing.T) {
			_, assignment, _ := loadWithdrawalFixture(t)
			mutation.mutate(assignment)
			if err := withdrawalIdentityAssignmentSolved(ccs, assignment); err == nil {
				t.Fatal("Withdrawal must reject the identity-class mutation")
			}
		})
	}
}
