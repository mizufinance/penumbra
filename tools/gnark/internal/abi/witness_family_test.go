package abi

import (
	"encoding/binary"
	"testing"

	decaf377 "github.com/mizufinance/decaf377-go"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/testfixtures"
)

type witnessFamily struct {
	name    string
	payload func(t *testing.T) []byte
	decode  func([]byte) error
}

func testWitnessFamilies() []witnessFamily {
	return []witnessFamily{
		{
			name:    "note_seizure",
			payload: func(t *testing.T) []byte { return testfixtures.LoadNoteSeizureWitness() },
			decode: func(payload []byte) error {
				_, err := DecodeNoteSeizureWitness(payload)
				return err
			},
		},
		{
			name:    "transfer",
			payload: func(t *testing.T) []byte { return testfixtures.LoadTransferWitness("transfer") },
			decode: func(payload []byte) error {
				_, _, err := DecodeTransferWitness(payload)
				return err
			},
		},
		{
			name: "transfer_flagged",
			payload: func(t *testing.T) []byte {
				return testfixtures.LoadTransferWitness("transfer_flagged")
			},
			decode: func(payload []byte) error {
				_, _, err := DecodeTransferWitness(payload)
				return err
			},
		},
		{
			name: "transfer_unregulated",
			payload: func(t *testing.T) []byte {
				return testfixtures.LoadTransferWitness("transfer_unregulated")
			},
			decode: func(payload []byte) error {
				_, _, err := DecodeTransferWitness(payload)
				return err
			},
		},
		{
			name: "shielded_ics20_withdrawal",
			payload: func(t *testing.T) []byte {
				return testfixtures.LoadShieldedIcs20WithdrawalWitness("shielded_ics20_withdrawal")
			},
			decode: func(payload []byte) error {
				_, _, err := DecodeShieldedIcs20WithdrawalWitness(payload)
				return err
			},
		},
		{
			name: "shielded_ics20_withdrawal_unregulated",
			payload: func(t *testing.T) []byte {
				return testfixtures.LoadShieldedIcs20WithdrawalWitness(
					"shielded_ics20_withdrawal_unregulated",
				)
			},
			decode: func(payload []byte) error {
				_, _, err := DecodeShieldedIcs20WithdrawalWitness(payload)
				return err
			},
		},
		{
			name: "shielded_ics20_withdrawal_accumulator_origin",
			payload: func(t *testing.T) []byte {
				return testfixtures.LoadShieldedIcs20WithdrawalWitness(
					"shielded_ics20_withdrawal_accumulator_origin",
				)
			},
			decode: func(payload []byte) error {
				_, _, err := DecodeShieldedIcs20WithdrawalWitness(payload)
				return err
			},
		},
		{
			name: "shielded_ics20_withdrawal_accumulator_continuation",
			payload: func(t *testing.T) []byte {
				return testfixtures.LoadShieldedIcs20WithdrawalWitness(
					"shielded_ics20_withdrawal_accumulator_continuation",
				)
			},
			decode: func(payload []byte) error {
				_, _, err := DecodeShieldedIcs20WithdrawalWitness(payload)
				return err
			},
		},
		{
			name: "note_reshape8x1",
			payload: func(t *testing.T) []byte {
				return testfixtures.LoadNoteReshapeWitness("note_reshape8x1")
			},
			decode: func(payload []byte) error {
				_, _, err := DecodeNoteReshapeWitness(payload)
				return err
			},
		},
		{
			name: "note_reshape1x8",
			payload: func(t *testing.T) []byte {
				return testfixtures.LoadNoteReshapeWitness("note_reshape1x8")
			},
			decode: func(payload []byte) error {
				_, _, err := DecodeNoteReshapeWitness(payload)
				return err
			},
		},
	}
}

func TestWitnessFamiliesDecode(t *testing.T) {
	for _, family := range testWitnessFamilies() {
		t.Run(family.name, func(t *testing.T) {
			if err := family.decode(family.payload(t)); err != nil {
				t.Fatalf("decode %s witness: %v", family.name, err)
			}
		})
	}
}

