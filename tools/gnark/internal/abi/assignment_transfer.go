package abi

import (
	"fmt"
	"math/big"

	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

func NewTransferCircuitAssignmentFromWitness(
	payload []byte,
) (*circuits.TransferCircuit, generated.TransferFamilySpec, error) {
	witness, family, err := DecodeTransferWitness(payload)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, fmt.Errorf("decode TransferWitness: %w", err)
	}
	if err := validateTransferStatementHash(witness); err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	assignment, err := newTransferCircuitAssignment(witness)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	return assignment, family, nil
}

func fqString(value [32]byte) string {
	return primitives.LittleEndianBytesToBigInt(value[:]).String()
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

func validateTransferStatementHash(witness *TransferWitnessBinary) error {
	fields, err := ReconstructedTransferStatementFieldsFromWitness(witness)
	if err != nil {
		return fmt.Errorf("reconstruct TransferWitness statement: %w", err)
	}
	fieldElements := make([]*big.Int, len(fields))
	for i := range fields {
		fieldElements[i] = primitives.LittleEndianBytesToBigInt(fields[i][:])
	}
	computed, err := primitives.TransferStatementHashNativeForShape(
		fieldElements,
		circuits.TransferCircuitInputs,
		circuits.TransferCircuitOutputs,
	)
	if err != nil {
		return fmt.Errorf("hash reconstructed TransferWitness statement: %w", err)
	}
	claimed := primitives.LittleEndianBytesToBigInt(witness.ClaimedStatementHash[:])
	if computed.Cmp(claimed) != 0 {
		return fmt.Errorf(
			"TransferWitness claimed statement hash mismatch: reconstructed=%s claimed=%s",
			computed,
			claimed,
		)
	}
	return nil
}

func newTransferSharedAssignmentParts(
	witness *TransferWitnessBinary,
) (
	circuits.TransferAuthSharedFields,
	circuits.AssetTreeFields,
	circuits.TransferUserCircuitFields,
	error,
) {
	var zeroAuth circuits.TransferAuthSharedFields
	var zeroAsset circuits.AssetTreeFields
	var zeroSender circuits.TransferUserCircuitFields

	assetPath, err := quadPathFromBinary(witness.AssetPath)
	if err != nil {
		return zeroAuth, zeroAsset, zeroSender, fmt.Errorf("decode transfer asset path: %w", err)
	}
	senderPath, err := quadPathFromBinary(witness.SenderCompliancePath)
	if err != nil {
		return zeroAuth, zeroAsset, zeroSender, fmt.Errorf("decode transfer sender compliance path: %w", err)
	}
	akCompressed, err := pointAffineToField(witness.AKAffine)
	if err != nil {
		return zeroAuth, zeroAsset, zeroSender, fmt.Errorf("compress transfer ak: %w", err)
	}
	ivkReduced, quotientA, err := incomingViewingKeyReductionFromBinary(witness.NK, akCompressed)
	if err != nil {
		return zeroAuth, zeroAsset, zeroSender, fmt.Errorf("compute transfer ivk reduction from affine ak: %w", err)
	}

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
	sender := circuits.TransferUserCircuitFields{
		DivGen:        point2DString(witness.SenderDiversifiedGenerator),
		Transmission:  point2DString(witness.SenderTransmissionKey),
		Capk:          point2DString(witness.SenderCapkAffine),
		RnkDhPk:       point2DString(witness.SenderRnkDhPkAffine),
		RnkCommitment: fqString(witness.SenderRnkCommitment),
		Status:        fqString(witness.SenderStatus),
		Path:          senderPath,
		Position:      witness.SenderCompliancePosition,
	}
	return auth, asset, sender, nil
}

func transferCoreTierFields(
	tier *TransferComplianceCiphertextWitnessBinary,
	keyConfirmation [32]byte,
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
		Epk:             point2DString(tier.EPKAffine),
		C2:              fqString(tier.C2),
		KeyConfirmation: fqString(keyConfirmation),
	}
	for i := range tier.Ciphertext {
		fields.Ciphertext[i] = fqString(tier.Ciphertext[i])
	}
	return fields, nil
}

func transferExtTierFields(
	tier *TransferComplianceCiphertextWitnessBinary,
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
		Epk: point2DString(tier.EPKAffine),
		C2:  fqString(tier.C2),
	}
	for i := range tier.Ciphertext {
		fields.Ciphertext[i] = fqString(tier.Ciphertext[i])
	}
	return fields, nil
}

