package circuits_test

import (
	"fmt"
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/shieldd/tools/gnark/internal/abi"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"github.com/mizufinance/shieldd/tools/gnark/internal/testfixtures"
	"golang.org/x/crypto/blake2b"
)

func loadNoteReshapeRegressionAssignment(t *testing.T, label string) *circuits.NoteReshapeCircuit {
	t.Helper()
	_, assignment := loadNoteReshapeRegressionWitnessAndAssignment(t, label)
	return assignment
}

type noteReshapeStatusGateCircuit struct {
	Status      frontend.Variable
	IsRegulated frontend.Variable
}

func (c *noteReshapeStatusGateCircuit) Define(api frontend.API) error {
	compliance.AssertEqualIf(api, c.Status, 1, c.IsRegulated)
	return nil
}

func TestNoteReshapeStatusGateRejectsFrozenRegulatedOwner(t *testing.T) {
	circuit := &noteReshapeStatusGateCircuit{}
	assert := test.NewAssert(t)
	assert.SolvingSucceeded(circuit, &noteReshapeStatusGateCircuit{
		Status:      2,
		IsRegulated: 0,
	})
	assert.SolvingFailed(circuit, &noteReshapeStatusGateCircuit{
		Status:      2,
		IsRegulated: 1,
	})
}

func TestNoteReshapeRejectsRegulatedNullifierRegistrationMutations(t *testing.T) {
	for _, label := range []string{"note_reshape1x8", "note_reshape8x1"} {
		label := label
		for _, mutation := range []struct {
			name   string
			mutate func(*circuits.NoteReshapeCircuit)
		}{
			{
				name: "dh key",
				mutate: func(c *circuits.NoteReshapeCircuit) {
					c.Sender.RnkDhPk.X = mutateFieldByOne(c.Sender.RnkDhPk.X)
				},
			},
			{
				name: "commitment",
				mutate: func(c *circuits.NoteReshapeCircuit) {
					c.Sender.RnkCommitment = mutateFieldByOne(c.Sender.RnkCommitment)
				},
			},
		} {
			mutation := mutation
			t.Run(label+"/"+mutation.name, func(t *testing.T) {
				assignment := loadNoteReshapeRegressionAssignment(t, label)
				mutation.mutate(assignment)
				family, ok := generated.NoteReshapeFamilyByLabel(label)
				if !ok {
					t.Fatalf("unknown note reshape family %s", label)
				}
				if err := test.IsSolved(
					circuits.NewNoteReshapeCircuit(label, family.NIn, family.NOut),
					assignment,
					ecc.BLS12_377.ScalarField(),
				); err == nil {
					t.Fatalf("%s accepted a mutated regulated nullifier %s", label, mutation.name)
				}
			})
		}
	}
}

func loadNoteReshapeRegressionWitnessAndAssignment(
	t *testing.T,
	label string,
) (*abi.NoteReshapeWitnessBinary, *circuits.NoteReshapeCircuit) {
	t.Helper()
	fixture := testfixtures.LoadNoteReshapeWitness(label)
	witness, _, err := abi.DecodeNoteReshapeWitness(fixture)
	if err != nil {
		t.Fatalf("decode %s binary witness fixture: %v", label, err)
	}
	assignment, _, err := abi.NewNoteReshapeCircuitAssignmentFromWitness(fixture)
	if err != nil {
		t.Fatalf("decode %s witness fixture: %v", label, err)
	}
	return witness, assignment
}

func setNoteReshapeStatementHash(
	t *testing.T,
	label string,
	witness *abi.NoteReshapeWitnessBinary,
	assignment *circuits.NoteReshapeCircuit,
) {
	t.Helper()
	fields, err := abi.ReconstructedNoteReshapeStatementFieldsFromWitness(witness)
	if err != nil {
		t.Fatalf("reconstruct %s statement fields: %v", label, err)
	}
	statementHash, err := primitives.NoteReshapeStatementHashNativeForShape(
		fieldElementStrings(fields),
		label,
		int(witness.NIn),
		int(witness.NOut),
	)
	if err != nil {
		t.Fatalf("compute %s statement hash: %v", label, err)
	}
	assignment.ClaimedStatementHash = statementHash.String()
}

