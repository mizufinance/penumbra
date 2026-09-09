package circuits_test

import (
	"math/big"
	"strings"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/test"
	"github.com/mizufinance/shieldd/tools/gnark/internal/abi"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"github.com/mizufinance/shieldd/tools/gnark/internal/testfixtures"
)

func TestShieldedWithdrawalRejectsFeeFundingContext(t *testing.T) {
	fixture := testfixtures.LoadShieldedWithdrawalWitness("shielded_withdrawal_unregulated")
	witness, family, err := abi.DecodeShieldedWithdrawalWitness(fixture)
	if err != nil {
		t.Fatal(err)
	}
	assignment, _, err := abi.NewShieldedWithdrawalCircuitAssignmentFromWitness(fixture)
	if err != nil {
		t.Fatal(err)
	}
	witness.VolumeAccumulator.ProofContext = [32]byte{2}
	witness.VolumeAccumulator.Nullifier = [32]byte{}
	witness.VolumeAccumulator.Commitment = [32]byte{}
	witness.VolumeAccumulator.DayStart = [32]byte{}
	assignment.VolumeAccumulator.ProofContext = 2
	assignment.VolumeAccumulator.Nullifier = 0
	assignment.VolumeAccumulator.Commitment = 0
	assignment.VolumeAccumulator.DayStart = 0
	setWithdrawalStatementHash(t, witness, assignment, family.NIn)
	if err := test.IsSolved(circuits.NewShieldedWithdrawalCircuit(family.NIn), assignment, ecc.BLS12_377.ScalarField()); err == nil {
		t.Fatal("withdrawal accepted disabled fee-funding accumulator context")
	}
}

func TestRegulatedWithdrawalRejectsFrozenSender(t *testing.T) {
	_, assignment, nIn := loadWithdrawalFixture(t)
	assignment.Sender.Status = 2

	if err := test.IsSolved(
		circuits.NewShieldedWithdrawalCircuit(nIn),
		assignment,
		ecc.BLS12_377.ScalarField(),
	); err == nil {
		t.Fatal("regulated withdrawal accepted a frozen sender")
	}
}

func TestShieldedWithdrawalBindsEveryEffectHashLimb(t *testing.T) {
	for limb, name := range []string{"0", "1", "2", "3"} {
		t.Run(name, func(t *testing.T) {
			fixture := testfixtures.LoadShieldedWithdrawalWitness(
				"shielded_withdrawal",
			)
			assignment, family, err :=
				abi.NewShieldedWithdrawalCircuitAssignmentFromWitness(fixture)
			if err != nil {
				t.Fatalf("decode withdrawal fixture: %v", err)
			}
			assignment.WithdrawalEffectHashLimbs[limb] =
				mutateFieldByOne(assignment.WithdrawalEffectHashLimbs[limb])

			if err := test.IsSolved(
				circuits.NewShieldedWithdrawalCircuit(family.NIn),
				assignment,
				ecc.BLS12_377.ScalarField(),
			); err == nil {
				t.Fatalf("withdrawal accepted mutated effect-hash limb %d", limb)
			}
		})
	}
}

func TestShieldedWithdrawalRejectsNonBooleanRegulatedSelector(t *testing.T) {
	_, assignment, nIn := loadWithdrawalFixture(t)
	assignment.IsRegulated = 2
	if err := test.IsSolved(
		circuits.NewShieldedWithdrawalCircuit(nIn),
		assignment,
		ecc.BLS12_377.ScalarField(),
	); err == nil {
		t.Fatal("shielded withdrawal accepted is_regulated=2")
	}
}