func TestShieldedIcs20WithdrawalFixtureBranchMatrix(t *testing.T) {
	for _, tc := range []struct {
		label       string
		isRegulated bool
		isDummy     bool
	}{
		{
			label:       "shielded_ics20_withdrawal",
			isRegulated: true,
			isDummy:     false,
		},
		{
			label:       "shielded_ics20_withdrawal_unregulated",
			isRegulated: false,
			isDummy:     true,
		},
		{
			label:       "shielded_ics20_withdrawal_accumulator_origin",
			isRegulated: true,
			isDummy:     false,
		},
		{
			label:       "shielded_ics20_withdrawal_accumulator_continuation",
			isRegulated: true,
			isDummy:     false,
		},
	} {
		t.Run(tc.label, func(t *testing.T) {
			witness, _, err := DecodeShieldedIcs20WithdrawalWitness(
				testfixtures.LoadShieldedIcs20WithdrawalWitness(tc.label),
			)
			if err != nil {
				t.Fatalf("decode branch fixture: %v", err)
			}
			if witness.IsRegulated != tc.isRegulated {
				t.Fatalf("is_regulated=%v, want %v", witness.IsRegulated, tc.isRegulated)
			}
			if witness.OptionalSpend.IsDummy != tc.isDummy {
				t.Fatalf("optional is_dummy=%v, want %v", witness.OptionalSpend.IsDummy, tc.isDummy)
			}
		})
	}
}

func TestWitnessFamiliesRejectBadHeader(t *testing.T) {
	for _, family := range testWitnessFamilies() {
		t.Run(family.name, func(t *testing.T) {
			payload := append([]byte(nil), family.payload(t)...)
			payload[0] ^= 0xff
			if err := family.decode(payload); err == nil {
				t.Fatalf("expected %s witness to reject mutated header", family.name)
			}
		})
	}
}

func TestWitnessFamiliesRejectTruncatedPayload(t *testing.T) {
	for _, family := range testWitnessFamilies() {
		t.Run(family.name, func(t *testing.T) {
			payload := append([]byte(nil), family.payload(t)...)
			payload = payload[:len(payload)-1]
			if err := family.decode(payload); err == nil {
				t.Fatalf("expected %s witness to reject truncated payload", family.name)
			}
		})
	}
}

func TestTransferAssignmentRejectsClaimedHashMismatch(t *testing.T) {
	payload := testfixtures.LoadTransferWitness("transfer")
	const claimedStatementHashOffset = 8 + 4*32
	payload[claimedStatementHashOffset] ^= 1
	if _, _, err := NewTransferCircuitAssignmentFromWitness(payload); err == nil {
		t.Fatal(" assignment must reject a claimed hash that disagrees with reconstructed fields")
	}
}

func TestTransferAssignmentRejectsSerializedSemanticMutation(t *testing.T) {
	payload := testfixtures.LoadTransferWitness("transfer")
	const anchorOffset = 8
	payload[anchorOffset] ^= 1
	if _, _, err := NewTransferCircuitAssignmentFromWitness(payload); err == nil {
		t.Fatal(" assignment must reject a serialized anchor mutation against the claimed hash")
	}
}

func TestShieldedIcs20WithdrawalAssignmentRejectsClaimedHashMismatch(t *testing.T) {
	payload := testfixtures.LoadShieldedIcs20WithdrawalWitness("shielded_ics20_withdrawal")
	const claimedStatementHashOffset = 16 + 6*32 + 4*32
	payload[claimedStatementHashOffset] ^= 1
	if _, _, err := NewShieldedIcs20WithdrawalCircuitAssignmentFromWitness(payload); err == nil {
		t.Fatal("assignment must reject a claimed hash that disagrees with reconstructed fields")
	}
}

func TestShieldedIcs20WithdrawalRejectsOversizedEffectHashLimb(t *testing.T) {
	payload := testfixtures.LoadShieldedIcs20WithdrawalWitness("shielded_ics20_withdrawal")
	const effectHashLimbsOffset = 16 + 6*32
	payload[effectHashLimbsOffset+16] = 1
	if _, _, err := DecodeShieldedIcs20WithdrawalWitness(payload); err == nil {
		t.Fatal("decoder must reject effect-hash limbs wider than 128 bits")
	}
}

func TestShieldedIcs20WithdrawalRejectsNonCanonicalBalanceBlinding(t *testing.T) {
	payload := testfixtures.LoadShieldedIcs20WithdrawalWitness("shielded_ics20_withdrawal")
	const volumeAccumulatorBytes = 4*32 + 2 + 4*32 + 8 + (4 + 24*3*32) + 2*32
	const actionBalanceBlindingOffset = 16 + 22*32 + volumeAccumulatorBytes
	modulus, err := bigIntToLE32(decaf377.ScalarOrder())
	if err != nil {
		t.Fatalf("encode Decaf377 scalar modulus: %v", err)
	}
	copy(payload[actionBalanceBlindingOffset:], modulus[:])
	if _, _, err := DecodeShieldedIcs20WithdrawalWitness(payload); err == nil {
		t.Fatal("decoder must reject a non-canonical action balance blinding")
	}
}