func compileNoteReshapeRegressionCircuit(t *testing.T, label string, nIn, nOut int) constraint.ConstraintSystem {
	t.Helper()
	ccs, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		circuits.NewNoteReshapeCircuit(label, nIn, nOut),
	)
	if err != nil {
		t.Fatalf("compile %s circuit: %v", label, err)
	}
	return ccs
}

func noteReshapeAssignmentSolved(t *testing.T, ccs constraint.ConstraintSystem, assignment *circuits.NoteReshapeCircuit) error {
	t.Helper()
	witness, err := frontend.NewWitness(assignment, ecc.BLS12_377.ScalarField())
	if err != nil {
		return fmt.Errorf("build witness: %w", err)
	}
	return ccs.IsSolved(witness)
}

func noteReshapeDomainHashNative(fields []*big.Int, label string) (*big.Int, error) {
	constant := func(suffix string) *big.Int {
		// Statement domains use BLAKE2b-512, and are reduced by the same
		// little-endian conversion as the production native helper.
		domain := fmt.Sprintf("shieldd.shielded_pool.%s.public_input_hash.%s", label, suffix)
		sum := blake2b.Sum512([]byte(domain))
		value := primitives.LittleEndianBytesToBigInt(sum[:])
		return value
	}
	if len(fields) == 0 {
		return nil, fmt.Errorf("empty statement field list")
	}

	pad0 := constant("pad0")
	pad1 := constant("pad1")
	domain := constant("statement")
	first := [7]*big.Int{pad0, pad1, pad0, pad1, pad0, pad1, pad0}
	for i := 0; i < len(first) && i < len(fields); i++ {
		first[i] = fields[i]
	}
	h, err := primitives.Poseidon377Hash7Native(domain, first)
	if err != nil {
		return nil, err
	}
	idx := len(first)
	for idx+6 <= len(fields) {
		h, err = primitives.Poseidon377Hash7Native(domain, [7]*big.Int{
			h,
			fields[idx],
			fields[idx+1],
			fields[idx+2],
			fields[idx+3],
			fields[idx+4],
			fields[idx+5],
		})
		if err != nil {
			return nil, err
		}
		idx += 6
	}
	if idx < len(fields) {
		tail := [6]*big.Int{pad0, pad1, pad0, pad1, pad0, pad1}
		for i, value := range fields[idx:] {
			tail[i] = value
		}
		return primitives.Poseidon377Hash7Native(domain, [7]*big.Int{
			h,
			tail[0],
			tail[1],
			tail[2],
			tail[3],
			tail[4],
			tail[5],
		})
	}
	return h, nil
}

func noteReshapeTransmissionKeyFQNative(
	t *testing.T,
	witness *abi.NoteReshapeWitnessBinary,
) *big.Int {
	t.Helper()
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}
	ivkModQ, err := primitives.Poseidon377Hash2Native(
		primitives.MustBigInt(vectors.Poseidon377.IVKDomain),
		[2]*big.Int{
			primitives.LittleEndianBytesToBigInt(witness.NK[:]),
			compressedPointFromBinary(t, witness.AKAffine),
		},
	)
	if err != nil {
		t.Fatalf("compute NoteReshape incoming viewing key: %v", err)
	}
	scalarOrder := primitives.MustBigInt(
		vectors.Decaf377CompanionCurve.Order,
	)
	ivkReduced := new(big.Int).Mod(
		new(big.Int).Set(ivkModQ),
		scalarOrder,
	)
	transmission, err := decafgnark.ScalarMulNative(
		gnarkte.Point{
			X: primitives.LittleEndianBytesToBigInt(witness.Shared.DivGen.X[:]),
			Y: primitives.LittleEndianBytesToBigInt(witness.Shared.DivGen.Y[:]),
		},
		ivkReduced,
		scalarOrder.BitLen(),
	)
	if err != nil {
		t.Fatalf("compute NoteReshape diversified transmission key: %v", err)
	}
	compressed, err := decafgnark.CompressToFieldNative(transmission)
	if err != nil {
		t.Fatalf("compress NoteReshape diversified transmission key: %v", err)
	}
	return compressed
}

