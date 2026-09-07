package circuits_test

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/test"
	"github.com/mizufinance/shieldd/tools/gnark/internal/abi"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"github.com/mizufinance/shieldd/tools/gnark/internal/testfixtures"
)

type circuitFamily struct {
	name            string
	circuit         func() frontend.Circuit
	assignment      func(t *testing.T) frontend.Circuit
	mutateStatement func(frontend.Circuit)
}

func mutateFieldByOne(value frontend.Variable) frontend.Variable {
	switch v := value.(type) {
	case string:
		n, ok := new(big.Int).SetString(v, 10)
		if !ok {
			panic("invalid decimal frontend.Variable string")
		}
		return new(big.Int).Add(n, big.NewInt(1)).String()
	case *big.Int:
		return new(big.Int).Add(v, big.NewInt(1))
	case big.Int:
		return new(big.Int).Add(&v, big.NewInt(1))
	case int:
		return v + 1
	case int64:
		return v + 1
	case uint64:
		return v + 1
	default:
		panic("unsupported frontend.Variable type for mutation")
	}
}

func variableIsOne(value frontend.Variable) bool {
	switch v := value.(type) {
	case string:
		return v == "1"
	case *big.Int:
		return v.Cmp(big.NewInt(1)) == 0
	case big.Int:
		return v.Cmp(big.NewInt(1)) == 0
	case int:
		return v == 1
	case int64:
		return v == 1
	case uint64:
		return v == 1
	default:
		return false
	}
}

func variableIsZero(value frontend.Variable) bool {
	switch v := value.(type) {
	case string:
		return v == "0"
	case *big.Int:
		return v.Sign() == 0
	case big.Int:
		return v.Sign() == 0
	case int:
		return v == 0
	case int64:
		return v == 0
	case uint64:
		return v == 0
	default:
		return false
	}
}

func testCircuitFamilies() []circuitFamily {
	return []circuitFamily{
		{
			name:    "transfer",
			circuit: func() frontend.Circuit { return circuits.NewTransferCircuit() },
			assignment: func(t *testing.T) frontend.Circuit {
				t.Helper()
				fixtureBytes := testfixtures.LoadTransferWitness("transfer")
				assignment, _, err := abi.NewTransferCircuitAssignmentFromWitness(fixtureBytes)
				if err != nil {
					t.Fatalf("decode transfer witness fixture: %v", err)
				}
				return assignment
			},
			mutateStatement: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.TransferCircuit)
				a.ClaimedStatementHash = mutateFieldByOne(a.ClaimedStatementHash)
			},
		},
		{
			name:    "shielded_ics20_withdrawal",
			circuit: func() frontend.Circuit { return circuits.NewShieldedIcs20WithdrawalCircuit(2) },
			assignment: func(t *testing.T) frontend.Circuit {
				t.Helper()
				fixtureBytes := testfixtures.LoadShieldedIcs20WithdrawalWitness("shielded_ics20_withdrawal")
				assignment, _, err := abi.NewShieldedIcs20WithdrawalCircuitAssignmentFromWitness(fixtureBytes)
				if err != nil {
					t.Fatalf("decode shielded ICS-20 withdrawal witness fixture: %v", err)
				}
				return assignment
			},
			mutateStatement: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.ShieldedIcs20WithdrawalCircuit)
				a.ClaimedStatementHash = mutateFieldByOne(a.ClaimedStatementHash)
			},
		},
		{
			name: "note_reshape8x1",
			circuit: func() frontend.Circuit {
				return circuits.NewNoteReshapeCircuit("note_reshape8x1", 8, 1)
			},
			assignment: func(t *testing.T) frontend.Circuit {
				t.Helper()
				fixtureBytes := testfixtures.LoadNoteReshapeWitness("note_reshape8x1")
				assignment, _, err := abi.NewNoteReshapeCircuitAssignmentFromWitness(fixtureBytes)
				if err != nil {
					t.Fatalf("decode note reshape witness fixture: %v", err)
				}
				return assignment
			},
			mutateStatement: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.NoteReshapeCircuit)
				a.ClaimedStatementHash = mutateFieldByOne(a.ClaimedStatementHash)
			},
		},
		{
			name:    "note_reshape1x8",
			circuit: func() frontend.Circuit { return circuits.NewNoteReshapeCircuit("note_reshape1x8", 1, 8) },
			assignment: func(t *testing.T) frontend.Circuit {
				t.Helper()
				fixtureBytes := testfixtures.LoadNoteReshapeWitness("note_reshape1x8")
				assignment, _, err := abi.NewNoteReshapeCircuitAssignmentFromWitness(fixtureBytes)
				if err != nil {
					t.Fatalf("decode note reshape witness fixture: %v", err)
				}
				return assignment
			},
			mutateStatement: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.NoteReshapeCircuit)
				a.ClaimedStatementHash = mutateFieldByOne(a.ClaimedStatementHash)
			},
		},
	}
}

