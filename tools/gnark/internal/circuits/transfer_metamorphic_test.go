package circuits_test

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/test"
	"github.com/mizufinance/penumbra/tools/gnark/internal/abi"
	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/testfixtures"
)

type transferMutation struct {
	name   string
	mutate func(*circuits.TransferCircuit)
}

func loadTransferAssignment(t *testing.T) *circuits.TransferCircuit {
	t.Helper()
	fixtureBytes := testfixtures.LoadTransferWitnessV1("transfer")
	assignment, _, err := abi.NewTransferCircuitAssignmentFromWitnessV1(fixtureBytes)
	if err != nil {
		t.Fatalf("decode transfer witness fixture: %v", err)
	}
	validateTransferMutationFixture(t, assignment)
	return assignment
}

func validateTransferMutationFixture(t *testing.T, assignment *circuits.TransferCircuit) {
	t.Helper()
	// These metamorphic tests assume the fixed 2-input, 2-output transfer fixture
	// used by the transfer circuit and mutate the first spend/output path directly.
	if len(assignment.Spends) == 0 {
		t.Fatalf("transfer witness fixture must contain at least one spend")
	}
	if len(assignment.Outputs) < 2 {
		t.Fatalf("transfer witness fixture must contain at least two outputs")
	}
	if len(assignment.Spends[0].StateProof.Path) == 0 {
		t.Fatalf("transfer witness fixture first spend must contain a state path")
	}
}

func assertTransferMutationRejected(t *testing.T, mutation transferMutation) {
	t.Helper()
	assignment := loadTransferAssignment(t)
	mutation.mutate(assignment)

	if err := test.IsSolved(circuits.NewTransferCircuit(), assignment, ecc.BLS12_377.ScalarField()); err == nil {
		t.Fatalf("expected transfer circuit to reject mutation %q", mutation.name)
	}
}

func TestTransferCircuitRejectsTransferOwnedMutations(t *testing.T) {
	mutations := []transferMutation{
		{
			name: "statement hash",
			mutate: func(c *circuits.TransferCircuit) {
				c.ClaimedStatementHash = mutateFieldByOne(c.ClaimedStatementHash)
			},
		},
		{
			name: "spend nullifier",
			mutate: func(c *circuits.TransferCircuit) {
				c.Spends[0].Nullifier = mutateFieldByOne(c.Spends[0].Nullifier)
			},
		},
		{
			name: "randomized verification key",
			mutate: func(c *circuits.TransferCircuit) {
				c.Spends[0].RK.X = mutateFieldByOne(c.Spends[0].RK.X)
			},
		},
		{
			name: "state path",
			mutate: func(c *circuits.TransferCircuit) {
				c.Spends[0].StateProof.Path[0][0] = mutateFieldByOne(c.Spends[0].StateProof.Path[0][0])
			},
		},
		{
			name: "output note commitment",
			mutate: func(c *circuits.TransferCircuit) {
				c.Outputs[0].NoteCommitment = mutateFieldByOne(c.Outputs[0].NoteCommitment)
			},
		},
		{
			name: "balance commitment",
			mutate: func(c *circuits.TransferCircuit) {
				c.BalanceCommitment.X = mutateFieldByOne(c.BalanceCommitment.X)
			},
		},
		{
			name: "output ordering",
			mutate: func(c *circuits.TransferCircuit) {
				c.Outputs[0].IsReceiver = 0
				c.Outputs[1].IsReceiver = 1
			},
		},
	}

	for _, mutation := range mutations {
		t.Run(mutation.name, func(t *testing.T) {
			assertTransferMutationRejected(t, mutation)
		})
	}
}

func TestTransferCircuitRejectsComplianceTierMutations(t *testing.T) {
	for _, mutation := range complianceTierMutations() {
		t.Run(mutation.name, func(t *testing.T) {
			assertTransferMutationRejected(t, mutation)
		})
	}
}

func complianceTierMutations() []transferMutation {
	var mutations []transferMutation
	mutations = append(mutations, senderCoreMutations()...)
	mutations = append(mutations, senderExtMutations()...)
	mutations = append(mutations, outputCoreMutations()...)
	mutations = append(mutations, outputExtMutations()...)
	return mutations
}

