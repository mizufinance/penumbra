package abi

import (
	"math/big"
	"os"
	"path/filepath"
	"testing"

	"github.com/consensys/gnark/test"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"github.com/mizufinance/shieldd/tools/gnark/internal/testfixtures"
	"golang.org/x/crypto/blake2b"
)

func validNoteSeizureWitness(t *testing.T) *NoteSeizureWitnessBinary {
	t.Helper()
	transfer, _, err := DecodeTransferWitness(testfixtures.LoadTransferWitness("transfer"))
	if err != nil {
		t.Fatal(err)
	}
	output := transfer.ReceiverOutput
	divGen, err := pointAffineToField(output.RecipientDiversifiedGenerator)
	if err != nil {
		t.Fatal(err)
	}
	transmission, err := pointAffineToField(output.RecipientTransmissionKey)
	if err != nil {
		t.Fatal(err)
	}
	path := make([][3][32]byte, circuits.StateCommitmentDepth)
	nativePath := make([][3]*big.Int, circuits.StateCommitmentDepth)
	for i := range nativePath {
		for j := range nativePath[i] {
			nativePath[i][j] = new(big.Int)
		}
	}
	position := uint64(0)
	anchor, err := primitives.VerifyStateCommitmentPathNativeFromParts(
		primitives.LittleEndianBytesToBigInt(output.NoteCommitment[:]),
		position,
		nativePath,
	)
	if err != nil {
		t.Fatal(err)
	}
	anchorBytes, err := bigIntToLE32(anchor)
	if err != nil {
		t.Fatal(err)
	}
	rnk := big.NewInt(29)
	rnkBytes, _ := bigIntToLE32(rnk)
	rnkDomain := blake2b.Sum512([]byte("shieldd.compliance.nullifier_key"))
	rnkCommitment, err := primitives.Poseidon377Hash1Native(
		primitives.LittleEndianBytesToBigInt(rnkDomain[:]),
		rnk,
	)
	if err != nil {
		t.Fatal(err)
	}
	rnkCommitmentBytes, _ := bigIntToLE32(rnkCommitment)
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatal(err)
	}
	nullifier, err := primitives.Poseidon377Hash3Native(
		primitives.MustBigInt(vectors.Poseidon377.NullifierDomain),
		[3]*big.Int{
			rnk,
			primitives.LittleEndianBytesToBigInt(output.NoteCommitment[:]),
			new(big.Int).SetUint64(position),
		},
	)
	if err != nil {
		t.Fatal(err)
	}
	nullifierBytes, _ := bigIntToLE32(nullifier)
	authorizationCommitment, _ := bigIntToLE32(big.NewInt(77))
	witness := &NoteSeizureWitnessBinary{
		Anchor:                      anchorBytes,
		NoteCommitment:              output.NoteCommitment,
		Nullifier:                   nullifierBytes,
		HistoryRequired:             false,
		RecentPositionFloor:         uint64ToLE32(0),
		AddressDiversifiedGenerator: divGen,
		AddressTransmissionKey:      transmission,
		AssetID:                     transfer.RequiredSpend.SpentNoteAssetID,
		Amount:                      output.CreatedNoteAmount,
		AuthorizationCommitment:     authorizationCommitment,
		Recovery: NoteSeizureRecoveryWitnessBinary{
			Commitment:            output.RecoveryCommitment,
			EPKAffine:             output.RecoveryCapsule.EPKAffine,
			C2:                    output.RecoveryCapsule.C2,
			Salt:                  output.RecoveryCapsule.Salt,
			KeyConfirmation:       output.RecoveryCapsule.KeyConfirmation,
			EncryptedAmount:       output.RecoveryCapsule.EncryptedAmount,
			EncryptedNoteBlinding: output.RecoveryCapsule.EncryptedNoteBlinding,
			Seed:                  output.RecoveryCapsule.Seed,
		},
		NoteBlinding:            output.CreatedNoteBlinding,
		Position:                position,
		StateCommitmentAuthPath: path,
		RNK:                     rnkBytes,
		RnkCommitment:           rnkCommitmentBytes,
	}
	fields, err := ReconstructedNoteSeizureStatementFieldsFromWitness(witness)
	if err != nil {
		t.Fatal(err)
	}
	fieldInts := make([]*big.Int, len(fields))
	for i := range fields {
		fieldInts[i] = primitives.LittleEndianBytesToBigInt(fields[i][:])
	}
	claimed, err := primitives.NoteSeizureStatementHashNative(fieldInts)
	if err != nil {
		t.Fatal(err)
	}
	witness.ClaimedStatementHash, _ = bigIntToLE32(claimed)
	return witness
}

func TestNoteSeizureWitnessRoundTripAndSolve(t *testing.T) {
	witness := validNoteSeizureWitness(t)
	payload, err := EncodeNoteSeizureWitness(witness)
	if err != nil {
		t.Fatal(err)
	}
	if os.Getenv("SHIELDD_BLESS_GNARK_FIXTURES") == "1" {
		path := filepath.Join("..", "testfixtures", "vectors", "note_seizure_witness.bin")
		if err := os.WriteFile(path, payload, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	decoded, err := DecodeNoteSeizureWitness(payload)
	if err != nil {
		t.Fatal(err)
	}
	if decoded.TotalLength != uint32(len(payload)) {
		t.Fatalf("decoded length %d, want %d", decoded.TotalLength, len(payload))
	}
	assignment, err := NewNoteSeizureCircuitAssignmentFromWitness(payload)
	if err != nil {
		t.Fatal(err)
	}
	if err := test.IsSolved(circuits.NewNoteSeizureCircuit(), assignment, primitives.ScalarField()); err != nil {
		t.Fatal(err)
	}
}

func TestNoteSeizureCircuitRejectsMutatedBindings(t *testing.T) {
	tests := []struct {
		name   string
		mutate func(*NoteSeizureWitnessBinary)
	}{
		{
			name: "amount",
			mutate: func(w *NoteSeizureWitnessBinary) {
				w.Amount[0]++
			},
		},
		{
			name: "capsule seed",
			mutate: func(w *NoteSeizureWitnessBinary) {
				w.Recovery.Seed[0]++
			},
		},
		{
			name: "RNK",
			mutate: func(w *NoteSeizureWitnessBinary) {
				w.RNK[0]++
			},
		},
		{
			name: "SCT path",
			mutate: func(w *NoteSeizureWitnessBinary) {
				w.StateCommitmentAuthPath[0][0][0]++
			},
		},
		{
			name: "history classification",
			mutate: func(w *NoteSeizureWitnessBinary) {
				w.HistoryRequired = true
			},
		},
		{
			name: "authority commitment",
			mutate: func(w *NoteSeizureWitnessBinary) {
				w.AuthorizationCommitment[0]++
			},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			witness := validNoteSeizureWitness(t)
			tc.mutate(witness)
			payload, err := EncodeNoteSeizureWitness(witness)
			if err != nil {
				t.Fatal(err)
			}
			assignment, err := NewNoteSeizureCircuitAssignmentFromWitness(payload)
			if err != nil {
				t.Fatal(err)
			}
			if err := test.IsSolved(
				circuits.NewNoteSeizureCircuit(),
				assignment,
				primitives.ScalarField(),
			); err == nil {
				t.Fatal("mutated witness unexpectedly satisfied note seizure circuit")
			}
		})
	}
}