func newTransferComplianceFields(
	witness *TransferWitnessBinary,
) (circuits.TransferComplianceFields, error) {
	var zero circuits.TransferComplianceFields
	if len(witness.DetectionCiphertext) != compliance.TransferDetectionFQCount {
		return zero, fmt.Errorf(
			"expected %d transfer detection ciphertext elements, got %d",
			compliance.TransferDetectionFQCount,
			len(witness.DetectionCiphertext),
		)
	}

	senderCore, err := transferCoreTierFields(
		&witness.SenderCore,
		witness.SenderCoreKeyConfirmation,
	)
	if err != nil {
		return zero, fmt.Errorf("decode transfer sender_core tier: %w", err)
	}
	senderExt, err := transferExtTierFields(&witness.SenderExt)
	if err != nil {
		return zero, fmt.Errorf("decode transfer sender_ext tier: %w", err)
	}
	outputCore, err := transferCoreTierFields(
		&witness.OutputCore,
		witness.OutputCoreKeyConfirmation,
	)
	if err != nil {
		return zero, fmt.Errorf("decode transfer output_core tier: %w", err)
	}
	outputExt, err := transferExtTierFields(&witness.OutputExt)
	if err != nil {
		return zero, fmt.Errorf("decode transfer output_ext tier: %w", err)
	}

	fields := circuits.TransferComplianceFields{
		TransferNonceRoot: fqString(witness.TransferNonceRoot),
		Metadata: circuits.TransferComplianceMetadataFields{
			RingIDHash:      fqString(witness.Metadata.RingIDHash),
			PolicyIDHash:    fqString(witness.Metadata.PolicyIDHash),
			ResourceHash:    fqString(witness.Metadata.ResourceHash),
			PermissionHash:  fqString(witness.Metadata.PermissionHash),
			TargetTimestamp: fqString(witness.Metadata.TargetTimestamp),
			SenderCoreSalt:  fqString(witness.Metadata.SenderCoreSalt),
			SenderExtSalt:   fqString(witness.Metadata.SenderExtSalt),
			OutputCoreSalt:  fqString(witness.Metadata.OutputCoreSalt),
			OutputExtSalt:   fqString(witness.Metadata.OutputExtSalt),
		},
		SenderRCore: fqString(witness.SenderRCore),
		SenderRExt:  fqString(witness.SenderRExt),
		OutputRCore: fqString(witness.OutputRCore),
		OutputRExt:  fqString(witness.OutputRExt),
		SenderCore:  senderCore,
		SenderExt:   senderExt,
		OutputCore:  outputCore,
		OutputExt:   outputExt,
	}
	for i := range witness.DetectionCiphertext {
		fields.DetectionCiphertext[i] = fqString(witness.DetectionCiphertext[i])
	}
	return fields, nil
}

func transferNotePayloadFields(
	blinding [32]byte,
	amount [32]byte,
	recoveryCommitment [32]byte,
) circuits.TransferNotePayloadCircuitFields {
	return circuits.TransferNotePayloadCircuitFields{
		Blinding:           fqString(blinding),
		Amount:             fqString(amount),
		RecoveryCommitment: fqString(recoveryCommitment),
	}
}

func transferStatePathFields(
	position uint64,
	path [][3][32]byte,
) (circuits.TransferStatePathCircuitFields, error) {
	statePath, err := statePathFromBinary(path)
	if err != nil {
		return circuits.TransferStatePathCircuitFields{}, err
	}
	return circuits.TransferStatePathCircuitFields{
		Position: position,
		Path:     statePath,
	}, nil
}

func newTransferRequiredSpendCircuitFields(
	witness *TransferRequiredSpendWitnessBinary,
) (circuits.TransferRequiredSpendCircuitFields, error) {
	stateProof, err := transferStatePathFields(
		witness.StateCommitmentPosition,
		witness.StateCommitmentAuthPath,
	)
	if err != nil {
		return circuits.TransferRequiredSpendCircuitFields{}, fmt.Errorf(
			"decode required transfer spend state path: %w",
			err,
		)
	}
	return circuits.TransferRequiredSpendCircuitFields{
		Nullifier: fqString(witness.Nullifier),
		RK:        point2DString(witness.RKAffine),
		Note: circuits.TransferRequiredSpendNoteCircuitFields{
			TransferNotePayloadCircuitFields: transferNotePayloadFields(
				witness.SpentNoteBlinding,
				witness.SpentNoteAmount,
				witness.SpentNoteRecoveryCommitment,
			),
			AssetID: fqString(witness.SpentNoteAssetID),
		},
		StateProof:      stateProof,
		AuthRandomizer:  fqString(witness.SpendAuthRandomizer),
		HistoryRequired: boolToVariable(witness.HistoryRequired),
	}, nil
}