func TestShieldedWithdrawalBindsComplianceCiphertextAndPolicy(t *testing.T) {
	tests := []struct {
		name   string
		public bool
		mutate func(
			*testing.T,
			*abi.ShieldedWithdrawalWitnessBinary,
			*circuits.ShieldedWithdrawalCircuit,
		)
	}{
		{
			name:   "epk",
			public: true,
			mutate: func(_ *testing.T, w *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				w.WithdrawalEPKAffine = w.SenderDiversifiedGenerator
				c.Compliance.EPK = c.Sender.DivGen
			},
		},
		{
			name:   "c2",
			public: true,
			mutate: func(t *testing.T, w *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				w.WithdrawalC2 = addFieldElementBytes(t, w.WithdrawalC2, big.NewInt(1))
				c.Compliance.C2 = primitives.LittleEndianBytesToBigInt(w.WithdrawalC2[:]).String()
			},
		},
		{
			name:   "key confirmation",
			public: true,
			mutate: func(t *testing.T, w *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				w.WithdrawalKeyConfirmation = addFieldElementBytes(t, w.WithdrawalKeyConfirmation, big.NewInt(1))
				c.Compliance.KeyConfirmation = primitives.LittleEndianBytesToBigInt(w.WithdrawalKeyConfirmation[:]).String()
			},
		},
		{
			name:   "encrypted sender address",
			public: true,
			mutate: func(t *testing.T, w *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				w.WithdrawalEncryptedSenderAddress[1] = addFieldElementBytes(t, w.WithdrawalEncryptedSenderAddress[1], big.NewInt(1))
				c.Compliance.EncryptedSenderAddress[1] = primitives.LittleEndianBytesToBigInt(w.WithdrawalEncryptedSenderAddress[1][:]).String()
			},
		},
		{
			name: "seed",
			mutate: func(_ *testing.T, _ *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				c.Compliance.Seed = mutateFieldByOne(c.Compliance.Seed)
			},
		},
		{
			name: "randomizer",
			mutate: func(_ *testing.T, _ *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				c.Compliance.Randomizer = mutateFieldByOne(c.Compliance.Randomizer)
			},
		},
		{
			name: "daily_volume_limit",
			mutate: func(_ *testing.T, _ *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				c.Asset.Leaf.DailyVolumeLimit = 1
			},
		},
		{
			name: "issuer key",
			mutate: func(_ *testing.T, _ *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				c.Asset.Leaf.DKPub = c.Sender.Capk
			},
		},
		{
			name: "ring key",
			mutate: func(_ *testing.T, _ *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				c.Asset.Leaf.RingPK = c.Sender.Capk
			},
		},
		{
			name: "regulated nullifier DH key",
			mutate: func(_ *testing.T, _ *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				c.Sender.RnkDhPk.X = mutateFieldByOne(c.Sender.RnkDhPk.X)
			},
		},
		{
			name: "regulated nullifier commitment",
			mutate: func(_ *testing.T, _ *abi.ShieldedWithdrawalWitnessBinary, c *circuits.ShieldedWithdrawalCircuit) {
				c.Sender.RnkCommitment = mutateFieldByOne(c.Sender.RnkCommitment)
			},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			witness, assignment, nIn := loadWithdrawalFixture(t)
			tc.mutate(t, witness, assignment)
			if tc.public {
				setWithdrawalStatementHash(t, witness, assignment, nIn)
			}
			if err := test.IsSolved(
				circuits.NewShieldedWithdrawalCircuit(nIn),
				assignment,
				ecc.BLS12_377.ScalarField(),
			); err == nil {
				t.Fatalf("withdrawal accepted mutated %s", tc.name)
			}
		})
	}
}

