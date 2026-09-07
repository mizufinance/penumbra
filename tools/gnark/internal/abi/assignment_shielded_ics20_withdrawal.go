package abi

import (
	"fmt"

	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

func NewShieldedIcs20WithdrawalCircuitAssignmentFromWitness(payload []byte) (*circuits.ShieldedIcs20WithdrawalCircuit, generated.ShieldedIcs20WithdrawalFamilySpec, error) {
	witness, family, err := DecodeShieldedIcs20WithdrawalWitness(payload)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("decode ShieldedIcs20WithdrawalWitness: %w", err)
	}
	assignment, err := newShieldedIcs20WithdrawalCircuitAssignment(witness, family.NIn)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, err
	}
	return assignment, family, nil
}

func newShieldedIcs20WithdrawalRequiredSpendCircuitFields(
	witness *ShieldedIcs20WithdrawalRequiredSpendWitnessBinary,
) (circuits.ShieldedIcs20WithdrawalRequiredSpendCircuitFields, error) {
	var zero circuits.ShieldedIcs20WithdrawalRequiredSpendCircuitFields
	statePath, err := statePathFromBinary(witness.StateCommitmentAuthPath)
	if err != nil {
		return zero, fmt.Errorf("decode shielded ICS-20 withdrawal spend state commitment auth path: %w", err)
	}
	return circuits.ShieldedIcs20WithdrawalRequiredSpendCircuitFields{
		Nullifier: fqString(witness.Nullifier),
		RK:        point2DString(witness.RKAffine),
		Note: circuits.ShieldedIcs20WithdrawalNoteCircuitFields{
			Blinding:           fqString(witness.SpentNoteBlinding),
			Amount:             fqString(witness.SpentNoteAmount),
			RecoveryCommitment: fqString(witness.SpentNoteRecoveryCommitment),
		},
		StateProof: circuits.ShieldedIcs20WithdrawalStatePathCircuitFields{
			Position: witness.StateCommitmentPosition,
			Path:     statePath,
		},
		AuthRandomizer:  fqString(witness.SpendAuthRandomizer),
		HistoryRequired: boolToVariable(witness.HistoryRequired),
	}, nil
}

func newShieldedIcs20WithdrawalOptionalSpendCircuitFields(
	witness *ShieldedIcs20WithdrawalOptionalSpendWitnessBinary,
) (circuits.ShieldedIcs20WithdrawalOptionalSpendCircuitFields, error) {
	spend, err := newShieldedIcs20WithdrawalRequiredSpendCircuitFields(
		&witness.ShieldedIcs20WithdrawalRequiredSpendWitnessBinary,
	)
	if err != nil {
		return circuits.ShieldedIcs20WithdrawalOptionalSpendCircuitFields{}, err
	}
	return circuits.ShieldedIcs20WithdrawalOptionalSpendCircuitFields{
		ShieldedIcs20WithdrawalRequiredSpendCircuitFields: spend,
		IsDummy:            boolToVariable(witness.IsDummy),
		DummyNullifierSeed: fqString(witness.DummyNullifierSeed),
	}, nil
}

func newShieldedIcs20WithdrawalChangeCircuitFields(
	witness *ShieldedIcs20WithdrawalChangeWitnessBinary,
) circuits.ShieldedIcs20WithdrawalChangeCircuitFields {
	return circuits.ShieldedIcs20WithdrawalChangeCircuitFields{
		NoteCommitment: fqString(witness.NoteCommitment),
		Note: circuits.ShieldedIcs20WithdrawalNoteCircuitFields{
			Blinding:           fqString(witness.CreatedNoteBlinding),
			Amount:             fqString(witness.CreatedNoteAmount),
			RecoveryCommitment: fqString(witness.RecoveryCommitment),
		},
		Recovery: recoveryCapsuleFields(witness.RecoveryCommitment, witness.RecoveryCapsule),
	}
}

func newShieldedIcs20WithdrawalCircuitAssignment(
	witness *ShieldedIcs20WithdrawalWitnessBinary,
	expectedNIn int,
) (*circuits.ShieldedIcs20WithdrawalCircuit, error) {
	if int(witness.NIn) != expectedNIn {
		return nil, fmt.Errorf("shielded ICS-20 withdrawal witness shape mismatch: got %d inputs, expected %d", witness.NIn, expectedNIn)
	}
	reconstructedHash, err := reconstructedShieldedIcs20WithdrawalStatementHashFromWitness(witness)
	if err != nil {
		return nil, err
	}
	claimedHash := primitives.LittleEndianBytesToBigInt(witness.ClaimedStatementHash[:])
	if reconstructedHash.Cmp(claimedHash) != 0 {
		return nil, fmt.Errorf(
			"shielded ICS-20 withdrawal reconstructed statement hash mismatch: got %s, claimed %s",
			reconstructedHash,
			claimedHash,
		)
	}
	assetPath, err := quadPathFromBinary(witness.AssetPath)
	if err != nil {
		return nil, fmt.Errorf("decode shielded ICS-20 withdrawal asset path: %w", err)
	}
	senderPath, err := quadPathFromBinary(witness.SenderCompliancePath)
	if err != nil {
		return nil, fmt.Errorf("decode shielded ICS-20 withdrawal sender compliance path: %w", err)
	}
	akCompressed, err := pointAffineToField(witness.AKAffine)
	if err != nil {
		return nil, fmt.Errorf("compress shielded ICS-20 withdrawal authorization key: %w", err)
	}
	ivkReduced, quotientA, err := incomingViewingKeyReductionFromBinary(witness.NK, akCompressed)
	if err != nil {
		return nil, fmt.Errorf("compute shielded ICS-20 withdrawal ivk reduction from binary witness: %w", err)
	}

	assignment := circuits.NewShieldedIcs20WithdrawalCircuit(expectedNIn)
	assignment.ClaimedStatementHash = fqString(witness.ClaimedStatementHash)
	assignment.RoutingTag = fqString(witness.RoutingTag)
	assignment.RoutingParameterSetID = fqString(witness.RoutingParameterSetID)
	assignment.Compliance = circuits.ShieldedIcs20WithdrawalComplianceCircuitFields{
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
	assignment.Sender = circuits.ShieldedIcs20WithdrawalSenderCircuitFields{
		DivGen:        point2DString(witness.SenderDiversifiedGenerator),
		Capk:          point2DString(witness.SenderCapkAffine),
		RnkDhPk:       point2DString(witness.SenderRnkDhPkAffine),
		RnkCommitment: fqString(witness.SenderRnkCommitment),
		Status:        fqString(witness.SenderStatus),
		Path:          senderPath,
		Position:      witness.SenderCompliancePosition,
	}
	requiredSpend, err := newShieldedIcs20WithdrawalRequiredSpendCircuitFields(
		&witness.RequiredSpend,
	)
	if err != nil {
		return nil, err
	}
	optionalSpend, err := newShieldedIcs20WithdrawalOptionalSpendCircuitFields(
		&witness.OptionalSpend,
	)
	if err != nil {
		return nil, err
	}
	assignment.RequiredSpend = requiredSpend
	assignment.OptionalSpend = optionalSpend
	assignment.ChangeOutput = newShieldedIcs20WithdrawalChangeCircuitFields(&witness.ChangeOutput)
	return assignment, nil
}