func noteReshapeSpendAssignment(
	t *testing.T,
	family generated.NoteReshapeFamilySpec,
	assignment *circuits.NoteReshapeCircuit,
	index int,
) *circuits.NoteReshapeSpendCircuitFields {
	t.Helper()
	if family.InputPadding == generated.InputPaddingSyntheticPrivate {
		if len(assignment.SyntheticSpends) != family.NIn {
			t.Fatalf(
				"%s synthetic assignment length = %d, want %d",
				family.Label,
				len(assignment.SyntheticSpends),
				family.NIn,
			)
		}
		return &assignment.SyntheticSpends[index].NoteReshapeSpendCircuitFields
	}
	if len(assignment.Spends) != family.NIn {
		t.Fatalf(
			"%s fixed assignment length = %d, want %d",
			family.Label,
			len(assignment.Spends),
			family.NIn,
		)
	}
	return &assignment.Spends[index]
}

func TestNoteReshapeEverySpendAndOutputPublicFieldIsConstrained(t *testing.T) {
	for _, family := range generated.NoteReshapeFamilies {
		family := family
		t.Run(family.Label, func(t *testing.T) {
			baseWitness, baseAssignment :=
				loadNoteReshapeRegressionWitnessAndAssignment(t, family.Label)
			if len(baseWitness.Spends) != family.NIn {
				t.Fatalf(
					"%s witness spend length = %d, want %d",
					family.Label,
					len(baseWitness.Spends),
					family.NIn,
				)
			}
			if len(baseWitness.Outputs) != family.NOut {
				t.Fatalf(
					"%s witness output length = %d, want %d",
					family.Label,
					len(baseWitness.Outputs),
					family.NOut,
				)
			}
			ccs := compileNoteReshapeRegressionCircuit(
				t,
				family.Label,
				family.NIn,
				family.NOut,
			)
			if err := noteReshapeAssignmentSolved(
				t,
				ccs,
				baseAssignment,
			); err != nil {
				t.Fatalf("%s canonical fixture must solve: %v", family.Label, err)
			}

			for spendIndex := range baseWitness.Spends {
				spendIndex := spendIndex
				t.Run(
					fmt.Sprintf("spend_%d_nullifier", spendIndex),
					func(t *testing.T) {
						witness, assignment :=
							loadNoteReshapeRegressionWitnessAndAssignment(
								t,
								family.Label,
							)
						witness.Spends[spendIndex].Nullifier =
							addFieldElementBytes(
								t,
								witness.Spends[spendIndex].Nullifier,
								big.NewInt(1),
							)
						noteReshapeSpendAssignment(
							t,
							family,
							assignment,
							spendIndex,
						).Nullifier = primitives.LittleEndianBytesToBigInt(
							witness.Spends[spendIndex].Nullifier[:],
						).String()
						setNoteReshapeStatementHash(
							t,
							family.Label,
							witness,
							assignment,
						)
						if err := noteReshapeAssignmentSolved(
							t,
							ccs,
							assignment,
						); err == nil {
							t.Fatalf(
								"%s accepted rebound spend %d nullifier",
								family.Label,
								spendIndex,
							)
						}
					},
				)

				t.Run(
					fmt.Sprintf("spend_%d_randomized_verification_key", spendIndex),
					func(t *testing.T) {
						witness, assignment :=
							loadNoteReshapeRegressionWitnessAndAssignment(
								t,
								family.Label,
							)
						target := witness.Spends[spendIndex].RKAffine
						candidates := make(
							[]abi.PointAffineBinary,
							0,
							len(witness.Spends)+1,
						)
						for index := range witness.Spends {
							if index != spendIndex {
								candidates = append(
									candidates,
									witness.Spends[index].RKAffine,
								)
							}
						}
						candidates = append(candidates, witness.AKAffine)
						var (
							replacement abi.PointAffineBinary
							found       bool
						)
						for _, candidate := range candidates {
							if pointsHaveDistinctCompression(
								t,
								candidate,
								target,
							) {
								replacement = candidate
								found = true
								break
							}
						}
						if !found {
							t.Fatalf(
								"%s fixture must expose a valid point distinct from spend %d RK",
								family.Label,
								spendIndex,
							)
						}
						compressedPointFromBinary(t, replacement)
						witness.Spends[spendIndex].RKAffine = replacement
						noteReshapeSpendAssignment(
							t,
							family,
							assignment,
							spendIndex,
						).RK = circuitPointFromBinary(replacement)
						setNoteReshapeStatementHash(
							t,
							family.Label,
							witness,
							assignment,
						)
						err := noteReshapeAssignmentSolved(t, ccs, assignment)
						if witness.Spends[spendIndex].IsDummy {
							if err != nil {
								t.Fatalf(
									"%s rejected externally authorized dummy spend %d RK: %v",
									family.Label,
									spendIndex,
									err,
								)
							}
							return
						}
						if err == nil {
							t.Fatalf(
								"%s accepted rebound real spend %d RK",
								family.Label,
								spendIndex,
							)
						}
					},
				)
			}

			for outputIndex := range baseWitness.Outputs {
				outputIndex := outputIndex
				t.Run(
					fmt.Sprintf("output_%d_commitment", outputIndex),
					func(t *testing.T) {
						witness, assignment :=
							loadNoteReshapeRegressionWitnessAndAssignment(
								t,
								family.Label,
							)
						witness.Outputs[outputIndex].NoteCommitment =
							addFieldElementBytes(
								t,
								witness.Outputs[outputIndex].NoteCommitment,
								big.NewInt(1),
							)
						assignment.Outputs[outputIndex].NoteCommitment =
							primitives.LittleEndianBytesToBigInt(
								witness.Outputs[outputIndex].NoteCommitment[:],
							).String()
						setNoteReshapeStatementHash(
							t,
							family.Label,
							witness,
							assignment,
						)
						if err := noteReshapeAssignmentSolved(
							t,
							ccs,
							assignment,
						); err == nil {
							t.Fatalf(
								"%s accepted rebound output %d commitment",
								family.Label,
								outputIndex,
							)
						}
					},
				)
			}
		})
	}
}