func newTransferOptionalSpendCircuitFields(
	witness *TransferOptionalSpendWitnessBinary,
) (circuits.TransferOptionalSpendCircuitFields, error) {
	stateProof, err := transferStatePathFields(
		witness.StateCommitmentPosition,
		witness.StateCommitmentAuthPath,
	)
	if err != nil {
		return circuits.TransferOptionalSpendCircuitFields{}, fmt.Errorf(
			"decode optional transfer spend state path: %w",
			err,
		)
	}
	return circuits.TransferOptionalSpendCircuitFields{
		Nullifier: fqString(witness.Nullifier),
		RK:        point2DString(witness.RKAffine),
		Note: transferNotePayloadFields(
			witness.SpentNoteBlinding,
			witness.SpentNoteAmount,
			witness.SpentNoteRecoveryCommitment,
		),
		StateProof:         stateProof,
		AuthRandomizer:     fqString(witness.SpendAuthRandomizer),
		IsDummy:            boolToVariable(witness.IsDummy),
		DummyNullifierSeed: fqString(witness.DummyNullifierSeed),
		HistoryRequired:    boolToVariable(witness.HistoryRequired),
	}, nil
}

func boolToVariable(value bool) int {
	if value {
		return 1
	}
	return 0
}

func newTransferReceiverOutputCircuitFields(
	witness *TransferReceiverOutputWitnessBinary,
) (circuits.TransferReceiverOutputCircuitFields, error) {
	recipientPath, err := quadPathFromBinary(witness.RecipientCompliancePath)
	if err != nil {
		return circuits.TransferReceiverOutputCircuitFields{}, fmt.Errorf(
			"decode transfer receiver compliance path: %w",
			err,
		)
	}
	return circuits.TransferReceiverOutputCircuitFields{
		NoteCommitment: fqString(witness.NoteCommitment),
		Note: circuits.TransferReceiverNoteCircuitFields{
			TransferNotePayloadCircuitFields: transferNotePayloadFields(
				witness.CreatedNoteBlinding,
				witness.CreatedNoteAmount,
				witness.RecoveryCommitment,
			),
		},
		Recipient: circuits.TransferUserCircuitFields{
			DivGen:        point2DString(witness.RecipientDiversifiedGenerator),
			Transmission:  point2DString(witness.RecipientTransmissionKey),
			Capk:          point2DString(witness.RecipientCapkAffine),
			RnkDhPk:       point2DString(witness.RecipientRnkDhPkAffine),
			RnkCommitment: fqString(witness.RecipientRnkCommitment),
			Status:        fqString(witness.RecipientStatus),
			Path:          recipientPath,
			Position:      witness.RecipientCompliancePosition,
		},
		Recovery: recoveryCapsuleFields(witness.RecoveryCommitment, witness.RecoveryCapsule),
	}, nil
}

func newTransferChangeOutputCircuitFields(
	witness *TransferChangeOutputWitnessBinary,
) circuits.TransferChangeOutputCircuitFields {
	return circuits.TransferChangeOutputCircuitFields{
		NoteCommitment: fqString(witness.NoteCommitment),
		Note: transferNotePayloadFields(
			witness.CreatedNoteBlinding,
			witness.CreatedNoteAmount,
			witness.RecoveryCommitment,
		),
		Recovery: recoveryCapsuleFields(witness.RecoveryCommitment, witness.RecoveryCapsule),
	}
}

func newTransferVolumeAccumulatorCircuitFields(
	witness *TransferWitnessBinary,
) (circuits.TransferVolumeAccumulatorCircuitFields, error) {
	return newVolumeAccumulatorCircuitFields(&witness.VolumeAccumulator, witness.TargetTimestamp)
}