type circuitStats struct {
	constraints int
	public      int
	secret      int
	internal    int
}

func compileCircuitFamilies() []struct {
	name    string
	circuit func() frontend.Circuit
	stats   circuitStats
} {
	return []struct {
		name    string
		circuit func() frontend.Circuit
		stats   circuitStats
	}{
		{
			name:    "transfer",
			circuit: func() frontend.Circuit { return circuits.NewTransferCircuit() },
			stats:   circuitStats{constraints: 163396, public: 2, secret: 518, internal: 150384},
		},
		{
			name:    "note_reshape8x1",
			circuit: func() frontend.Circuit { return circuits.NewNoteReshapeCircuit("note_reshape8x1", 8, 1) },
			stats:   circuitStats{constraints: 156742, public: 2, secret: 825, internal: 147761},
		},
		{
			name:    "note_reshape1x8",
			circuit: func() frontend.Circuit { return circuits.NewNoteReshapeCircuit("note_reshape1x8", 1, 8) },
			stats:   circuitStats{constraints: 122896, public: 2, secret: 333, internal: 112226},
		},
		{
			name:    "shielded_ics20_withdrawal",
			circuit: func() frontend.Circuit { return circuits.NewShieldedIcs20WithdrawalCircuit(2) },
			stats:   circuitStats{constraints: 102571, public: 2, secret: 418, internal: 94827},
		},
	}
}

func TestCircuitFamiliesCompile(t *testing.T) {
	for _, family := range compileCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			ccs, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, family.circuit())
			if err != nil {
				t.Fatalf("compile %s circuit: %v", family.name, err)
			}
			stats := circuitStats{
				constraints: ccs.GetNbConstraints(),
				public:      ccs.GetNbPublicVariables(),
				secret:      ccs.GetNbSecretVariables(),
				internal:    ccs.GetNbInternalVariables(),
			}
			if stats != family.stats {
				t.Fatalf("unexpected %s circuit stats: got %+v, want %+v", family.name, stats, family.stats)
			}
		})
	}
}

func TestCircuitFamiliesAcceptValidAssignment(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithValidAssignment(family.assignment(t)),
			)
		})
	}
}

func TestShieldedIcs20WithdrawalAccumulatorBranchesAcceptValidAssignment(t *testing.T) {
	for _, label := range []string{
		"shielded_ics20_withdrawal_accumulator_origin",
		"shielded_ics20_withdrawal_accumulator_continuation",
	} {
		t.Run(label, func(t *testing.T) {
			fixture := testfixtures.LoadShieldedIcs20WithdrawalWitness(label)
			assignment, _, err := abi.NewShieldedIcs20WithdrawalCircuitAssignmentFromWitness(fixture)
			if err != nil {
				t.Fatalf("decode withdrawal fixture: %v", err)
			}

			assert := test.NewAssert(t)
			assert.CheckCircuit(
				circuits.NewShieldedIcs20WithdrawalCircuit(2),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithValidAssignment(assignment),
			)
		})
	}
}

func TestCircuitFamiliesRejectWrongStatementHash(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			assignment := family.assignment(t)
			family.mutateStatement(assignment)

			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithInvalidAssignment(assignment),
			)
		})
	}
}