func TestNoteReshapeFamiliesRejectIsolatedExactConservationMutation(
	t *testing.T,
) {
	for _, family := range generated.NoteReshapeFamilies {
		family := family
		t.Run(family.Label, func(t *testing.T) {
			ccs := compileNoteReshapeRegressionCircuit(
				t,
				family.Label,
				family.NIn,
				family.NOut,
			)
			witness, assignment :=
				loadNoteReshapeRegressionWitnessAndAssignment(t, family.Label)
			if len(witness.Outputs) != family.NOut || family.NOut == 0 {
				t.Fatalf(
					"%s witness output length = %d, want positive %d",
					family.Label,
					len(witness.Outputs),
					family.NOut,
				)
			}

			const outputIndex = 0
			amount := primitives.LittleEndianBytesToBigInt(
				witness.Outputs[outputIndex].CreatedNoteAmount[:],
			)
			amount.Add(amount, big.NewInt(1))
			if amount.BitLen() > 128 {
				t.Fatalf(
					"%s mutated output amount exceeds the u128 range: %s",
					family.Label,
					amount,
				)
			}
			witness.Outputs[outputIndex].CreatedNoteAmount =
				le32FromBigInt(t, amount)
			assignment.Outputs[outputIndex].Note.Amount = amount.String()

			commitment := noteCommitmentNativeFromFields(
				t,
				witness.Outputs[outputIndex].CreatedNoteBlinding,
				witness.Outputs[outputIndex].CreatedNoteAmount,
				witness.Shared.AssetID,
				compressedPointFromBinary(t, witness.Shared.DivGen),
				noteReshapeTransmissionKeyFQNative(t, witness),
				witness.Outputs[outputIndex].RecoveryCommitment,
			)
			if commitment.Cmp(
				primitives.LittleEndianBytesToBigInt(
					witness.Outputs[outputIndex].NoteCommitment[:],
				),
			) == 0 {
				t.Fatalf(
					"%s amount mutation unexpectedly preserved output %d commitment",
					family.Label,
					outputIndex,
				)
			}
			witness.Outputs[outputIndex].NoteCommitment =
				le32FromBigInt(t, commitment)
			assignment.Outputs[outputIndex].NoteCommitment =
				commitment.String()
			setNoteReshapeStatementHash(
				t,
				family.Label,
				witness,
				assignment,
			)

			if err := noteReshapeAssignmentSolved(
				t,
				ccs,
				assignment,
			); err == nil {
				t.Fatalf(
					"%s accepted a fully rebound output note with non-conserving amount",
					family.Label,
				)
			}
		})
	}
}

