package abi

import (
	"fmt"

	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/compliance"
	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

func NewTransferCircuitAssignmentFromWitnessV1(payload []byte) (*circuits.TransferCircuit, generated.TransferFamilySpec, error) {
	witness, family, err := DecodeTransferWitnessV1(payload)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, fmt.Errorf("decode TransferWitnessV1: %w", err)
	}
	assignment, err := newTransferCircuitAssignment(witness)
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

func expectedTransferStatementFieldCount() int {
	return primitives.TransferStatementBaseFields +
		primitives.TransferStatementFieldsPerInput*circuits.TransferCircuitInputs +
		primitives.TransferStatementFieldsPerOutput*circuits.TransferCircuitOutputs
}

func newTransferSharedAssignmentParts(
	witness *TransferWitnessV1Binary,
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

	if int(witness.NIn) != circuits.TransferCircuitInputs || int(witness.NOut) != circuits.TransferCircuitOutputs {
		return zeroPoint, zeroAuth, zeroAsset, zeroSender, fmt.Errorf(
			"transfer witness shape mismatch: got %dx%d, expected %dx%d",
			witness.NIn, witness.NOut, circuits.TransferCircuitInputs, circuits.TransferCircuitOutputs,
		)
	}
	if len(witness.StatementFields) != expectedTransferStatementFieldCount() {
		return zeroPoint, zeroAuth, zeroAsset, zeroSender, fmt.Errorf(
			"expected %d transfer statement fields, got %d",
			expectedTransferStatementFieldCount(),
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
		Leaf: indexedLeafFieldsFromIndexedLeafBinary(
			witness.AssetIndexedLeaf,
			witness.AssetIndexedLeafDKPub,
			witness.AssetIndexedLeafRingPK,
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

func transferCoreTierFields(
	tier *TransferComplianceCiphertextWitnessV1Binary,
) (circuits.TransferComplianceCoreFields, error) {
	var zero circuits.TransferComplianceCoreFields
	if len(tier.Ciphertext) != compliance.TransferCoreCiphertextFQCount {
		return zero, fmt.Errorf(
			"expected %d transfer core ciphertext elements, got %d",
			compliance.TransferCoreCiphertextFQCount,
			len(tier.Ciphertext),
		)
	}
	fields := circuits.TransferComplianceCoreFields{
		Epk:   point2DString(tier.EPKAffine),
		C2:    fqString(tier.C2),
		Proof: transferTierProofFields(tier),
	}
	for i := range tier.Ciphertext {
		fields.Ciphertext[i] = fqString(tier.Ciphertext[i])
	}
	return fields, nil
}

func transferExtTierFields(
	tier *TransferComplianceCiphertextWitnessV1Binary,
) (circuits.TransferComplianceExtFields, error) {
	var zero circuits.TransferComplianceExtFields
	if len(tier.Ciphertext) != compliance.TransferExtCiphertextFQCount {
		return zero, fmt.Errorf(
			"expected %d transfer ext ciphertext elements, got %d",
			compliance.TransferExtCiphertextFQCount,
			len(tier.Ciphertext),
		)
	}
	fields := circuits.TransferComplianceExtFields{
		Epk:   point2DString(tier.EPKAffine),
		C2:    fqString(tier.C2),
		Proof: transferTierProofFields(tier),
	}
	for i := range tier.Ciphertext {
		fields.Ciphertext[i] = fqString(tier.Ciphertext[i])
	}
	return fields, nil
}

func transferTierProofFields(
	tier *TransferComplianceCiphertextWitnessV1Binary,
) circuits.TransferComplianceProofFields {
	return circuits.TransferComplianceProofFields{
		Statement: circuits.TransferComplianceStatementFields{
			SubjectBD:       fqString(tier.SubjectBD),
			RingIDHash:      fqString(tier.RingIDHash),
			PolicyIDHash:    fqString(tier.PolicyIDHash),
			ResourceHash:    fqString(tier.ResourceHash),
			PermissionHash:  fqString(tier.PermissionHash),
			Tier:            tier.Tier,
			TargetTimestamp: fqString(tier.StatementTimestamp),
			Salt:            fqString(tier.Salt),
		},
		Challenge:   fqString(tier.Challenge),
		Response:    fqString(tier.Response),
		DerivedPK:   point2DString(tier.DerivedPKAffine),
		EncCmt:      point2DString(tier.EncCmtAffine),
		SharedPoint: point2DString(tier.SharedPointAffine),
	}
}

func newTransferComplianceFields(
	witness *TransferWitnessV1Binary,
) (circuits.TransferComplianceFields, error) {
	var zero circuits.TransferComplianceFields
	if len(witness.DetectionCiphertext) != compliance.TransferDetectionFQCount {
		return zero, fmt.Errorf(
			"expected %d transfer detection ciphertext elements, got %d",
			compliance.TransferDetectionFQCount,
			len(witness.DetectionCiphertext),
		)
	}

	senderCore, err := transferCoreTierFields(&witness.SenderCore)
	if err != nil {
		return zero, fmt.Errorf("decode transfer sender_core tier: %w", err)
	}
	senderExt, err := transferExtTierFields(&witness.SenderExt)
	if err != nil {
		return zero, fmt.Errorf("decode transfer sender_ext tier: %w", err)
	}
	outputCore, err := transferCoreTierFields(&witness.OutputCore)
	if err != nil {
		return zero, fmt.Errorf("decode transfer output_core tier: %w", err)
	}
	outputExt, err := transferExtTierFields(&witness.OutputExt)
	if err != nil {
		return zero, fmt.Errorf("decode transfer output_ext tier: %w", err)
	}

	fields := circuits.TransferComplianceFields{
		TransferNonceRoot: fqString(witness.TransferNonceRoot),
		SenderRCore:       fqString(witness.SenderRCore),
		SenderRExt:        fqString(witness.SenderRExt),
		OutputRCore:       fqString(witness.OutputRCore),
		OutputRExt:        fqString(witness.OutputRExt),
		SenderCore:        senderCore,
		SenderExt:         senderExt,
		OutputCore:        outputCore,
		OutputExt:         outputExt,
	}
	for i := range witness.DetectionCiphertext {
		fields.DetectionCiphertext[i] = fqString(witness.DetectionCiphertext[i])
	}
	return fields, nil
}

func newTransferSpendCircuitFields(
	witness *TransferSpendWitnessV1Binary,
) (circuits.TransferSpendCircuitFields, error) {
	var zero circuits.TransferSpendCircuitFields
	statePath, err := statePathFromBinary(witness.StateCommitmentAuthPath)
	if err != nil {
		return zero, fmt.Errorf("decode transfer spend state commitment auth path: %w", err)
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
	}, nil
}

func boolToVariable(value bool) int {
	if value {
		return 1
	}
	return 0
}

func newTransferOutputCircuitFields(
	witness *TransferOutputWitnessV1Binary,
) (circuits.TransferOutputCircuitFields, error) {
	var zero circuits.TransferOutputCircuitFields
	recipientPath, err := quadPathFromBinary(witness.RecipientCompliancePath)
	if err != nil {
		return zero, fmt.Errorf("decode transfer output recipient compliance path: %w", err)
	}
	return circuits.TransferOutputCircuitFields{
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
		IsReceiver: circuits.BoolToField(witness.IsReceiver),
	}, nil
}

func newTransferCircuitAssignment(
	witness *TransferWitnessV1Binary,
) (*circuits.TransferCircuit, error) {
	balanceCommitment, auth, asset, sender, err := newTransferSharedAssignmentParts(witness)
	if err != nil {
		return nil, err
	}
	if len(witness.Spends) != circuits.TransferCircuitInputs {
		return nil, fmt.Errorf(
			"transfer witness spend count mismatch: witness.NIn=%d expectedNIn=%d len(Spends)=%d",
			witness.NIn, circuits.TransferCircuitInputs, len(witness.Spends),
		)
	}
	if len(witness.Outputs) != circuits.TransferCircuitOutputs {
		return nil, fmt.Errorf(
			"transfer witness output count mismatch: witness.NOut=%d expectedNOut=%d len(Outputs)=%d",
			witness.NOut, circuits.TransferCircuitOutputs, len(witness.Outputs),
		)
	}
	if !witness.Outputs[0].IsReceiver {
		return nil, fmt.Errorf("transfer witness output 0 must be the receiver leg")
	}
	for i := 1; i < len(witness.Outputs); i++ {
		if witness.Outputs[i].IsReceiver {
			return nil, fmt.Errorf("transfer witness output %d must be change, not receiver", i)
		}
	}

	complianceFields, err := newTransferComplianceFields(witness)
	if err != nil {
		return nil, err
	}

	assignment := circuits.NewTransferCircuit()
	assignment.ClaimedStatementHash = fqString(witness.ClaimedStatementHash)
	assignment.Anchor = fqString(witness.Anchor)
	assignment.BalanceCommitment = balanceCommitment
	assignment.AssetAnchor = fqString(witness.AssetAnchor)
	assignment.ComplianceAnchor = fqString(witness.ComplianceAnchor)
	assignment.TargetTimestamp = fqString(witness.TargetTimestamp)
	assignment.ActionBalanceBlinding = fqString(witness.ActionBalanceBlinding)
	assignment.IsRegulated = circuits.BoolToField(witness.IsRegulated)
	assignment.Auth = auth
	assignment.Asset = asset
	assignment.Sender = sender
	assignment.Compliance = complianceFields

	for i := range witness.Spends {
		spend, err := newTransferSpendCircuitFields(&witness.Spends[i])
		if err != nil {
			return nil, err
		}
		assignment.Spends[i] = spend
	}
	for i := range witness.Outputs {
		output, err := newTransferOutputCircuitFields(&witness.Outputs[i])
		if err != nil {
			return nil, err
		}
		assignment.Outputs[i] = output
	}
	return assignment, nil
}
