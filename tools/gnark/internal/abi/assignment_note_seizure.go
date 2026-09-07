package abi

import (
	"fmt"

	"github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
)

func NewNoteSeizureCircuitAssignmentFromWitness(payload []byte) (*circuits.NoteSeizureCircuit, error) {
	witness, err := DecodeNoteSeizureWitness(payload)
	if err != nil {
		return nil, fmt.Errorf("decode NoteSeizureWitness: %w", err)
	}
	path, err := statePathFromBinary(witness.StateCommitmentAuthPath)
	if err != nil {
		return nil, fmt.Errorf("decode note seizure SCT path: %w", err)
	}
	recovery := witness.Recovery
	return &circuits.NoteSeizureCircuit{
		ClaimedStatementHash:    fqString(witness.ClaimedStatementHash),
		Anchor:                  fqString(witness.Anchor),
		NoteCommitment:          fqString(witness.NoteCommitment),
		Nullifier:               fqString(witness.Nullifier),
		HistoryRequired:         boolToVariable(witness.HistoryRequired),
		RecentPositionFloor:     fqString(witness.RecentPositionFloor),
		AddressDivGen:           fqString(witness.AddressDiversifiedGenerator),
		AddressTransmissionKey:  fqString(witness.AddressTransmissionKey),
		AssetID:                 fqString(witness.AssetID),
		Amount:                  fqString(witness.Amount),
		RnkCommitment:           fqString(witness.RnkCommitment),
		AuthorizationCommitment: fqString(witness.AuthorizationCommitment),
		Recovery: compliance.RecoveryCapsuleFields{
			Commitment: fqString(recovery.Commitment),
			EPK: twistededwards.Point{
				X: fqString(recovery.EPKAffine.X),
				Y: fqString(recovery.EPKAffine.Y),
			},
			C2:                    fqString(recovery.C2),
			Salt:                  fqString(recovery.Salt),
			KeyConfirmation:       fqString(recovery.KeyConfirmation),
			EncryptedAmount:       fqString(recovery.EncryptedAmount),
			EncryptedNoteBlinding: fqString(recovery.EncryptedNoteBlinding),
			Seed:                  fqString(recovery.Seed),
			Randomizer:            0,
		},
		NoteBlinding: fqString(witness.NoteBlinding),
		Position:     witness.Position,
		Path:         path,
		RNK:          fqString(witness.RNK),
	}, nil
}