func TestNoteReshape1x8BindsEveryOutputCommitment(t *testing.T) {
	ccs := compileNoteReshapeRegressionCircuit(t, "note_reshape1x8", 1, 8)
	for outputIndex := 0; outputIndex < 8; outputIndex++ {
		t.Run(fmt.Sprintf("output_%d", outputIndex), func(t *testing.T) {
			witness, assignment := loadNoteReshapeRegressionWitnessAndAssignment(
				t,
				"note_reshape1x8",
			)
			witness.Outputs[outputIndex].NoteCommitment = addFieldElementBytes(
				t,
				witness.Outputs[outputIndex].NoteCommitment,
				big.NewInt(1),
			)
			assignment.Outputs[outputIndex].NoteCommitment =
				primitives.LittleEndianBytesToBigInt(
					witness.Outputs[outputIndex].NoteCommitment[:],
				).String()
			setNoteReshapeStatementHash(
				t,
				"note_reshape1x8",
				witness,
				assignment,
			)
			if err := noteReshapeAssignmentSolved(t, ccs, assignment); err == nil {
				t.Fatalf("mutating output %d commitment must invalidate the 1x8 witness", outputIndex)
			}
		})
	}
}

func TestNoteReshape1x8PaddedOutputFieldsAreBound(t *testing.T) {
	ccs := compileNoteReshapeRegressionCircuit(t, "note_reshape1x8", 1, 8)
	base := loadNoteReshapeRegressionAssignment(t, "note_reshape1x8")
	paddedIndex := -1
	for index, output := range base.Outputs {
		if variableIsZero(output.Note.Amount) {
			paddedIndex = index
			break
		}
	}
	if paddedIndex < 0 {
		t.Fatal("note_reshape1x8 fixture must contain a padded output")
	}

	mutations := []struct {
		name   string
		mutate func(*circuits.NoteReshapeCircuit)
	}{
		{name: "amount", mutate: func(c *circuits.NoteReshapeCircuit) {
			c.Outputs[paddedIndex].Note.Amount = mutateFieldByOne(c.Outputs[paddedIndex].Note.Amount)
		}},
		{name: "blinding", mutate: func(c *circuits.NoteReshapeCircuit) {
			c.Outputs[paddedIndex].Note.Blinding = mutateFieldByOne(c.Outputs[paddedIndex].Note.Blinding)
		}},
		{name: "asset", mutate: func(c *circuits.NoteReshapeCircuit) {
			c.Shared.AssetID = mutateFieldByOne(c.Shared.AssetID)
		}},
		{name: "diversified_generator", mutate: func(c *circuits.NoteReshapeCircuit) {
			c.Shared.DivGen.X = mutateFieldByOne(c.Shared.DivGen.X)
		}},
	}

	for _, mutation := range mutations {
		t.Run(mutation.name, func(t *testing.T) {
			assignment := loadNoteReshapeRegressionAssignment(t, "note_reshape1x8")
			mutation.mutate(assignment)
			if err := noteReshapeAssignmentSolved(t, ccs, assignment); err == nil {
				t.Fatalf("mutating padded output %s must invalidate the witness", mutation.name)
			}
		})
	}
}