func TestShieldedIcs20WithdrawalRejectsNonCanonicalBooleanFlags(t *testing.T) {
	const (
		headerBytes            = 16
		topFieldsThroughNK     = 24*32 + 4*32 + 2 + 4*32 + 8 + (4 + 24*3*32) + 2*32
		merklePathBytes        = 4 + 16*(4+3*32)
		committedLeafBytes     = 32 + 8 + 32 + 16 + 5*32
		isRegulatedOffset      = headerBytes + topFieldsThroughNK + merklePathBytes + 8 + committedLeafBytes
		slimRequiredSpendBytes = 4*32 + 8 + 4 + 24*3*32 + 32 + 64 + 1
		routingPrivateBytes    = 2 + 8 + 32
		optionalIsDummyOffset  = isRegulatedOffset + 1 + routingPrivateBytes + merklePathBytes + 8 + 8*32 + 2*slimRequiredSpendBytes
	)
	for name, offset := range map[string]int{
		"is_regulated":      isRegulatedOffset,
		"optional.is_dummy": optionalIsDummyOffset,
	} {
		t.Run(name, func(t *testing.T) {
			payload := testfixtures.LoadShieldedIcs20WithdrawalWitness(
				"shielded_ics20_withdrawal",
			)
			payload[offset] = 2
			if _, _, err := DecodeShieldedIcs20WithdrawalWitness(payload); err == nil {
				t.Fatal("withdrawal decoder must reject non-canonical boolean flags")
			}
		})
	}
}

func TestNoteReshapeWitnessPaddingABI(t *testing.T) {
	fixedPayload := testfixtures.LoadNoteReshapeWitness("note_reshape1x8")
	fixed, fixedFamily, err := DecodeNoteReshapeWitness(fixedPayload)
	if err != nil {
		t.Fatalf("decode fixed-family witness: %v", err)
	}
	if fixedFamily.InputPadding != generated.InputPaddingFixed {
		t.Fatalf("1x8 input policy: got %v", fixedFamily.InputPadding)
	}
	for index, spend := range fixed.Spends {
		if spend.IsDummy || spend.DummyNullifierSeed != [32]byte{} {
			t.Fatalf("fixed input %d carries synthetic-padding ABI data", index)
		}
	}

	syntheticPayload := testfixtures.LoadNoteReshapeWitness("note_reshape8x1")
	synthetic, syntheticFamily, err := DecodeNoteReshapeWitness(syntheticPayload)
	if err != nil {
		t.Fatalf("decode synthetic-family witness: %v", err)
	}
	if syntheticFamily.InputPadding != generated.InputPaddingSyntheticPrivate {
		t.Fatalf("8x1 input policy: got %v", syntheticFamily.InputPadding)
	}
	if !synthetic.Spends[len(synthetic.Spends)-1].IsDummy {
		t.Fatal("8x1 fixture must carry a private dummy selector")
	}

	const (
		headerBytes           = 20
		topFieldsThroughNK    = 9 * 32
		merklePathBytes       = 4 + 16*(4+3*32)
		indexedLeafBytes      = 32 + 8 + 32 + 16 + 5*32
		assetLeafPointBytes   = 2 * 64
		routingPrivateBytes   = 1 + 2 + 8 + 32
		sharedContextBytes    = 32 + 64
		senderComplianceBytes = merklePathBytes + 8 + 6*32
		flagOffset            = headerBytes + topFieldsThroughNK + merklePathBytes + 8 + indexedLeafBytes + assetLeafPointBytes + routingPrivateBytes + senderComplianceBytes + sharedContextBytes
	)
	malformed := append([]byte(nil), syntheticPayload...)
	malformed[flagOffset] = 2
	if _, _, err := DecodeNoteReshapeWitness(malformed); err == nil {
		t.Fatal("synthetic-family witness must reject a non-boolean private padding flag")
	}
}

func TestNoteReshapeRejectsSplitAddressRepresentationPayload(t *testing.T) {
	payload := append(
		[]byte(nil),
		testfixtures.LoadNoteReshapeWitness("note_reshape8x1")...,
	)

	payload = append(payload, make([]byte, 32+64+64+32+32)...)
	binary.LittleEndian.PutUint32(payload[4:8], uint32(len(payload)))

	if _, _, err := DecodeNoteReshapeWitness(payload); err == nil {
		t.Fatal("decoder must reject an appended split address representation")
	}
}
