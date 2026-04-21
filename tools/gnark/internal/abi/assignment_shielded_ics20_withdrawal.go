package abi

import (
	"fmt"

	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

func NewShieldedIcs20WithdrawalCircuitAssignmentFromWitnessV1(payload []byte) (*circuits.ShieldedIcs20WithdrawalCircuit, generated.ShieldedIcs20WithdrawalFamilySpec, error) {
	witness, family, err := DecodeShieldedIcs20WithdrawalWitnessV1(payload)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("decode ShieldedIcs20WithdrawalWitnessV1: %w", err)
	}
	assignment, err := newShieldedIcs20WithdrawalCircuitAssignment(witness, family.NIn)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, err
	}
	return assignment, family, nil
}

func newShieldedIcs20WithdrawalSpendCircuitFields(
	witness *ShieldedIcs20WithdrawalSpendWitnessV1Binary,
) (circuits.TransferSpendCircuitFields, error) {
	var zero circuits.TransferSpendCircuitFields
	statePath, err := statePathFromBinary(witness.StateCommitmentAuthPath)
	if err != nil {
		return zero, fmt.Errorf("decode shielded ICS-20 withdrawal spend state commitment auth path: %w", err)
	}
	return circuits.TransferSpendCircuitFields{
		Nullifier:          fqString(witness.Nullifier),
		RK:                 point2DString(witness.RKAffine),
		IsDummy:            boolToVariable(witness.IsDummy),
		DummyNullifierSeed: fqString(witness.DummyNullifierSeed),
		DummySpendAuthKey:  fqString(witness.DummySpendAuthKey),
		Note: noteFields(
			fqString(witness.SpentNoteBlinding),
			fqString(witness.SpentNoteAmount),
			fqString(witness.SpentNoteAssetID),
			fqString(witness.SpentDivGenAffine.X),
			fqString(witness.SpentDivGenAffine.Y),
			fqString(witness.SpentTransmissionKey),
			fqString(witness.SpentTransmissionAffine.X),
			fqString(witness.SpentTransmissionAffine.Y),
			fqString(witness.SpentClueKey),
		),
		StateProof: circuits.StateCommitmentFields{
			Commitment: fqString(witness.StateCommitmentCommitment),
			Position:   witness.StateCommitmentPosition,
			Path:       statePath,
		},
		AuthRandomizer: fqString(witness.SpendAuthRandomizer),
	}, nil
}

func newShieldedIcs20WithdrawalChangeCircuitFields(
	witness *ShieldedIcs20WithdrawalChangeWitnessV1Binary,
) circuits.ShieldedIcs20WithdrawalChangeCircuitFields {
	return circuits.ShieldedIcs20WithdrawalChangeCircuitFields{
		NoteCommitment: fqString(witness.NoteCommitment),
		Note: noteFields(
			fqString(witness.CreatedNoteBlinding),
			fqString(witness.CreatedNoteAmount),
			fqString(witness.CreatedNoteAssetID),
			fqString(witness.CreatedDivGenAffine.X),
			fqString(witness.CreatedDivGenAffine.Y),
			fqString(witness.CreatedTransmissionKey),
			fqString(witness.CreatedTransmissionAffine.X),
			fqString(witness.CreatedTransmissionAffine.Y),
			fqString(witness.CreatedClueKey),
		),
	}
}

func expectedShieldedIcs20WithdrawalStatementFieldCount(nIn int) int {
	return primitives.ShieldedIcs20WithdrawalStatementFieldCount(nIn)
}