func newVolumeAccumulatorCircuitFields(
	witness *TransferVolumeAccumulatorWitnessBinary,
	targetTimestamp [32]byte,
) (circuits.TransferVolumeAccumulatorCircuitFields, error) {
	priorProof, err := transferStatePathFields(
		witness.PriorPosition,
		witness.PriorAuthPath,
	)
	if err != nil {
		return circuits.TransferVolumeAccumulatorCircuitFields{}, fmt.Errorf(
			"decode volume accumulator prior state path: %w",
			err,
		)
	}
	target := primitives.LittleEndianBytesToBigInt(targetTimestamp[:])
	if !target.IsUint64() {
		return circuits.TransferVolumeAccumulatorCircuitFields{}, fmt.Errorf("target timestamp exceeds u64")
	}
	targetU64 := target.Uint64()
	return circuits.TransferVolumeAccumulatorCircuitFields{
		Nullifier:         fqString(witness.Nullifier),
		Commitment:        fqString(witness.Commitment),
		DayStart:          fqString(witness.DayStart),
		ProofContext:      fqString(witness.ProofContext),
		UseReal:           boolToVariable(witness.UseReal),
		StartsNewDay:      boolToVariable(witness.StartsNewDay),
		TimestampDayIndex: targetU64 / 86400,
		TimestampSecond:   targetU64 % 86400,
		Subject:           fqString(witness.Subject),
		PriorVolume:       fqString(witness.PriorVolume),
		PriorBlinding:     fqString(witness.PriorBlinding),
		PriorCommitment:   fqString(witness.PriorCommitment),
		PriorStateProof:   priorProof,
		SuccessorVolume:   fqString(witness.SuccessorVolume),
		SuccessorBlinding: fqString(witness.SuccessorBlinding),
	}, nil
}

func newTransferCircuitAssignment(
	witness *TransferWitnessBinary,
) (*circuits.TransferCircuit, error) {
	auth, asset, sender, err := newTransferSharedAssignmentParts(witness)
	if err != nil {
		return nil, err
	}
	complianceFields, err := newTransferComplianceFields(witness)
	if err != nil {
		return nil, err
	}
	requiredSpend, err := newTransferRequiredSpendCircuitFields(&witness.RequiredSpend)
	if err != nil {
		return nil, err
	}
	optionalSpend, err := newTransferOptionalSpendCircuitFields(&witness.OptionalSpend)
	if err != nil {
		return nil, err
	}
	receiverOutput, err := newTransferReceiverOutputCircuitFields(&witness.ReceiverOutput)
	if err != nil {
		return nil, err
	}
	volumeAccumulator, err := newTransferVolumeAccumulatorCircuitFields(witness)
	if err != nil {
		return nil, err
	}

	assignment := circuits.NewTransferCircuit()
	assignment.ClaimedStatementHash = fqString(witness.ClaimedStatementHash)
	for i := range assignment.RoutingTags {
		assignment.RoutingTags[i] = fqString(witness.RoutingTags[i])
	}
	assignment.RoutingParameterSetID = fqString(witness.RoutingParameterSetID)
	assignment.RecentPositionFloor = fqString(witness.RecentPositionFloor)
	assignment.Anchor = fqString(witness.Anchor)
	assignment.AssetAnchor = fqString(witness.AssetAnchor)
	assignment.ComplianceAnchor = fqString(witness.ComplianceAnchor)
	assignment.TargetTimestamp = fqString(witness.TargetTimestamp)
	assignment.ActionBalanceBlinding = fqString(witness.ActionBalanceBlinding)
	assignment.IsRegulated = circuits.BoolToField(witness.IsRegulated)
	assignment.RegulatedPrecision = witness.RegulatedPrecision
	assignment.UnregulatedPrecision = witness.UnregulatedPrecision
	assignment.RoutingAsOfHeight = witness.RoutingAsOfHeight
	assignment.Auth = auth
	assignment.Asset = asset
	assignment.Sender = sender
	assignment.Compliance = complianceFields
	assignment.RequiredSpend = requiredSpend
	assignment.OptionalSpend = optionalSpend
	assignment.ReceiverOutput = receiverOutput
	assignment.ChangeOutput = newTransferChangeOutputCircuitFields(&witness.ChangeOutput)
	assignment.VolumeAccumulator = volumeAccumulator
	return assignment, nil
}
