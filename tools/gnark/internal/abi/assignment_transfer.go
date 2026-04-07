package abi

import (
	"fmt"

	"github.com/penumbra-zone/penumbra/tools/gnark/internal/circuits"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/generated"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

func NewTransferCircuitAssignmentFromWitnessV1(payload []byte) (*circuits.TransferCircuit, generated.TransferFamilySpec, error) {
	witness, family, err := DecodeTransferWitnessV1(payload)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, fmt.Errorf("decode TransferWitnessV1: %w", err)
	}
	assignment, err := newTransferCircuitAssignment(witness, family.NIn, family.NOut)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	return assignment, family, nil
}

func fqString(bytes [32]byte) string {
	return primitives.LittleEndianBytesToBigInt(bytes[:]).String()
}

func point2DString(point PointAffineBinary) circuits.Point2D {
	return circuits.Point2D{
		X: fqString(point.X),
		Y: fqString(point.Y),
	}
}

func expectedTransferStatementFieldCount(nIn, nOut int) int {
	return 5 + 11*nIn + 24*nOut
}

func newTransferSharedAssignmentParts(
	witness *TransferWitnessV1Binary,
	expectedNIn, expectedNOut int,
) (
	circuits.Point2D,
	circuits.TransferAuthSharedFields,
	circuits.AssetTreeFields,
	circuits.UserComplianceFields,
	error,
) {
	var zeroPoint circuits.Point2D
	var zeroAuth circuits.TransferAuthSharedFields
	var zeroAsset circuits.AssetTreeFields
	var zeroSender circuits.UserComplianceFields

	if int(witness.NIn) != expectedNIn || int(witness.NOut) != expectedNOut {
		return zeroPoint, zeroAuth, zeroAsset, zeroSender, fmt.Errorf(
			"transfer witness shape mismatch: got %dx%d, expected %dx%d",
			witness.NIn, witness.NOut, expectedNIn, expectedNOut,
		)
	}
	if len(witness.StatementFields) != expectedTransferStatementFieldCount(expectedNIn, expectedNOut) {
		return zeroPoint, zeroAuth, zeroAsset, zeroSender, fmt.Errorf(
			"expected %d transfer statement fields, got %d",
			expectedTransferStatementFieldCount(expectedNIn, expectedNOut),
			len(witness.StatementFields),
		)
	}

	assetPath, err := quadPathFromBinary(witness.AssetPath)
	if err != nil {
		return zeroPoint, zeroAuth, zeroAsset, zeroSender, fmt.Errorf("decode transfer asset path: %w", err)
	}
	senderPath, err := quadPathFromBinary(witness.SenderCompliancePath)
	if err != nil {
		return zeroPoint, zeroAuth, zeroAsset, zeroSender, fmt.Errorf("decode transfer sender compliance path: %w", err)
	}
	indexedLeaf := indexedLeafInputsFromIndexedLeafBinary(
		witness.AssetIndexedLeaf,
		witness.AssetIndexedLeafDKPub,
		witness.AssetIndexedLeafRingPK,
	)
	ivkReduced, quotientA, err := incomingViewingKeyReductionFromBinary(witness.NK, witness.AK)
	if err != nil {
		return zeroPoint, zeroAuth, zeroAsset, zeroSender, fmt.Errorf("compute transfer ivk reduction from binary witness: %w", err)
	}

	balanceCommitment := point2DString(witness.BalanceCommitmentAffine)
	auth := circuits.TransferAuthSharedFields{
		AK:           point2DString(witness.AKAffine),
		NK:           primitives.LittleEndianBytesToBigInt(witness.NK[:]).String(),
		IVKReduced:   ivkReduced.String(),
		IVKQuotientA: quotientA,
	}
	asset := circuits.AssetTreeFields{
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
	sender := userComplianceFields(
		fqString(witness.SenderDiversifiedGenerator.X),
		fqString(witness.SenderDiversifiedGenerator.Y),
		fqString(witness.SenderTransmissionKey.X),
		fqString(witness.SenderTransmissionKey.Y),
		fqString(witness.SenderAssetID),
		fqString(witness.SenderD),
		senderPath,
		witness.SenderCompliancePosition,
	)
	return balanceCommitment, auth, asset, sender, nil
}

func newTransferSpendCircuitFields(
	witness *TransferSpendWitnessV1Binary,
	isRegulated bool,
	txBlindingNonce [32]byte,
) (circuits.TransferSpendCircuitFields, error) {
	var zero circuits.TransferSpendCircuitFields
	if len(witness.SpendComplianceCiphertext) != circuits.SpendCiphertextFQCount {
		return zero, fmt.Errorf("expected %d transfer spend ciphertext elements, got %d", circuits.SpendCiphertextFQCount, len(witness.SpendComplianceCiphertext))
	}
	statePath, err := statePathFromBinary(witness.StateCommitmentAuthPath)
	if err != nil {
		return zero, fmt.Errorf("decode transfer spend state commitment auth path: %w", err)
	}
	fields := circuits.TransferSpendCircuitFields{
		Nullifier: fqString(witness.Nullifier),
		RK:        point2DString(witness.RKAffine),
		Note: noteFields(
			fqString(witness.SpentNoteBlinding),
			fqString(witness.SpentNoteAmount),
			fqString(witness.SpentNoteAssetID),
			fqString(witness.SpentDiversifiedGeneratorXY.X),
			fqString(witness.SpentDiversifiedGeneratorXY.Y),
			fqString(witness.SpentTransmissionKey),
			fqString(witness.SpentTransmissionKeyXY.X),
			fqString(witness.SpentTransmissionKeyXY.Y),
			fqString(witness.SpentClueKey),
		),
		StateProof: circuits.StateCommitmentFields{
			Commitment: fqString(witness.StateCommitmentCommitment),
			Position:   witness.StateCommitmentPosition,
			Path:       statePath,
		},
		AuthRandomizer: fqString(witness.SpendAuthRandomizer),
		Enc: circuits.SpendEncryptionFields{
			Epk:                 point2DString(witness.SpendEPKAffine),
			C2Core:              fqString(witness.SpendC2Core),
			IsRegulated:         circuits.BoolToField(isRegulated),
			IsFlagged:           circuits.BoolToField(witness.SpendIsFlagged),
			ComplianceEphemeral: fqString(witness.SpendComplianceEphemeral),
			Salt:                fqString(witness.SpendSalt),
			TxBlindingNonce:     fqString(txBlindingNonce),
		},
		Dleq: circuits.DLEQFields{
			C: fqString(witness.SpendDleqC),
			S: fqString(witness.SpendDleqS),
		},
	}
	for i := range witness.SpendComplianceCiphertext {
		fields.Enc.ComplianceCiphertext[i] = fqString(witness.SpendComplianceCiphertext[i])
	}
	return fields, nil
}

func newTransferOutputCircuitFields(
	witness *TransferOutputWitnessV1Binary,
	isRegulated bool,
	txBlindingNonce [32]byte,
) (circuits.TransferOutputCircuitFields, error) {
	var zero circuits.TransferOutputCircuitFields
	if len(witness.OutputComplianceCiphertext) != compliance.OutputCiphertextFQCount {
		return zero, fmt.Errorf("expected %d transfer output ciphertext elements, got %d", compliance.OutputCiphertextFQCount, len(witness.OutputComplianceCiphertext))
	}
	recipientPath, err := quadPathFromBinary(witness.RecipientCompliancePath)
	if err != nil {
		return zero, fmt.Errorf("decode transfer output recipient compliance path: %w", err)
	}
	fields := circuits.TransferOutputCircuitFields{
		NoteCommitment: fqString(witness.NoteCommitment),
		Note: noteFields(
			fqString(witness.CreatedNoteBlinding),
			fqString(witness.CreatedNoteAmount),
			fqString(witness.CreatedNoteAssetID),
			fqString(witness.CreatedDiversifiedGeneratorXY.X),
			fqString(witness.CreatedDiversifiedGeneratorXY.Y),
			fqString(witness.CreatedTransmissionKey),
			fqString(witness.CreatedTransmissionKeyXY.X),
			fqString(witness.CreatedTransmissionKeyXY.Y),
			fqString(witness.CreatedClueKey),
		),
		Recipient: userComplianceFields(
			fqString(witness.RecipientDiversifiedGenerator.X),
			fqString(witness.RecipientDiversifiedGenerator.Y),
			fqString(witness.RecipientTransmissionKey.X),
			fqString(witness.RecipientTransmissionKey.Y),
			fqString(witness.RecipientAssetID),
			fqString(witness.RecipientD),
			recipientPath,
			witness.RecipientCompliancePosition,
		),
		Enc: circuits.OutputEncryptionFields{
			Epk1:                point2DString(witness.OutputEPK1Affine),
			Epk2:                point2DString(witness.OutputEPK2Affine),
			Epk3:                point2DString(witness.OutputEPK3Affine),
			C2Core:              fqString(witness.OutputC2Core),
			C2Ext:               fqString(witness.OutputC2Ext),
			C2Sext:              fqString(witness.OutputC2Sext),
			IsRegulated:         circuits.BoolToField(isRegulated),
			IsFlagged:           circuits.BoolToField(witness.OutputIsFlagged),
			ComplianceEphemeral: fqString(witness.OutputComplianceEphemeral),
			R2:                  fqString(witness.OutputR2),
			R3:                  fqString(witness.OutputR3),
			Salt:                fqString(witness.OutputSalt),
			TxBlindingNonce:     fqString(txBlindingNonce),
		},
		Dleq: circuits.TransferOutputDLEQFields{
			Core: circuits.DLEQFields{
				C: fqString(witness.OutputDleqC1),
				S: fqString(witness.OutputDleqS1),
			},
			Ext: circuits.DLEQFields{
				C: fqString(witness.OutputDleqC2),
				S: fqString(witness.OutputDleqS2),
			},
			Sext: circuits.DLEQFields{
				C: fqString(witness.OutputDleqC3),
				S: fqString(witness.OutputDleqS3),
			},
		},
	}
	for i := range witness.OutputComplianceCiphertext {
		fields.Enc.ComplianceCiphertext[i] = fqString(witness.OutputComplianceCiphertext[i])
	}
	return fields, nil
}

func newTransferCircuitAssignment(
	witness *TransferWitnessV1Binary,
	expectedNIn, expectedNOut int,
) (*circuits.TransferCircuit, error) {
	balanceCommitment, auth, asset, sender, err := newTransferSharedAssignmentParts(witness, expectedNIn, expectedNOut)
	if err != nil {
		return nil, err
	}
	if len(witness.Spends) != expectedNIn {
		return nil, fmt.Errorf(
			"transfer witness spend count mismatch: witness.NIn=%d expectedNIn=%d len(Spends)=%d",
			witness.NIn, expectedNIn, len(witness.Spends),
		)
	}
	if len(witness.Outputs) != expectedNOut {
		return nil, fmt.Errorf(
			"transfer witness output count mismatch: witness.NOut=%d expectedNOut=%d len(Outputs)=%d",
			witness.NOut, expectedNOut, len(witness.Outputs),
		)
	}
	assignment := circuits.NewTransferCircuit(expectedNIn, expectedNOut)
	assignment.ClaimedStatementHash = fqString(witness.ClaimedStatementHash)
	assignment.Anchor = fqString(witness.Anchor)
	assignment.BalanceCommitment = balanceCommitment
	assignment.AssetAnchor = fqString(witness.AssetAnchor)
	assignment.ComplianceAnchor = fqString(witness.ComplianceAnchor)
	assignment.TargetTimestamp = fqString(witness.TargetTimestamp)
	assignment.ActionBalanceBlinding = fqString(witness.ActionBalanceBlinding)
	assignment.IsRegulated = circuits.BoolToField(witness.IsRegulated)
	assignment.TxBlindingNonce = fqString(witness.TxBlindingNonce)
	assignment.Auth = auth
	assignment.Asset = asset
	assignment.Sender = sender

	for i := range witness.Spends {
		spend, err := newTransferSpendCircuitFields(&witness.Spends[i], witness.IsRegulated, witness.TxBlindingNonce)
		if err != nil {
			return nil, err
		}
		assignment.Spends[i] = spend
	}
	for i := range witness.Outputs {
		output, err := newTransferOutputCircuitFields(&witness.Outputs[i], witness.IsRegulated, witness.TxBlindingNonce)
		if err != nil {
			return nil, err
		}
		assignment.Outputs[i] = output
	}
	return assignment, nil
}