func TestNoteReshapePaddedSpendRegressions(t *testing.T) {
	for _, tc := range []struct {
		label string
		nIn   int
	}{
		{label: "note_reshape8x1", nIn: 8},
	} {
		t.Run(tc.label, func(t *testing.T) {
			ccs := compileNoteReshapeRegressionCircuit(t, tc.label, tc.nIn, 1)
			valid := loadNoteReshapeRegressionAssignment(t, tc.label)
			if err := noteReshapeAssignmentSolved(t, ccs, valid); err != nil {
				t.Fatalf("canonical synthetic-input fixture must solve: %v", err)
			}

			dummyIndex := -1
			for index, spend := range valid.SyntheticSpends {
				if variableIsOne(spend.IsDummy) {
					dummyIndex = index
					break
				}
			}
			if dummyIndex < 0 {
				t.Fatal("fixture must contain a dummy spend")
			}

			mutations := []struct {
				name   string
				mutate func(*circuits.NoteReshapeCircuit)
			}{
				{name: "real_invalid_membership", mutate: func(c *circuits.NoteReshapeCircuit) {
					c.SyntheticSpends[0].StateProof.Path[0][0] = mutateFieldByOne(c.SyntheticSpends[0].StateProof.Path[0][0])
				}},
				{name: "dummy_nonzero_amount", mutate: func(c *circuits.NoteReshapeCircuit) {
					c.SyntheticSpends[dummyIndex].Note.Amount = mutateFieldByOne(c.SyntheticSpends[dummyIndex].Note.Amount)
				}},
				{name: "dummy_nullifier_seed", mutate: func(c *circuits.NoteReshapeCircuit) {
					c.SyntheticSpends[dummyIndex].DummyNullifierSeed = mutateFieldByOne(c.SyntheticSpends[dummyIndex].DummyNullifierSeed)
				}},
				{name: "dummy_malformed_rk_point", mutate: func(c *circuits.NoteReshapeCircuit) {
					c.SyntheticSpends[dummyIndex].RK.X = mutateFieldByOne(c.SyntheticSpends[dummyIndex].RK.X)
				}},
				{name: "non_boolean_selector", mutate: func(c *circuits.NoteReshapeCircuit) {
					c.SyntheticSpends[dummyIndex].IsDummy = 2
				}},
				{name: "non_suffix_selector", mutate: func(c *circuits.NoteReshapeCircuit) {
					c.SyntheticSpends[dummyIndex-1].IsDummy = 1
					c.SyntheticSpends[dummyIndex].IsDummy = 0
				}},
				{name: "out_of_policy_active_count", mutate: func(c *circuits.NoteReshapeCircuit) {
					c.SyntheticSpends[dummyIndex-1].IsDummy = 1
				}},
			}

			for _, mutation := range mutations {
				t.Run(mutation.name, func(t *testing.T) {
					assignment := loadNoteReshapeRegressionAssignment(t, tc.label)
					mutation.mutate(assignment)
					if err := noteReshapeAssignmentSolved(t, ccs, assignment); err == nil {
						t.Fatalf("mutation %s must invalidate the %s witness", mutation.name, tc.label)
					}
				})
			}
		})
	}
}

