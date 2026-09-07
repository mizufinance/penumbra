package abi

import (
	"bytes"
	"math/big"
	"testing"

	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"github.com/mizufinance/shieldd/tools/gnark/internal/testfixtures"
)

func TestRustGoStatementFieldDifferential(t *testing.T) {
	t.Run("transfer", func(t *testing.T) {
		witness, _, err := DecodeTransferWitness(testfixtures.LoadTransferWitness("transfer"))
		if err != nil {
			t.Fatalf("decode transfer witness: %v", err)
		}
		reconstructed, err := ReconstructedTransferStatementFieldsFromWitness(witness)
		if err != nil {
			t.Fatalf("reconstruct transfer statement fields: %v", err)
		}
		nativeFields := make([]*big.Int, len(reconstructed))
		for i, field := range reconstructed {
			nativeFields[i] = primitives.LittleEndianBytesToBigInt(field[:])
		}
		hash, err := primitives.TransferStatementHashNativeForShape(nativeFields, 2, 2)
		if err != nil {
			t.Fatalf("hash reconstructed transfer statement fields: %v", err)
		}
		claimed := primitives.LittleEndianBytesToBigInt(witness.ClaimedStatementHash[:])
		if hash.Cmp(claimed) != 0 {
			t.Fatalf("transfer reconstructed statement hash mismatch:\ngot=%s\nwant=%s", hash, claimed)
		}
	})

	t.Run("shielded_ics20_withdrawal", func(t *testing.T) {
		witness, _, err := DecodeShieldedIcs20WithdrawalWitness(
			testfixtures.LoadShieldedIcs20WithdrawalWitness("shielded_ics20_withdrawal"),
		)
		if err != nil {
			t.Fatalf("decode shielded ICS-20 withdrawal witness: %v", err)
		}
		reconstructed, err := ReconstructedShieldedIcs20WithdrawalStatementFieldsFromWitness(witness)
		if err != nil {
			t.Fatalf("reconstruct shielded ICS-20 withdrawal statement fields: %v", err)
		}
		nativeFields := make([]*big.Int, len(reconstructed))
		for i, field := range reconstructed {
			nativeFields[i] = primitives.LittleEndianBytesToBigInt(field[:])
		}
		hash, err := primitives.ShieldedIcs20WithdrawalStatementHashNativeForShape(
			nativeFields,
			int(witness.NIn),
		)
		if err != nil {
			t.Fatalf("hash reconstructed shielded ICS-20 withdrawal statement fields: %v", err)
		}
		claimed := primitives.LittleEndianBytesToBigInt(witness.ClaimedStatementHash[:])
		if hash.Cmp(claimed) != 0 {
			t.Fatalf(
				"shielded ICS-20 withdrawal reconstructed statement hash mismatch:\ngot=%s\nwant=%s",
				hash,
				claimed,
			)
		}
	})

	for _, tc := range []struct {
		name  string
		label string
	}{
		{name: "note_reshape1x8", label: "note_reshape1x8"},
		{name: "note_reshape8x1", label: "note_reshape8x1"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			witness, _, err := DecodeNoteReshapeWitness(
				testfixtures.LoadNoteReshapeWitness(tc.label),
			)
			if err != nil {
				t.Fatalf("decode note reshape witness: %v", err)
			}
			reconstructed, err := ReconstructedNoteReshapeStatementFieldsFromWitness(witness)
			if err != nil {
				t.Fatalf("reconstruct note reshape statement fields: %v", err)
			}
			nativeFields := make([]*big.Int, len(reconstructed))
			for i, field := range reconstructed {
				nativeFields[i] = primitives.LittleEndianBytesToBigInt(field[:])
			}
			hash, err := primitives.NoteReshapeStatementHashNativeForShape(
				nativeFields,
				tc.label,
				int(witness.NIn),
				int(witness.NOut),
			)
			if err != nil {
				t.Fatalf("hash reconstructed note reshape statement fields: %v", err)
			}
			claimed := primitives.LittleEndianBytesToBigInt(witness.ClaimedStatementHash[:])
			if hash.Cmp(claimed) != 0 {
				t.Fatalf("%s reconstructed statement hash mismatch:\ngot=%s\nwant=%s", tc.name, hash, claimed)
			}
		})
	}
}

func TestTransferStatementBalanceIsDerivedFromWitnessInputs(t *testing.T) {
	witness, _, err := DecodeTransferWitness(
		testfixtures.LoadTransferWitness("transfer"),
	)
	if err != nil {
		t.Fatalf("decode transfer witness: %v", err)
	}
	original, err := ReconstructedTransferStatementFieldsFromWitness(witness)
	if err != nil {
		t.Fatalf("reconstruct original transfer statement fields: %v", err)
	}

	witness.RequiredSpend.SpentNoteAmount[0]++
	mutated, err := ReconstructedTransferStatementFieldsFromWitness(witness)
	if err != nil {
		t.Fatalf("reconstruct mutated transfer statement fields: %v", err)
	}
	if bytes.Equal(original[5][:], mutated[5][:]) {
		t.Fatal("transfer balance statement field did not change with a private input amount")
	}
	if err := validateTransferStatementHash(witness); err == nil {
		t.Fatal("mutated transfer amount retained the original claimed statement hash")
	}
}

func TestTransferStatementBalanceRejectsOversizedAmount(t *testing.T) {
	witness, _, err := DecodeTransferWitness(
		testfixtures.LoadTransferWitness("transfer"),
	)
	if err != nil {
		t.Fatalf("decode transfer witness: %v", err)
	}
	witness.RequiredSpend.SpentNoteAmount = [32]byte{}
	witness.RequiredSpend.SpentNoteAmount[16] = 1
	if _, err := ReconstructedTransferStatementFieldsFromWitness(witness); err == nil {
		t.Fatal("transfer statement reconstruction accepted an amount outside the 128-bit circuit range")
	}
}
