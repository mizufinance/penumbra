package abi

import (
	"fmt"

	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

func NewNoteReshapeCircuitAssignmentFromWitness(payload []byte) (*circuits.NoteReshapeCircuit, generated.NoteReshapeFamilySpec, error) {
	witness, family, err := DecodeNoteReshapeWitness(payload)
	if err != nil {
		return nil, generated.NoteReshapeFamilySpec{}, fmt.Errorf("decode NoteReshapeWitness: %w", err)
	}
	if len(witness.Spends) != family.NIn || len(witness.Outputs) != family.NOut {
		return nil, generated.NoteReshapeFamilySpec{}, fmt.Errorf("note reshape witness counts mismatch: spends=%d outputs=%d expected=%dx%d", len(witness.Spends), len(witness.Outputs), family.NIn, family.NOut)
	}
	auth, err := newNoteReshapeAuthSharedFields(witness.NK, witness.AKAffine)
	if err != nil {
		return nil, generated.NoteReshapeFamilySpec{}, err
	}
	assignment := circuits.NewNoteReshapeCircuit(family.Label, family.NIn, family.NOut)
	assignment.ClaimedStatementHash = fqString(witness.ClaimedStatementHash)
	assignment.Anchor = fqString(witness.Anchor)
	assignment.AssetAnchor = fqString(witness.AssetAnchor)
	assignment.ComplianceAnchor = fqString(witness.ComplianceAnchor)
	assignment.RoutingTag = fqString(witness.RoutingTag)
	assignment.RoutingParameterSetID = fqString(witness.RoutingParameterSetID)
	assignment.RecentPositionFloor = fqString(witness.RecentPositionFloor)
	assignment.BalanceCommitment = point2DString(witness.BalanceCommitmentAffine)
	assignment.ActionBalanceBlinding = fqString(witness.ActionBalanceBlinding)
	assignment.Auth = auth
	assetPath, err := quadPathFromBinary(witness.AssetPath)
	if err != nil {
		return nil, generated.NoteReshapeFamilySpec{}, fmt.Errorf("decode note reshape asset path: %w", err)
	}
	assignment.Asset = circuits.AssetTreeFields{
		Leaf: indexedLeafFieldsFromIndexedLeafBinary(
			witness.AssetIndexedLeaf,
			witness.AssetIndexedLeafDKPub,
			witness.AssetIndexedLeafRingPK,
		),
		Path:     assetPath,
		Position: witness.AssetPosition,
	}
	assignment.IsRegulated = circuits.BoolToField(witness.IsRegulated)
	assignment.RegulatedPrecision = witness.RegulatedPrecision
	assignment.UnregulatedPrecision = witness.UnregulatedPrecision
	assignment.RoutingAsOfHeight = witness.RoutingAsOfHeight
	assignment.RoutingNonce = fqString(witness.RoutingNonce)
	assignment.Shared = circuits.NoteReshapeSharedNoteContextCircuitFields{
		AssetID: fqString(witness.Shared.AssetID),
		DivGen:  point2DString(witness.Shared.DivGen),
	}
	senderPath, err := quadPathFromBinary(witness.SenderCompliancePath)
	if err != nil {
		return nil, generated.NoteReshapeFamilySpec{}, fmt.Errorf("decode note reshape sender compliance path: %w", err)
	}
	assignment.Sender = circuits.NoteReshapeSenderCircuitFields{
		Capk:          point2DString(witness.SenderCapkAffine),
		RnkDhPk:       point2DString(witness.SenderRnkDhPkAffine),
		RnkCommitment: fqString(witness.SenderRnkCommitment),
		Status:        fqString(witness.SenderStatus),
		Path:          senderPath,
		Position:      witness.SenderCompliancePosition,
	}
	for i := range witness.Spends {
		spend, err := newNoteReshapeSpendCircuitFields(&witness.Spends[i])
		if err != nil {
			return nil, generated.NoteReshapeFamilySpec{}, err
		}
		if family.InputPadding == generated.InputPaddingSyntheticPrivate {
			assignment.SyntheticSpends[i] = circuits.NoteReshapeSyntheticSpendCircuitFields{
				NoteReshapeSpendCircuitFields: spend,
				IsDummy:                       boolVariable(witness.Spends[i].IsDummy),
				DummyNullifierSeed:            fqString(witness.Spends[i].DummyNullifierSeed),
			}
		} else {
			assignment.Spends[i] = spend
		}
	}
	for i := range witness.Outputs {
		assignment.Outputs[i] = newNoteReshapeOutputCircuitFields(&witness.Outputs[i])
	}
	return assignment, family, nil
}

func newNoteReshapeAuthSharedFields(nk [32]byte, akAffine PointAffineBinary) (circuits.TransferAuthSharedFields, error) {
	akCompressed, err := pointAffineToField(akAffine)
	if err != nil {
		return circuits.TransferAuthSharedFields{}, fmt.Errorf("compress note reshape authorization key: %w", err)
	}
	ivkReduced, quotientA, err := incomingViewingKeyReductionFromBinary(nk, akCompressed)
	if err != nil {
		return circuits.TransferAuthSharedFields{}, fmt.Errorf("compute note reshape ivk reduction from binary witness: %w", err)
	}
	return circuits.TransferAuthSharedFields{
		AK:           point2DString(akAffine),
		NK:           primitives.LittleEndianBytesToBigInt(nk[:]).String(),
		IVKReduced:   ivkReduced.String(),
		IVKQuotientA: quotientA,
	}, nil
}

func newNoteReshapeSpendCircuitFields(witness *NoteReshapeSpendWitnessBinary) (circuits.NoteReshapeSpendCircuitFields, error) {
	statePath, err := statePathFromBinary(witness.StateCommitmentAuthPath)
	if err != nil {
		return circuits.NoteReshapeSpendCircuitFields{}, fmt.Errorf("decode note reshape spend state commitment auth path: %w", err)
	}
	return circuits.NoteReshapeSpendCircuitFields{
		Nullifier: fqString(witness.Nullifier),
		RK:        point2DString(witness.RKAffine),
		Note: circuits.NoteReshapeNoteCircuitFields{
			Blinding:           fqString(witness.SpentNoteBlinding),
			Amount:             fqString(witness.SpentNoteAmount),
			RecoveryCommitment: fqString(witness.SpentNoteRecoveryCommitment),
		},
		StateProof:      circuits.StateCommitmentFields{Commitment: fqString(witness.StateCommitmentCommitment), Position: witness.StateCommitmentPosition, Path: statePath},
		AuthRandomizer:  fqString(witness.SpendAuthRandomizer),
		HistoryRequired: boolToVariable(witness.HistoryRequired),
	}, nil
}

func newNoteReshapeOutputCircuitFields(witness *NoteReshapeOutputWitnessBinary) circuits.NoteReshapeOutputCircuitFields {
	return circuits.NoteReshapeOutputCircuitFields{
		NoteCommitment: fqString(witness.NoteCommitment),
		Note: circuits.NoteReshapeNoteCircuitFields{
			Blinding:           fqString(witness.CreatedNoteBlinding),
			Amount:             fqString(witness.CreatedNoteAmount),
			RecoveryCommitment: fqString(witness.RecoveryCommitment),
		},
		Recovery: recoveryCapsuleFields(witness.RecoveryCommitment, witness.RecoveryCapsule),
	}
}

func boolVariable(value bool) string {
	if value {
		return "1"
	}
	return "0"
}