func TestCircuitFamiliesRejectMutatedComplianceField(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			var assignment frontend.Circuit
			switch family.name {
			case "transfer":
				witness, transfer := loadTransferAssignment(t)
				witness.Metadata.SenderCoreSalt = addFieldElementBytes(
					t,
					witness.Metadata.SenderCoreSalt,
					big.NewInt(1),
				)
				transfer.Compliance.Metadata.SenderCoreSalt =
					primitives.LittleEndianBytesToBigInt(
						witness.Metadata.SenderCoreSalt[:],
					).String()
				setTransferStatementHash(t, witness, transfer)
				assignment = transfer
			case "shielded_ics20_withdrawal":
				_, withdrawal, _ := loadWithdrawalFixture(t)
				withdrawal.WithdrawalEffectHashLimbs[0] =
					mutateFieldByOne(withdrawal.WithdrawalEffectHashLimbs[0])
				assignment = withdrawal
			case "note_reshape1x8":
				_, noteReshape :=
					loadNoteReshapeRegressionWitnessAndAssignment(
						t,
						family.name,
					)
				noteReshape.Outputs[0].Note.Amount =
					mutateFieldByOne(noteReshape.Outputs[0].Note.Amount)
				assignment = noteReshape
			default:
				witness, noteReshape :=
					loadNoteReshapeRegressionWitnessAndAssignment(
						t,
						family.name,
					)
				witness.Spends[0].Nullifier = addFieldElementBytes(
					t,
					witness.Spends[0].Nullifier,
					big.NewInt(1),
				)
				mutatedNullifier := primitives.LittleEndianBytesToBigInt(
					witness.Spends[0].Nullifier[:],
				).String()
				if len(noteReshape.SyntheticSpends) != 0 {
					noteReshape.SyntheticSpends[0].Nullifier = mutatedNullifier
				} else {
					noteReshape.Spends[0].Nullifier = mutatedNullifier
				}
				setNoteReshapeStatementHash(
					t,
					family.name,
					witness,
					noteReshape,
				)
				assignment = noteReshape
			}

			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithInvalidAssignment(assignment),
			)
		})
	}
}

func TestCircuitFamiliesRejectMutatedBalanceCommitment(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			var assignment frontend.Circuit
			switch family.name {
			case "transfer":
				_, transfer := loadTransferAssignment(t)
				transfer.ActionBalanceBlinding =
					mutateFieldByOne(transfer.ActionBalanceBlinding)
				assignment = transfer
			case "shielded_ics20_withdrawal":
				_, withdrawal, _ := loadWithdrawalFixture(t)
				withdrawal.ActionBalanceBlinding =
					mutateFieldByOne(withdrawal.ActionBalanceBlinding)
				assignment = withdrawal
			default:
				witness, noteReshape :=
					loadNoteReshapeRegressionWitnessAndAssignment(
						t,
						family.name,
					)
				if witness.BalanceCommitmentAffine == witness.AKAffine {
					t.Fatalf(
						"%s fixture needs a distinct valid balance point mutation",
						family.name,
					)
				}
				witness.BalanceCommitmentAffine = witness.AKAffine
				noteReshape.BalanceCommitment = noteReshape.Auth.AK
				setNoteReshapeStatementHash(
					t,
					family.name,
					witness,
					noteReshape,
				)
				assignment = noteReshape
			}

			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithInvalidAssignment(assignment),
			)
		})
	}
}

func TestCircuitFamiliesRejectMutatedNullifier(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			var assignment frontend.Circuit
			switch family.name {
			case "transfer":
				witness, transfer := loadTransferAssignment(t)
				witness.RequiredSpend.Nullifier = addFieldElementBytes(
					t,
					witness.RequiredSpend.Nullifier,
					big.NewInt(1),
				)
				transfer.RequiredSpend.Nullifier =
					primitives.LittleEndianBytesToBigInt(
						witness.RequiredSpend.Nullifier[:],
					).String()
				setTransferStatementHash(t, witness, transfer)
				assignment = transfer
			case "shielded_ics20_withdrawal":
				witness, withdrawal, nIn := loadWithdrawalFixture(t)
				witness.RequiredSpend.Nullifier = addFieldElementBytes(
					t,
					witness.RequiredSpend.Nullifier,
					big.NewInt(1),
				)
				withdrawal.RequiredSpend.Nullifier =
					primitives.LittleEndianBytesToBigInt(
						witness.RequiredSpend.Nullifier[:],
					).String()
				setWithdrawalStatementHash(t, witness, withdrawal, nIn)
				assignment = withdrawal
			default:
				witness, noteReshape :=
					loadNoteReshapeRegressionWitnessAndAssignment(
						t,
						family.name,
					)
				witness.Spends[0].Nullifier = addFieldElementBytes(
					t,
					witness.Spends[0].Nullifier,
					big.NewInt(1),
				)
				mutatedNullifier := primitives.LittleEndianBytesToBigInt(
					witness.Spends[0].Nullifier[:],
				).String()
				if len(noteReshape.SyntheticSpends) != 0 {
					noteReshape.SyntheticSpends[0].Nullifier = mutatedNullifier
				} else {
					noteReshape.Spends[0].Nullifier = mutatedNullifier
				}
				setNoteReshapeStatementHash(
					t,
					family.name,
					witness,
					noteReshape,
				)
				assignment = noteReshape
			}

			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithInvalidAssignment(assignment),
			)
		})
	}
}

