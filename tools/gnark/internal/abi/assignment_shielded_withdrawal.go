package abi

import (
	"fmt"

	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

func NewShieldedWithdrawalCircuitAssignmentFromWitness(payload []byte) (*circuits.ShieldedWithdrawalCircuit, generated.ShieldedWithdrawalFamilySpec, error) {
	witness, family, err := DecodeShieldedWithdrawalWitness(payload)
	if err != nil {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, fmt.Errorf("decode ShieldedWithdrawalWitness: %w", err)
	}
	assignment, err := newShieldedWithdrawalCircuitAssignment(witness, family.NIn)
	if err != nil {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, err
	}
	return assignment, family, nil
}

func newShieldedWithdrawalRequiredSpendCircuitFields(
	witness *ShieldedWithdrawalRequiredSpendWitnessBinary,
) (circuits.ShieldedWithdrawalRequiredSpendCircuitFields, error) {
	var zero circuits.ShieldedWithdrawalRequiredSpendCircuitFields
	statePath, err := statePathFromBinary(witness.StateCommitmentAuthPath)
	if err != nil {
		return zero, fmt.Errorf("decode shielded withdrawal spend state commitment auth path: %w", err)
	}
	return circuits.ShieldedWithdrawalRequiredSpendCircuitFields{
		Nullifier: fqString(witness.Nullifier),
		RK:        point2DString(witness.RKAffine),
		Note: circuits.ShieldedWithdrawalNoteCircuitFields{
			Blinding:           fqString(witness.SpentNoteBlinding),
			Amount:             fqString(witness.SpentNoteAmount),
			RecoveryCommitment: fqString(witness.SpentNoteRecoveryCommitment),
		},
		StateProof: circuits.ShieldedWithdrawalStatePathCircuitFields{
			Position: witness.StateCommitmentPosition,
			Path:     statePath,
		},
		AuthRandomizer:  fqString(witness.SpendAuthRandomizer),
		HistoryRequired: boolToVariable(witness.HistoryRequired),
	}, nil
}

func newShieldedWithdrawalOptionalSpendCircuitFields(
	witness *ShieldedWithdrawalOptionalSpendWitnessBinary,
) (circuits.ShieldedWithdrawalOptionalSpendCircuitFields, error) {
	spend, err := newShieldedWithdrawalRequiredSpendCircuitFields(
		&witness.ShieldedWithdrawalRequiredSpendWitnessBinary,
	)
	if err != nil {
		return circuits.ShieldedWithdrawalOptionalSpendCircuitFields{}, err
	}
	return circuits.ShieldedWithdrawalOptionalSpendCircuitFields{
		ShieldedWithdrawalRequiredSpendCircuitFields: spend,
		IsDummy:            boolToVariable(witness.IsDummy),
		DummyNullifierSeed: fqString(witness.DummyNullifierSeed),
	}, nil
}

func newShieldedWithdrawalChangeCircuitFields(
	witness *ShieldedWithdrawalChangeWitnessBinary,
) circuits.ShieldedWithdrawalChangeCircuitFields {
	return circuits.ShieldedWithdrawalChangeCircuitFields{
		NoteCommitment: fqString(witness.NoteCommitment),
		Note: circuits.ShieldedWithdrawalNoteCircuitFields{
			Blinding:           fqString(witness.CreatedNoteBlinding),
			Amount:             fqString(witness.CreatedNoteAmount),
			RecoveryCommitment: fqString(witness.RecoveryCommitment),
		},
		Recovery: recoveryCapsuleFields(witness.RecoveryCommitment, witness.RecoveryCapsule),
	}
}

func newShieldedWithdrawalCircuitAssignment(
	witness *ShieldedWithdrawalWitnessBinary,
	expectedNIn int,
) (*circuits.ShieldedWithdrawalCircuit, error) {
	if int(witness.NIn) != expectedNIn {
		return nil, fmt.Errorf("shielded withdrawal witness shape mismatch: got %d inputs, expected %d", witness.NIn, expectedNIn)
	}
	reconstructedHash, err := reconstructedShieldedWithdrawalStatementHashFromWitness(witness)
	if err != nil {
		return nil, err
	}
	claimedHash := primitives.LittleEndianBytesToBigInt(witness.ClaimedStatementHash[:])
	if reconstructedHash.Cmp(claimedHash) != 0 {
		return nil, fmt.Errorf(
			"shielded withdrawal reconstructed statement hash mismatch: got %s, claimed %s",
			reconstructedHash,
			claimedHash,
		)
	}
	assetPath, err := quadPathFromBinary(witness.AssetPath)
	if err != nil {
		return nil, fmt.Errorf("decode shielded withdrawal asset path: %w", err)
	}
	senderPath, err := quadPathFromBinary(witness.SenderCompliancePath)
	if err != nil {
		return nil, fmt.Errorf("decode shielded withdrawal sender compliance path: %w", err)
	}
	akCompressed, err := pointAffineToField(witness.AKAffine)
	if err != nil {
		return nil, fmt.Errorf("compress shielded withdrawal authorization key: %w", err)
	}
	ivkReduced, quotientA, err := incomingViewingKeyReductionFromBinary(witness.NK, akCompressed)
	if err != nil {
		return nil, fmt.Errorf("compute shielded withdrawal ivk reduction from binary witness: %w", err)
	}

	assignment := circuits.NewShieldedWithdrawalCircuit(expectedNIn)
	assignment.ClaimedStatementHash = fqString(witness.ClaimedStatementHash)
	assignment.RoutingTag = fqString(witness.RoutingTag)
	assignment.RoutingParameterSetID = fqString(witness.RoutingParameterSetID)
	assignment.Compliance = circuits.ShieldedWithdrawalComplianceCircuitFields{
		EPK:             point2DString(witness.WithdrawalEPKAffine),
		C2:              fqString(witness.WithdrawalC2),
		KeyConfirmation: fqString(witness.WithdrawalKeyConfirmation),
		Seed:            fqString(witness.WithdrawalSeed),
		Randomizer:      fqString(witness.WithdrawalRandomizer),
	}
	for index := range witness.WithdrawalEncryptedSenderAddress {
		assignment.Compliance.EncryptedSenderAddress[index] =
			fqString(witness.WithdrawalEncryptedSenderAddress[index])
	}
	assignment.RecentPositionFloor = fqString(witness.RecentPositionFloor)
	assignment.Anchor = fqString(witness.Anchor)
	assignment.AssetAnchor = fqString(witness.AssetAnchor)
	assignment.ComplianceAnchor = fqString(witness.ComplianceAnchor)
	assignment.TargetTimestamp = fqString(witness.TargetTimestamp)
	assignment.OutboundAssetID = fqString(witness.OutboundAssetID)
	assignment.OutboundAmount = fqString(witness.OutboundAmount)
	for index := range witness.WithdrawalEffectHashLimbs {
		assignment.WithdrawalEffectHashLimbs[index] =
			fqString(witness.WithdrawalEffectHashLimbs[index])
	}
	assignment.ActionBalanceBlinding = fqString(witness.ActionBalanceBlinding)
	assignment.IsRegulated = boolToVariable(witness.IsRegulated)
	assignment.RegulatedPrecision = witness.RegulatedPrecision
	assignment.UnregulatedPrecision = witness.UnregulatedPrecision
	assignment.RoutingAsOfHeight = witness.RoutingAsOfHeight
	assignment.RoutingNonce = fqString(witness.RoutingNonce)
	volumeAccumulator, err := newVolumeAccumulatorCircuitFields(&witness.VolumeAccumulator, witness.TargetTimestamp)
	if err != nil {
		return nil, err
	}
	assignment.VolumeAccumulator = volumeAccumulator
	assignment.VolumeAccumulatorSeed = fqString(witness.VolumeAccumulatorSeed)
	assignment.Auth = circuits.TransferAuthSharedFields{
		AK:           point2DString(witness.AKAffine),
		NK:           primitives.LittleEndianBytesToBigInt(witness.NK[:]).String(),
		IVKReduced:   ivkReduced.String(),
		IVKQuotientA: quotientA,
	}
	assignment.Asset = circuits.AssetTreeFields{
		Leaf: indexedLeafFieldsFromIndexedLeafBinary(
			witness.AssetIndexedLeaf,
			witness.AssetIndexedLeafDKPubAffine,
			witness.AssetIndexedLeafRingPKAffine,
		),
		Path:     assetPath,
		Position: witness.AssetPosition,
	}
	assignment.Sender = circuits.ShieldedWithdrawalSenderCircuitFields{
		DivGen:        point2DString(witness.SenderDiversifiedGenerator),
		Capk:          point2DString(witness.SenderCapkAffine),
		RnkDhPk:       point2DString(witness.SenderRnkDhPkAffine),
		RnkCommitment: fqString(witness.SenderRnkCommitment),
		Status:        fqString(witness.SenderStatus),
		AuditKeys:     auditKeysInputs(witness.SenderAuditKeys),
		Path:          senderPath,
		Position:      witness.SenderCompliancePosition,
	}
	requiredSpend, err := newShieldedWithdrawalRequiredSpendCircuitFields(
		&witness.RequiredSpend,
	)
	if err != nil {
		return nil, err
	}
	optionalSpend, err := newShieldedWithdrawalOptionalSpendCircuitFields(
		&witness.OptionalSpend,
	)
	if err != nil {
		return nil, err
	}
	assignment.RequiredSpend = requiredSpend
	assignment.OptionalSpend = optionalSpend
	assignment.ChangeOutput = newShieldedWithdrawalChangeCircuitFields(&witness.ChangeOutput)
	return assignment, nil
}