func senderCoreMutations() []transferMutation {
	return []transferMutation{
		{name: "sender core ciphertext", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderCore.Ciphertext[0] = mutateFieldByOne(c.Compliance.SenderCore.Ciphertext[0])
		}},
		{name: "sender core c2", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderCore.C2 = mutateFieldByOne(c.Compliance.SenderCore.C2)
		}},
		{name: "sender core swapped statement", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderCore.Proof.Statement = c.Compliance.SenderExt.Proof.Statement
		}},
		{name: "sender core swapped challenge", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderCore.Proof.Challenge = c.Compliance.SenderExt.Proof.Challenge
		}},
		{name: "sender core derived pk", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderCore.Proof.DerivedPK.X = mutateFieldByOne(c.Compliance.SenderCore.Proof.DerivedPK.X)
		}},
	}
}

func senderExtMutations() []transferMutation {
	return []transferMutation{
		{name: "sender ext ciphertext", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderExt.Ciphertext[0] = mutateFieldByOne(c.Compliance.SenderExt.Ciphertext[0])
		}},
		{name: "sender ext c2", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderExt.C2 = mutateFieldByOne(c.Compliance.SenderExt.C2)
		}},
		{name: "sender ext swapped statement", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderExt.Proof.Statement = c.Compliance.SenderCore.Proof.Statement
		}},
		{name: "sender ext swapped challenge", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderExt.Proof.Challenge = c.Compliance.SenderCore.Proof.Challenge
		}},
		{name: "sender ext derived pk", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.SenderExt.Proof.DerivedPK.X = mutateFieldByOne(c.Compliance.SenderExt.Proof.DerivedPK.X)
		}},
	}
}

func outputCoreMutations() []transferMutation {
	return []transferMutation{
		{name: "output core ciphertext", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputCore.Ciphertext[0] = mutateFieldByOne(c.Compliance.OutputCore.Ciphertext[0])
		}},
		{name: "output core c2", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputCore.C2 = mutateFieldByOne(c.Compliance.OutputCore.C2)
		}},
		{name: "output core swapped statement", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputCore.Proof.Statement = c.Compliance.OutputExt.Proof.Statement
		}},
		{name: "output core swapped challenge", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputCore.Proof.Challenge = c.Compliance.OutputExt.Proof.Challenge
		}},
		{name: "output core derived pk", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputCore.Proof.DerivedPK.X = mutateFieldByOne(c.Compliance.OutputCore.Proof.DerivedPK.X)
		}},
	}
}

func outputExtMutations() []transferMutation {
	return []transferMutation{
		{name: "output ext ciphertext", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputExt.Ciphertext[0] = mutateFieldByOne(c.Compliance.OutputExt.Ciphertext[0])
		}},
		{name: "output ext c2", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputExt.C2 = mutateFieldByOne(c.Compliance.OutputExt.C2)
		}},
		{name: "output ext swapped statement", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputExt.Proof.Statement = c.Compliance.OutputCore.Proof.Statement
		}},
		{name: "output ext swapped challenge", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputExt.Proof.Challenge = c.Compliance.OutputCore.Proof.Challenge
		}},
		{name: "output ext derived pk", mutate: func(c *circuits.TransferCircuit) {
			c.Compliance.OutputExt.Proof.DerivedPK.X = mutateFieldByOne(c.Compliance.OutputExt.Proof.DerivedPK.X)
		}},
	}
}

func TestTransferCircuitRejectsTierMetadataMutations(t *testing.T) {
	for _, mutation := range tierMetadataMutations() {
		t.Run(mutation.name, func(t *testing.T) {
			assertTransferMutationRejected(t, mutation)
		})
	}
}