func noteReshapeDummyNullifierForSlot(
	t *testing.T,
	spend abi.NoteReshapeSpendWitnessBinary,
	slot int,
) *big.Int {
	t.Helper()
	domainBytes := blake2b.Sum512(
		[]byte("shieldd.note_reshape.synthetic_dummy.nullifier"),
	)
	nullifier, err := primitives.Poseidon377Hash3Native(
		primitives.LittleEndianBytesToBigInt(domainBytes[:]),
		[3]*big.Int{
			primitives.LittleEndianBytesToBigInt(spend.DummyNullifierSeed[:]),
			primitives.LittleEndianBytesToBigInt(spend.SpendAuthRandomizer[:]),
			big.NewInt(int64(slot)),
		},
	)
	if err != nil {
		t.Fatalf("compute NoteReshape synthetic nullifier for slot %d: %v", slot, err)
	}
	return nullifier
}

func TestNoteReshapeSyntheticDummyNullifiersBindFixedSlot(t *testing.T) {
	for _, label := range []string{"note_reshape8x1"} {
		t.Run(label, func(t *testing.T) {
			family, ok := generated.NoteReshapeFamilyByLabel(label)
			if !ok {
				t.Fatalf("missing generated family %s", label)
			}
			baseWitness, _ :=
				loadNoteReshapeRegressionWitnessAndAssignment(t, label)
			var dummyIndexes []int
			for index, spend := range baseWitness.Spends {
				if spend.IsDummy {
					dummyIndexes = append(dummyIndexes, index)
				}
			}
			if len(dummyIndexes) == 0 {
				t.Fatalf(
					"%s fixture must contain at least one dummy spend",
					label,
				)
			}
			ccs := compileNoteReshapeRegressionCircuit(
				t,
				label,
				family.NIn,
				family.NOut,
			)

			for _, dummyIndex := range dummyIndexes {
				dummyIndex := dummyIndex
				t.Run(fmt.Sprintf("slot_%d", dummyIndex), func(t *testing.T) {
					if dummyIndex <= 0 {
						t.Fatalf(
							"%s dummy slot %d has no preceding wrong slot",
							label,
							dummyIndex,
						)
					}
					witness, assignment :=
						loadNoteReshapeRegressionWitnessAndAssignment(t, label)
					canonicalNullifier := noteReshapeDummyNullifierForSlot(
						t,
						witness.Spends[dummyIndex],
						dummyIndex,
					)
					storedNullifier := primitives.LittleEndianBytesToBigInt(
						witness.Spends[dummyIndex].Nullifier[:],
					)
					if canonicalNullifier.Cmp(storedNullifier) != 0 {
						t.Fatalf(
							"%s slot %d fixture nullifier is not its canonical fixed-slot nullifier",
							label,
							dummyIndex,
						)
					}
					wrongSlot := dummyIndex - 1
					wrongNullifier := noteReshapeDummyNullifierForSlot(
						t,
						witness.Spends[dummyIndex],
						wrongSlot,
					)
					if wrongNullifier.Cmp(canonicalNullifier) == 0 {
						t.Fatalf(
							"%s slot %d wrong-slot nullifier unexpectedly equals canonical",
							label,
							dummyIndex,
						)
					}
					witness.Spends[dummyIndex].Nullifier = le32FromBigInt(
						t,
						wrongNullifier,
					)
					assignment.SyntheticSpends[dummyIndex].Nullifier =
						wrongNullifier.String()
					setNoteReshapeStatementHash(
						t,
						label,
						witness,
						assignment,
					)

					if err := noteReshapeAssignmentSolved(
						t,
						ccs,
						assignment,
					); err == nil {
						t.Fatalf(
							"%s accepted a dummy nullifier from slot %d in fixed slot %d",
							label,
							wrongSlot,
							dummyIndex,
						)
					}
				})
			}
		})
	}
}