func TestShieldedWithdrawalRejectsEveryOwnedPublicFieldMutation(
	t *testing.T,
) {
	tests := []struct {
		name   string
		mutate func(
			*testing.T,
			*abi.ShieldedWithdrawalWitnessBinary,
			*circuits.ShieldedWithdrawalCircuit,
		)
	}{
		{
			name: "required spend nullifier",
			mutate: func(
				t *testing.T,
				w *abi.ShieldedWithdrawalWitnessBinary,
				c *circuits.ShieldedWithdrawalCircuit,
			) {
				w.RequiredSpend.Nullifier = addFieldElementBytes(
					t,
					w.RequiredSpend.Nullifier,
					big.NewInt(1),
				)
				c.RequiredSpend.Nullifier = primitives.LittleEndianBytesToBigInt(
					w.RequiredSpend.Nullifier[:],
				).String()
			},
		},
		{
			name: "optional real spend nullifier",
			mutate: func(
				t *testing.T,
				w *abi.ShieldedWithdrawalWitnessBinary,
				c *circuits.ShieldedWithdrawalCircuit,
			) {
				if w.OptionalSpend.IsDummy {
					t.Fatal("withdrawal mutation fixture must expose a real optional spend")
				}
				w.OptionalSpend.Nullifier = addFieldElementBytes(
					t,
					w.OptionalSpend.Nullifier,
					big.NewInt(1),
				)
				c.OptionalSpend.Nullifier = primitives.LittleEndianBytesToBigInt(
					w.OptionalSpend.Nullifier[:],
				).String()
			},
		},
		{
			name: "required randomized verification key",
			mutate: func(
				t *testing.T,
				w *abi.ShieldedWithdrawalWitnessBinary,
				c *circuits.ShieldedWithdrawalCircuit,
			) {
				if !pointsHaveDistinctCompression(
					t,
					w.RequiredSpend.RKAffine,
					w.OptionalSpend.RKAffine,
				) {
					t.Fatal("withdrawal mutation fixture must expose distinct spend RKs")
				}
				w.RequiredSpend.RKAffine = w.OptionalSpend.RKAffine
				c.RequiredSpend.RK = c.OptionalSpend.RK
			},
		},
		{
			name: "optional real randomized verification key",
			mutate: func(
				t *testing.T,
				w *abi.ShieldedWithdrawalWitnessBinary,
				c *circuits.ShieldedWithdrawalCircuit,
			) {
				if w.OptionalSpend.IsDummy {
					t.Fatal("withdrawal mutation fixture must expose a real optional spend")
				}
				if !pointsHaveDistinctCompression(
					t,
					w.RequiredSpend.RKAffine,
					w.OptionalSpend.RKAffine,
				) {
					t.Fatal("withdrawal mutation fixture must expose distinct spend RKs")
				}
				w.OptionalSpend.RKAffine = w.RequiredSpend.RKAffine
				c.OptionalSpend.RK = c.RequiredSpend.RK
			},
		},
		{
			name: "outbound asset id",
			mutate: func(
				t *testing.T,
				w *abi.ShieldedWithdrawalWitnessBinary,
				c *circuits.ShieldedWithdrawalCircuit,
			) {
				w.OutboundAssetID = addFieldElementBytes(
					t,
					w.OutboundAssetID,
					big.NewInt(1),
				)
				c.OutboundAssetID = primitives.LittleEndianBytesToBigInt(
					w.OutboundAssetID[:],
				).String()
			},
		},
		{
			name: "outbound amount exact conservation",
			mutate: func(
				t *testing.T,
				w *abi.ShieldedWithdrawalWitnessBinary,
				c *circuits.ShieldedWithdrawalCircuit,
			) {
				amount := primitives.LittleEndianBytesToBigInt(
					w.OutboundAmount[:],
				)
				amount.Add(amount, big.NewInt(1))
				if amount.BitLen() > 128 {
					t.Fatalf(
						"mutated withdrawal outbound amount exceeds the u128 range: %s",
						amount,
					)
				}
				w.OutboundAmount = le32FromBigInt(t, amount)
				c.OutboundAmount = amount.String()
			},
		},
		{
			name: "change note commitment",
			mutate: func(
				t *testing.T,
				w *abi.ShieldedWithdrawalWitnessBinary,
				c *circuits.ShieldedWithdrawalCircuit,
			) {
				w.ChangeOutput.NoteCommitment = addFieldElementBytes(
					t,
					w.ChangeOutput.NoteCommitment,
					big.NewInt(1),
				)
				c.ChangeOutput.NoteCommitment =
					primitives.LittleEndianBytesToBigInt(
						w.ChangeOutput.NoteCommitment[:],
					).String()
			},
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			witness, assignment, nIn := loadWithdrawalFixture(t)
			tc.mutate(t, witness, assignment)
			setWithdrawalStatementHash(t, witness, assignment, nIn)
			if err := test.IsSolved(
				circuits.NewShieldedWithdrawalCircuit(nIn),
				assignment,
				ecc.BLS12_377.ScalarField(),
			); err == nil {
				t.Fatalf(
					"withdrawal accepted independently rebound %s",
					tc.name,
				)
			}
		})
	}
}