func tierMetadataMutations() []transferMutation {
	type tier struct {
		name  string
		apply func(*circuits.TransferCircuit, func(*circuits.TransferComplianceStatementFields))
	}
	tiers := []tier{
		{name: "sender core", apply: func(c *circuits.TransferCircuit, mutate func(*circuits.TransferComplianceStatementFields)) {
			mutate(&c.Compliance.SenderCore.Proof.Statement)
		}},
		{name: "sender ext", apply: func(c *circuits.TransferCircuit, mutate func(*circuits.TransferComplianceStatementFields)) {
			mutate(&c.Compliance.SenderExt.Proof.Statement)
		}},
		{name: "output core", apply: func(c *circuits.TransferCircuit, mutate func(*circuits.TransferComplianceStatementFields)) {
			mutate(&c.Compliance.OutputCore.Proof.Statement)
		}},
		{name: "output ext", apply: func(c *circuits.TransferCircuit, mutate func(*circuits.TransferComplianceStatementFields)) {
			mutate(&c.Compliance.OutputExt.Proof.Statement)
		}},
	}
	fields := []struct {
		name   string
		mutate func(*circuits.TransferComplianceStatementFields)
	}{
		{name: "tier label", mutate: func(s *circuits.TransferComplianceStatementFields) {
			s.Tier = mutateFieldByOne(s.Tier)
		}},
		{name: "policy hash", mutate: func(s *circuits.TransferComplianceStatementFields) {
			s.PolicyIDHash = mutateFieldByOne(s.PolicyIDHash)
		}},
		{name: "resource hash", mutate: func(s *circuits.TransferComplianceStatementFields) {
			s.ResourceHash = mutateFieldByOne(s.ResourceHash)
		}},
		{name: "permission hash", mutate: func(s *circuits.TransferComplianceStatementFields) {
			s.PermissionHash = mutateFieldByOne(s.PermissionHash)
		}},
		{name: "timestamp", mutate: func(s *circuits.TransferComplianceStatementFields) {
			s.TargetTimestamp = mutateFieldByOne(s.TargetTimestamp)
		}},
		{name: "salt", mutate: func(s *circuits.TransferComplianceStatementFields) {
			s.Salt = mutateFieldByOne(s.Salt)
		}},
	}

	var mutations []transferMutation
	for _, tier := range tiers {
		for _, field := range fields {
			tier := tier
			field := field
			mutations = append(mutations, transferMutation{
				name: tier.name + " " + field.name,
				mutate: func(c *circuits.TransferCircuit) {
					tier.apply(c, field.mutate)
				},
			})
		}
	}
	return mutations
}

func TestTransferCircuitRejectsTierProofPointMutations(t *testing.T) {
	for _, mutation := range tierProofPointMutations() {
		t.Run(mutation.name, func(t *testing.T) {
			assertTransferMutationRejected(t, mutation)
		})
	}
}

func tierProofPointMutations() []transferMutation {
	type tier struct {
		name  string
		apply func(*circuits.TransferCircuit, func(*circuits.TransferComplianceProofFields))
	}
	tiers := []tier{
		{name: "sender core", apply: func(c *circuits.TransferCircuit, mutate func(*circuits.TransferComplianceProofFields)) {
			mutate(&c.Compliance.SenderCore.Proof)
		}},
		{name: "sender ext", apply: func(c *circuits.TransferCircuit, mutate func(*circuits.TransferComplianceProofFields)) {
			mutate(&c.Compliance.SenderExt.Proof)
		}},
		{name: "output core", apply: func(c *circuits.TransferCircuit, mutate func(*circuits.TransferComplianceProofFields)) {
			mutate(&c.Compliance.OutputCore.Proof)
		}},
		{name: "output ext", apply: func(c *circuits.TransferCircuit, mutate func(*circuits.TransferComplianceProofFields)) {
			mutate(&c.Compliance.OutputExt.Proof)
		}},
	}
	fields := []struct {
		name   string
		mutate func(*circuits.TransferComplianceProofFields)
	}{
		{name: "shared point", mutate: func(p *circuits.TransferComplianceProofFields) {
			p.SharedPoint.X = mutateFieldByOne(p.SharedPoint.X)
		}},
		{name: "enc commitment", mutate: func(p *circuits.TransferComplianceProofFields) {
			p.EncCmt.X = mutateFieldByOne(p.EncCmt.X)
		}},
		{name: "response", mutate: func(p *circuits.TransferComplianceProofFields) {
			p.Response = mutateFieldByOne(p.Response)
		}},
	}

	var mutations []transferMutation
	for _, tier := range tiers {
		for _, field := range fields {
			tier := tier
			field := field
			mutations = append(mutations, transferMutation{
				name: tier.name + " " + field.name,
				mutate: func(c *circuits.TransferCircuit) {
					tier.apply(c, field.mutate)
				},
			})
		}
	}
	return mutations
}