func newShieldedIcs20WithdrawalCircuitAssignment(
	witness *ShieldedIcs20WithdrawalWitnessV1Binary,
	expectedNIn int,
) (*circuits.ShieldedIcs20WithdrawalCircuit, error) {
	if int(witness.NIn) != expectedNIn {
		return nil, fmt.Errorf("shielded ICS-20 withdrawal witness shape mismatch: got %d inputs, expected %d", witness.NIn, expectedNIn)
	}
	if len(witness.StatementFields) != expectedShieldedIcs20WithdrawalStatementFieldCount(expectedNIn) {
		return nil, fmt.Errorf("expected %d shielded ICS-20 withdrawal statement fields, got %d", expectedShieldedIcs20WithdrawalStatementFieldCount(expectedNIn), len(witness.StatementFields))
	}
	if len(witness.Spends) != expectedNIn {
		return nil, fmt.Errorf("shielded ICS-20 withdrawal witness count mismatch: spends=%d expected=%d", len(witness.Spends), expectedNIn)
	}

	assetPath, err := quadPathFromBinary(witness.AssetPath)
	if err != nil {
		return nil, fmt.Errorf("decode shielded ICS-20 withdrawal asset path: %w", err)
	}
	senderPath, err := quadPathFromBinary(witness.SenderCompliancePath)
	if err != nil {
		return nil, fmt.Errorf("decode shielded ICS-20 withdrawal sender compliance path: %w", err)
	}
	indexedLeaf := indexedLeafInputsFromIndexedLeafBinary(
		witness.AssetIndexedLeaf,
		witness.AssetIndexedLeafDKPub,
		witness.AssetIndexedLeafRingPK,
	)
	ivkReduced, quotientA, err := incomingViewingKeyReductionFromBinary(witness.NK, witness.AK)
	if err != nil {
		return nil, fmt.Errorf("compute shielded ICS-20 withdrawal ivk reduction from binary witness: %w", err)
	}

	assignment := circuits.NewShieldedIcs20WithdrawalCircuit(expectedNIn)
	assignment.ClaimedStatementHash = fqString(witness.ClaimedStatementHash)
	assignment.Anchor = fqString(witness.Anchor)
	assignment.BalanceCommitment = point2DString(witness.BalanceCommitmentAffine)
	assignment.AssetAnchor = fqString(witness.AssetAnchor)
	assignment.ComplianceAnchor = fqString(witness.ComplianceAnchor)
	assignment.TargetTimestamp = fqString(witness.TargetTimestamp)
	assignment.OutboundAssetID = fqString(witness.OutboundAssetID)
	assignment.OutboundAmount = fqString(witness.OutboundAmount)
	assignment.WithdrawalEffectHashLo = fqString(witness.WithdrawalEffectHashLo)
	assignment.WithdrawalEffectHashHi = fqString(witness.WithdrawalEffectHashHi)
	assignment.ActionBalanceBlinding = fqString(witness.ActionBalanceBlinding)
	assignment.IsRegulated = boolToVariable(witness.IsRegulated)
	assignment.Auth = circuits.TransferAuthSharedFields{
		AK:           point2DString(witness.AKAffine),
		NK:           primitives.LittleEndianBytesToBigInt(witness.NK[:]).String(),
		IVKReduced:   ivkReduced.String(),
		IVKQuotientA: quotientA,
	}
	assignment.Asset = circuits.AssetTreeFields{
		Leaf: indexedLeafFields(
			indexedLeaf.Value,
			indexedLeaf.NextValue,
			indexedLeaf.Threshold,
			indexedLeaf.ChannelsHash,
			indexedLeaf.NextIndex,
			fqString(witness.AssetIndexedLeafDKPub.X),
			fqString(witness.AssetIndexedLeafDKPub.Y),
			fqString(witness.AssetIndexedLeafRingPK.X),
			fqString(witness.AssetIndexedLeafRingPK.Y),
			indexedLeaf.RingIDHash,
			indexedLeaf.PolicyIDHash,
			indexedLeaf.PermissionHash,
			indexedLeaf.ResourceHash,
		),
		Path:     assetPath,
		Position: witness.AssetPosition,
	}
	assignment.Sender = userComplianceFields(
		fqString(witness.SenderDiversifiedGenerator.X),
		fqString(witness.SenderDiversifiedGenerator.Y),
		fqString(witness.SenderTransmissionKey.X),
		fqString(witness.SenderTransmissionKey.Y),
		fqString(witness.SenderAssetID),
		fqString(witness.SenderD),
		senderPath,
		witness.SenderCompliancePosition,
	)
	for i := range witness.Spends {
		spend, err := newShieldedIcs20WithdrawalSpendCircuitFields(&witness.Spends[i])
		if err != nil {
			return nil, err
		}
		assignment.Spends[i] = spend
	}
	assignment.ChangeOutput = newShieldedIcs20WithdrawalChangeCircuitFields(&witness.ChangeOutput)
	return assignment, nil
}