func TestShieldedWithdrawalWiringBindsPolicyLeafAndSentinel(t *testing.T) {
	transcript, err := circuits.ExportShieldedWithdrawalWiringTranscript(
		"shielded_withdrawal",
		2,
	)
	if err != nil {
		t.Fatalf("export withdrawal wiring transcript: %v", err)
	}

	for _, binding := range []string{
		"assert.ne lhs=shared.asset_id rhs=0",
		"gadget.asset_registry_params_hash dk_pub_fq=asset.leaf.dk_pub_fq daily_volume_limit=asset.leaf.daily_volume_limit route_policy_hash=asset.leaf.route_policy_hash out=asset.leaf.params_hash",
		"gadget.asset_registry_ring_hash ring_pk_fq=asset.leaf.ring_pk_fq ring_id_hash=asset.leaf.ring_id_hash policy_id_hash=asset.leaf.policy_id_hash permission_hash=asset.leaf.permission_hash resource_hash=asset.leaf.resource_hash out=asset.leaf.ring_hash",
		"gadget.asset_registry_leaf_hash value=asset.leaf.value next_index=asset.leaf.next_index next_value=asset.leaf.next_value params_hash=asset.leaf.params_hash ring_hash=asset.leaf.ring_hash out=asset.leaf.commitment",
		"gadget.compliance_leaf div_gen_fq=sender.div_gen_fq transmission_fq=sender.transmission_fq asset_id=shared.asset_id capk=sender.capk rnk_dh_pk=sender.rnk_dh_pk rnk_commitment=sender.rnk_commitment status=sender.status out=sender.leaf_commitment",
	} {
		if count := strings.Count(transcript, binding); count != 1 {
			t.Fatalf("withdrawal wiring must contain %q exactly once, got %d", binding, count)
		}
	}
	manifest, manifestErr := circuits.ExportShieldedWithdrawalConstraintManifest("")
	if manifestErr != nil {
		t.Fatalf("export withdrawal constraint manifest: %v", manifestErr)
	}
	for _, rk := range []string{"spend0.rk.claimed", "spend1.rk.claimed"} {
		count := 0
		for _, binding := range manifest.SemanticBindings {
			if binding.Name == rk {
				count++
			}
		}
		if count != 1 {
			t.Fatalf("withdrawal wiring must bind %s exactly once, got %d", rk, count)
		}
	}
}

func TestShieldedWithdrawalOptionalDummyBindsNullifierSeed(t *testing.T) {
	witness, assignment, nIn := loadWithdrawalFixture(t)
	makeWithdrawalOptionalSpendDummy(t, witness, assignment)
	balanceWithdrawalAfterOptionalDummy(t, witness, assignment)
	setWithdrawalStatementHash(t, witness, assignment, nIn)
	assignment.OptionalSpend.DummyNullifierSeed =
		mutateFieldByOne(assignment.OptionalSpend.DummyNullifierSeed)

	if err := test.IsSolved(
		circuits.NewShieldedWithdrawalCircuit(nIn),
		assignment,
		ecc.BLS12_377.ScalarField(),
	); err == nil {
		t.Fatal("withdrawal optional dummy accepted a mutated nullifier seed")
	}
}

func TestShieldedWithdrawalSyntheticDummyNullifierBindsFixedSlot(
	t *testing.T,
) {
	fixture := testfixtures.LoadShieldedWithdrawalWitness(
		"shielded_withdrawal_unregulated",
	)
	witness, family, err := abi.DecodeShieldedWithdrawalWitness(fixture)
	if err != nil {
		t.Fatalf("decode dummy withdrawal fixture: %v", err)
	}
	assignment, _, err :=
		abi.NewShieldedWithdrawalCircuitAssignmentFromWitness(fixture)
	if err != nil {
		t.Fatalf("build dummy withdrawal assignment: %v", err)
	}
	if !witness.OptionalSpend.IsDummy {
		t.Fatal("unregulated withdrawal fixture must have a dummy optional spend")
	}

	const (
		wrongSlot     = 0
		canonicalSlot = 1
	)
	wrongNullifier := withdrawalDummyNullifierForSlot(
		t,
		witness.OptionalSpend,
		wrongSlot,
	)
	canonicalNullifier := withdrawalDummyNullifierForSlot(
		t,
		witness.OptionalSpend,
		canonicalSlot,
	)
	if wrongNullifier.Cmp(canonicalNullifier) == 0 {
		t.Fatal("wrong-slot withdrawal nullifier unexpectedly equals canonical")
	}
	witness.OptionalSpend.Nullifier = le32FromBigInt(t, wrongNullifier)
	assignment.OptionalSpend.Nullifier = wrongNullifier.String()
	setWithdrawalStatementHash(t, witness, assignment, family.NIn)

	if err := test.IsSolved(
		circuits.NewShieldedWithdrawalCircuit(family.NIn),
		assignment,
		ecc.BLS12_377.ScalarField(),
	); err == nil {
		t.Fatal("withdrawal accepted a slot-0 dummy nullifier in fixed slot 1")
	}
}