func TestNoteReshapeFamiliesRejectWrongFamilyDomain(t *testing.T) {
	for _, family := range generated.NoteReshapeFamilies {
		t.Run(family.Label, func(t *testing.T) {
			assignment := loadNoteReshapeRegressionAssignment(t, family.Label)
			witness, _, err := abi.DecodeNoteReshapeWitness(
				testfixtures.LoadNoteReshapeWitness(family.Label),
			)
			if err != nil {
				t.Fatalf("decode %s fixture: %v", family.Label, err)
			}
			fields, err := abi.ReconstructedNoteReshapeStatementFieldsFromWitness(witness)
			if err != nil {
				t.Fatalf("reconstruct %s statement fields: %v", family.Label, err)
			}
			wrongHash, err := noteReshapeDomainHashNative(
				fieldElementStrings(fields),
				"note_reshape_wrong_family",
			)
			if err != nil {
				t.Fatalf("compute wrong-family hash: %v", err)
			}
			assignment.ClaimedStatementHash = wrongHash.String()
			ccs := compileNoteReshapeRegressionCircuit(t, family.Label, family.NIn, family.NOut)
			if err := noteReshapeAssignmentSolved(t, ccs, assignment); err == nil {
				t.Fatalf("wrong family statement domain must invalidate %s", family.Label)
			}
		})
	}
}

func TestNoteReshapeFamiliesRejectWrongShape(t *testing.T) {
	for _, family := range generated.NoteReshapeFamilies {
		t.Run(family.Label, func(t *testing.T) {
			for _, shape := range []struct {
				name string
				nIn  int
				nOut int
			}{
				{name: "input_minus_one", nIn: family.NIn - 1, nOut: family.NOut},
				{name: "input_plus_one", nIn: family.NIn + 1, nOut: family.NOut},
				{name: "output_minus_one", nIn: family.NIn, nOut: family.NOut - 1},
				{name: "output_plus_one", nIn: family.NIn, nOut: family.NOut + 1},
			} {
				t.Run(shape.name, func(t *testing.T) {
					wrongShape := circuits.NewNoteReshapeCircuit(
						family.Label,
						shape.nIn,
						shape.nOut,
					)
					if _, err := frontend.Compile(
						ecc.BLS12_377.ScalarField(),
						r1cs.NewBuilder,
						wrongShape,
					); err == nil {
						t.Fatalf(
							"wrong %s circuit shape %dx%d must be rejected",
							family.Label,
							shape.nIn,
							shape.nOut,
						)
					}
				})
			}
		})
	}
}

func TestNoteReshapeFamiliesRejectWrongStatementPreimage(t *testing.T) {
	for _, family := range generated.NoteReshapeFamilies {
		t.Run(family.Label, func(t *testing.T) {
			witness, assignment :=
				loadNoteReshapeRegressionWitnessAndAssignment(t, family.Label)
			witness.Spends[0].Nullifier = addFieldElementBytes(
				t,
				witness.Spends[0].Nullifier,
				big.NewInt(1),
			)
			if family.InputPadding == generated.InputPaddingSyntheticPrivate {
				assignment.SyntheticSpends[0].Nullifier =
					primitives.LittleEndianBytesToBigInt(
						witness.Spends[0].Nullifier[:],
					).String()
			} else {
				assignment.Spends[0].Nullifier =
					primitives.LittleEndianBytesToBigInt(
						witness.Spends[0].Nullifier[:],
					).String()
			}
			setNoteReshapeStatementHash(
				t,
				family.Label,
				witness,
				assignment,
			)
			ccs := compileNoteReshapeRegressionCircuit(t, family.Label, family.NIn, family.NOut)
			if err := noteReshapeAssignmentSolved(t, ccs, assignment); err == nil {
				t.Fatalf("mutating a statement-preimage nullifier must invalidate %s", family.Label)
			}
		})
	}
}