func TestPaddedSpendCircuitsRejectMutatedDummyNullifierSeed(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		if family.name != "transfer" && family.name != "shielded_ics20_withdrawal" {
			continue
		}
		t.Run(family.name, func(t *testing.T) {
			var assignment frontend.Circuit
			switch family.name {
			case "transfer":
				fixture := testfixtures.LoadTransferWitness("transfer_flagged")
				transfer, _, err :=
					abi.NewTransferCircuitAssignmentFromWitness(fixture)
				if err != nil {
					t.Fatalf("decode dummy transfer fixture: %v", err)
				}
				assignment = transfer
			case "shielded_ics20_withdrawal":
				fixture := testfixtures.LoadShieldedIcs20WithdrawalWitness(
					"shielded_ics20_withdrawal_unregulated",
				)
				withdrawal, _, err :=
					abi.NewShieldedIcs20WithdrawalCircuitAssignmentFromWitness(
						fixture,
					)
				if err != nil {
					t.Fatalf("decode dummy withdrawal fixture: %v", err)
				}
				assignment = withdrawal
			default:
				t.Fatalf("unsupported padded-spend family %s", family.name)
			}
			mutated := false
			switch a := assignment.(type) {
			case *circuits.TransferCircuit:
				if variableIsOne(a.OptionalSpend.IsDummy) {
					a.OptionalSpend.DummyNullifierSeed =
						mutateFieldByOne(a.OptionalSpend.DummyNullifierSeed)
					mutated = true
				}
			case *circuits.ShieldedIcs20WithdrawalCircuit:
				if variableIsOne(a.OptionalSpend.IsDummy) {
					a.OptionalSpend.DummyNullifierSeed =
						mutateFieldByOne(a.OptionalSpend.DummyNullifierSeed)
					mutated = true
				}
			default:
				t.Fatalf("unsupported assignment type %T", assignment)
			}
			if !mutated {
				t.Fatalf("%s dummy fixture has no dummy spend to mutate", family.name)
			}

			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithInvalidAssignment(assignment),
			)
		})
	}
}

func TestNoteReshapeRejectsDummyOutputCommitmentMutation(t *testing.T) {
	witness, noteReshape := loadNoteReshapeRegressionWitnessAndAssignment(
		t,
		"note_reshape1x8",
	)
	mutated := false
	for index, output := range noteReshape.Outputs {
		if variableIsZero(output.Note.Amount) {
			witness.Outputs[index].NoteCommitment = addFieldElementBytes(
				t,
				witness.Outputs[index].NoteCommitment,
				big.NewInt(1),
			)
			noteReshape.Outputs[index].NoteCommitment =
				primitives.LittleEndianBytesToBigInt(
					witness.Outputs[index].NoteCommitment[:],
				).String()
			mutated = true
			break
		}
	}
	if !mutated {
		t.Fatal("note_reshape1x8 fixture must contain a dummy output")
	}
	setNoteReshapeStatementHash(
		t,
		"note_reshape1x8",
		witness,
		noteReshape,
	)

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		circuits.NewNoteReshapeCircuit("note_reshape1x8", 1, 8),
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithInvalidAssignment(noteReshape),
	)
}

func TestNoteReshapeRejectsPaddedOutputPayloadMutation(t *testing.T) {
	fixtureBytes := testfixtures.LoadNoteReshapeWitness("note_reshape1x8")
	assignment, _, err := abi.NewNoteReshapeCircuitAssignmentFromWitness(fixtureBytes)
	if err != nil {
		t.Fatalf("decode note reshape witness fixture: %v", err)
	}
	noteReshape := assignment
	mutated := false
	for index, output := range noteReshape.Outputs {
		if variableIsZero(output.Note.Amount) {
			noteReshape.Outputs[index].Note.Amount = mutateFieldByOne(output.Note.Amount)
			mutated = true
			break
		}
	}
	if !mutated {
		t.Fatal("note_reshape1x8 fixture must contain a zero-note padded output")
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		circuits.NewNoteReshapeCircuit("note_reshape1x8", 1, 8),
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithInvalidAssignment(noteReshape),
	)
}
