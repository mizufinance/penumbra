package abi

import (
	"fmt"
	"math/big"

	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

func boolField(value bool) [32]byte {
	var out [32]byte
	if value {
		out[0] = 1
	}
	return out
}

func bigIntToLE32(value *big.Int) ([32]byte, error) {
	var out [32]byte
	if value.Sign() < 0 {
		return out, fmt.Errorf("negative field value %s", value.String())
	}
	bytes := value.Bytes()
	if len(bytes) > len(out) {
		return out, fmt.Errorf("field value %s exceeds 32 bytes", value.String())
	}
	for i := range bytes {
		out[i] = bytes[len(bytes)-1-i]
	}
	return out, nil
}

func uint64ToLE32(value uint64) [32]byte {
	out, err := bigIntToLE32(new(big.Int).SetUint64(value))
	if err != nil {
		panic(err)
	}
	return out
}

func pointAffineToField(point PointAffineBinary) ([32]byte, error) {
	compressed, err := decafgnark.CompressToFieldNative(
		circuits.PointAffineToNative(pointAffineBinaryToStrings(point)),
	)
	if err != nil {
		return [32]byte{}, err
	}
	return bigIntToLE32(compressed)
}

func ensureFieldCount(label string, fields [][32]byte, expected int) error {
	if len(fields) != expected {
		return fmt.Errorf("expected %d %s statement fields, got %d", expected, label, len(fields))
	}
	return nil
}

func ReconstructedNoteSeizureStatementFieldsFromWitness(
	witness *NoteSeizureWitnessBinary,
) ([][32]byte, error) {
	epk, err := pointAffineToField(witness.Recovery.EPKAffine)
	if err != nil {
		return nil, fmt.Errorf("compress note seizure recovery EPK: %w", err)
	}
	fields := [][32]byte{
		witness.Anchor,
		witness.NoteCommitment,
		witness.Nullifier,
		boolField(witness.HistoryRequired),
		witness.RecentPositionFloor,
		witness.AddressDiversifiedGenerator,
		witness.AddressTransmissionKey,
		witness.AssetID,
		witness.Amount,
		witness.Recovery.Commitment,
		epk,
		witness.Recovery.C2,
		witness.Recovery.Salt,
		witness.Recovery.KeyConfirmation,
		witness.Recovery.EncryptedAmount,
		witness.Recovery.EncryptedNoteBlinding,
		witness.Recovery.Seed,
		witness.RnkCommitment,
		witness.AuthorizationCommitment,
	}
	if err := ensureFieldCount("note seizure", fields, primitives.NoteSeizureStatementFieldCount); err != nil {
		return nil, err
	}
	return fields, nil
}

// transferBalanceCommitmentField reconstructs the exact commitment that the
// circuit computes. Transfer does not serialize a second prover-chosen affine
// copy: the sole public statement binds this value directly.
func transferBalanceCommitmentField(
	witness *TransferWitnessBinary,
) ([32]byte, error) {
	inputs := [2]*big.Int{
		primitives.LittleEndianBytesToBigInt(witness.RequiredSpend.SpentNoteAmount[:]),
		primitives.LittleEndianBytesToBigInt(witness.OptionalSpend.SpentNoteAmount[:]),
	}
	outputs := [2]*big.Int{
		primitives.LittleEndianBytesToBigInt(witness.ReceiverOutput.CreatedNoteAmount[:]),
		primitives.LittleEndianBytesToBigInt(witness.ChangeOutput.CreatedNoteAmount[:]),
	}
	for index, amount := range append(inputs[:], outputs[:]...) {
		if amount.BitLen() > 128 {
			return [32]byte{}, fmt.Errorf(
				"transfer amount %d exceeds the 128-bit circuit range",
				index,
			)
		}
	}

	valueGenerator, err := circuits.ValueGeneratorNative(
		primitives.LittleEndianBytesToBigInt(
			witness.RequiredSpend.SpentNoteAssetID[:],
		),
	)
	if err != nil {
		return [32]byte{}, fmt.Errorf("derive transfer value generator: %w", err)
	}
	valueBlindingGenerator, err := circuits.ValueBlindingGeneratorNative()
	if err != nil {
		return [32]byte{}, fmt.Errorf(
			"load transfer value blinding generator: %w",
			err,
		)
	}
	sum, err := decafgnark.ScalarMulNative(
		valueGenerator,
		big.NewInt(0),
		128,
	)
	if err != nil {
		return [32]byte{}, fmt.Errorf(
			"initialize transfer balance commitment: %w",
			err,
		)
	}
	for index, amount := range inputs {
		amountPoint, err := decafgnark.ScalarMulNative(
			valueGenerator,
			amount,
			128,
		)
		if err != nil {
			return [32]byte{}, fmt.Errorf(
				"compute transfer input %d value point: %w",
				index,
				err,
			)
		}
		sum, err = decafgnark.PointAddNative(sum, amountPoint)
		if err != nil {
			return [32]byte{}, fmt.Errorf(
				"add transfer input %d value point: %w",
				index,
				err,
			)
		}
	}
	for index, amount := range outputs {
		amountPoint, err := decafgnark.ScalarMulNative(
			valueGenerator,
			amount,
			128,
		)
		if err != nil {
			return [32]byte{}, fmt.Errorf(
				"compute transfer output %d value point: %w",
				index,
				err,
			)
		}
		negativeAmountPoint := gnarkte.Point{
			X: new(big.Int).Mod(
				new(big.Int).Neg(amountPoint.X.(*big.Int)),
				primitives.ScalarField(),
			),
			Y: new(big.Int).Set(amountPoint.Y.(*big.Int)),
		}
		sum, err = decafgnark.PointAddNative(sum, negativeAmountPoint)
		if err != nil {
			return [32]byte{}, fmt.Errorf(
				"subtract transfer output %d value point: %w",
				index,
				err,
			)
		}
	}
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return [32]byte{}, fmt.Errorf("load transfer vectors: %w", err)
	}
	scalarBitLength := primitives.MustBigInt(
		vectors.Decaf377CompanionCurve.Order,
	).BitLen()
	blinding := primitives.LittleEndianBytesToBigInt(
		witness.ActionBalanceBlinding[:],
	)
	if blinding.BitLen() > scalarBitLength {
		return [32]byte{}, fmt.Errorf(
			"transfer balance blinding exceeds the %d-bit circuit range",
			scalarBitLength,
		)
	}
	blindingPoint, err := decafgnark.ScalarMulNative(
		valueBlindingGenerator,
		blinding,
		scalarBitLength,
	)
	if err != nil {
		return [32]byte{}, fmt.Errorf(
			"compute transfer balance blinding point: %w",
			err,
		)
	}
	sum, err = decafgnark.PointAddNative(sum, blindingPoint)
	if err != nil {
		return [32]byte{}, fmt.Errorf(
			"add transfer balance blinding point: %w",
			err,
		)
	}
	compressed, err := decafgnark.CompressToFieldNative(sum)
	if err != nil {
		return [32]byte{}, fmt.Errorf(
			"compress computed transfer balance commitment: %w",
			err,
		)
	}
	return bigIntToLE32(compressed)
}

// ReconstructedTransferStatementFieldsFromWitness mirrors the Go transfer
// circuit's statement-field order using the canonical witness records.
func ReconstructedTransferStatementFieldsFromWitness(
	witness *TransferWitnessBinary,
) ([][32]byte, error) {
	expected := expectedTransferStatementFieldCount()
	fields := make([][32]byte, 0, expected)
	fields = append(
		fields,
		witness.Anchor,
		witness.ReceiverOutput.NoteCommitment,
		witness.ReceiverOutput.RecoveryCommitment,
		witness.ChangeOutput.NoteCommitment,
		witness.ChangeOutput.RecoveryCommitment,
	)
	balanceCommitment, err := transferBalanceCommitmentField(witness)
	if err != nil {
		return nil, err
	}
	fields = append(fields, balanceCommitment)
	fields = append(fields, witness.RoutingTags[:]...)
	fields = append(fields, witness.RoutingParameterSetID)
	fields = append(fields, witness.RecentPositionFloor)
	fields = append(fields,
		witness.VolumeAccumulator.Nullifier,
		witness.VolumeAccumulator.Commitment,
		witness.VolumeAccumulator.DayStart,
		witness.VolumeAccumulator.ProofContext,
	)
	for index, spend := range []struct {
		nullifier [32]byte
		rk        PointAffineBinary
		history   bool
	}{
		{witness.RequiredSpend.Nullifier, witness.RequiredSpend.RKAffine, witness.RequiredSpend.HistoryRequired},
		{witness.OptionalSpend.Nullifier, witness.OptionalSpend.RKAffine, witness.OptionalSpend.HistoryRequired},
	} {
		fields = append(fields, spend.nullifier)
		rk, err := pointAffineToField(spend.rk)
		if err != nil {
			return nil, fmt.Errorf("compress transfer rk_%d: %w", index, err)
		}
		fields = append(fields, rk)
		fields = append(fields, boolField(spend.history))
	}
	fields = append(fields, witness.AssetAnchor, witness.ComplianceAnchor)
	if len(witness.DetectionCiphertext) != compliance.TransferDetectionFQCount {
		return nil, fmt.Errorf(
			"expected %d transfer detection ciphertext elements, got %d",
			compliance.TransferDetectionFQCount,
			len(witness.DetectionCiphertext),
		)
	}
	fields = append(fields, witness.DetectionCiphertext...)

	appendTier := func(label string, tier TransferComplianceCiphertextWitnessBinary, expectedCiphertext int) error {
		if len(tier.Ciphertext) != expectedCiphertext {
			return fmt.Errorf(
				"expected %d %s ciphertext elements, got %d",
				expectedCiphertext,
				label,
				len(tier.Ciphertext),
			)
		}
		epk, err := pointAffineToField(tier.EPKAffine)
		if err != nil {
			return fmt.Errorf("compress %s epk: %w", label, err)
		}
		fields = append(fields, epk, tier.C2)
		fields = append(fields, tier.Ciphertext...)
		return nil
	}

	for _, tier := range []struct {
		label              string
		value              TransferComplianceCiphertextWitnessBinary
		expectedCiphertext int
	}{
		{"sender_core", witness.SenderCore, compliance.TransferCoreCiphertextFQCount},
		{"sender_ext", witness.SenderExt, compliance.TransferExtCiphertextFQCount},
		{"output_core", witness.OutputCore, compliance.TransferCoreCiphertextFQCount},
		{"output_ext", witness.OutputExt, compliance.TransferExtCiphertextFQCount},
	} {
		if err := appendTier(tier.label, tier.value, tier.expectedCiphertext); err != nil {
			return nil, err
		}
	}

	fields = append(fields, witness.TargetTimestamp)
	fields = append(
		fields,
		witness.SenderCoreKeyConfirmation,
		witness.OutputCoreKeyConfirmation,
		witness.Metadata.RingIDHash,
		witness.Metadata.PolicyIDHash,
		witness.Metadata.ResourceHash,
		witness.Metadata.PermissionHash,
		witness.Metadata.SenderCoreSalt,
		witness.Metadata.SenderExtSalt,
		witness.Metadata.OutputCoreSalt,
		witness.Metadata.OutputExtSalt,
	)

	if err := ensureFieldCount("transfer", fields, expected); err != nil {
		return nil, err
	}
	return fields, nil
}

func appendNoteReshapeStatementFields(
	label string,
	fields [][32]byte,
	anchor [32]byte,
	balanceCommitmentAffine PointAffineBinary,
	assetAnchor [32]byte,
	complianceAnchor [32]byte,
	routingTag [32]byte,
	routingParameterSetID [32]byte,
	recentPositionFloor [32]byte,
	spends []NoteReshapeSpendWitnessBinary,
	outputs []NoteReshapeOutputWitnessBinary,
	expected int,
) ([][32]byte, error) {
	fields = append(fields, anchor)
	for _, output := range outputs {
		fields = append(fields, output.NoteCommitment, output.RecoveryCommitment)
	}
	balanceCommitment, err := pointAffineToField(balanceCommitmentAffine)
	if err != nil {
		return nil, fmt.Errorf("compress %s balance commitment: %w", label, err)
	}
	fields = append(fields, balanceCommitment)
	fields = append(
		fields,
		assetAnchor,
		complianceAnchor,
		routingTag,
		routingParameterSetID,
	)
	fields = append(fields, recentPositionFloor)
	for index, spend := range spends {
		fields = append(fields, spend.Nullifier)
		rk, err := pointAffineToField(spend.RKAffine)
		if err != nil {
			return nil, fmt.Errorf("compress %s rk_%d: %w", label, index, err)
		}
		fields = append(fields, rk)
		fields = append(fields, boolField(spend.IsDummy == false && spend.HistoryRequired))
	}
	if err := ensureFieldCount(label, fields, expected); err != nil {
		return nil, err
	}
	return fields, nil
}

// ReconstructedNoteReshapeStatementFieldsFromWitness mirrors the unified
// note-reshape circuit's statement-field order using decoded witness fields.
func ReconstructedNoteReshapeStatementFieldsFromWitness(
	witness *NoteReshapeWitnessBinary,
) ([][32]byte, error) {
	expected := primitives.NoteReshapeStatementFieldCount(int(witness.NIn), int(witness.NOut))
	fields, err := appendNoteReshapeStatementFields(
		"note reshape",
		make([][32]byte, 0, expected),
		witness.Anchor,
		witness.BalanceCommitmentAffine,
		witness.AssetAnchor,
		witness.ComplianceAnchor,
		witness.RoutingTag,
		witness.RoutingParameterSetID,
		witness.RecentPositionFloor,
		witness.Spends,
		witness.Outputs,
		expected,
	)
	if err != nil {
		return nil, err
	}
	if err := ensureFieldCount("note reshape", fields, expected); err != nil {
		return nil, err
	}
	return fields, nil
}

func shieldedIcs20WithdrawalBalanceCommitmentField(
	witness *ShieldedIcs20WithdrawalWitnessBinary,
) ([32]byte, error) {
	valueBlindingGenerator, err := circuits.ValueBlindingGeneratorNative()
	if err != nil {
		return [32]byte{}, fmt.Errorf("load shielded ICS-20 withdrawal value blinding generator: %w", err)
	}
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return [32]byte{}, fmt.Errorf("load shielded ICS-20 withdrawal prototype vectors: %w", err)
	}
	point, err := decafgnark.ScalarMulNative(
		valueBlindingGenerator,
		primitives.LittleEndianBytesToBigInt(witness.ActionBalanceBlinding[:]),
		primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	if err != nil {
		return [32]byte{}, fmt.Errorf("compute shielded ICS-20 withdrawal blinding-only balance commitment: %w", err)
	}
	compressed, err := decafgnark.CompressToFieldNative(point)
	if err != nil {
		return [32]byte{}, fmt.Errorf("compress shielded ICS-20 withdrawal balance commitment: %w", err)
	}
	return bigIntToLE32(compressed)
}

// ReconstructedShieldedIcs20WithdrawalStatementFieldsFromWitness mirrors the
// Go shielded ICS-20 withdrawal circuit's statement-field order using decoded
// witness fields. Internal conservation makes the balance commitment depend
// only on the action balance blinding.
func ReconstructedShieldedIcs20WithdrawalStatementFieldsFromWitness(
	witness *ShieldedIcs20WithdrawalWitnessBinary,
) ([][32]byte, error) {
	expected := primitives.ShieldedIcs20WithdrawalStatementFieldCount(int(witness.NIn))
	fields := make([][32]byte, 0, expected)
	fields = append(
		fields,
		witness.Anchor,
		witness.ChangeOutput.NoteCommitment,
		witness.ChangeOutput.RecoveryCommitment,
	)
	balanceCommitment, err := shieldedIcs20WithdrawalBalanceCommitmentField(witness)
	if err != nil {
		return nil, err
	}
	fields = append(fields, balanceCommitment)
	fields = append(fields, witness.RecentPositionFloor)
	for index, spend := range []ShieldedIcs20WithdrawalRequiredSpendWitnessBinary{
		witness.RequiredSpend,
		witness.OptionalSpend.ShieldedIcs20WithdrawalRequiredSpendWitnessBinary,
	} {
		fields = append(fields, spend.Nullifier)
		rk, err := pointAffineToField(spend.RKAffine)
		if err != nil {
			return nil, fmt.Errorf("compress shielded ICS-20 withdrawal rk_%d: %w", index, err)
		}
		fields = append(fields, rk)
		fields = append(fields, boolField(spend.HistoryRequired))
	}
	fields = append(
		fields,
		witness.AssetAnchor,
		witness.ComplianceAnchor,
		witness.TargetTimestamp,
		witness.OutboundAssetID,
		witness.OutboundAmount,
	)
	fields = append(fields, witness.WithdrawalEffectHashLimbs[:]...)
	fields = append(fields, witness.RoutingTag, witness.RoutingParameterSetID)
	fields = append(fields,
		witness.VolumeAccumulator.Nullifier,
		witness.VolumeAccumulator.Commitment,
		witness.VolumeAccumulator.DayStart,
	)
	withdrawalEPK, err := pointAffineToField(witness.WithdrawalEPKAffine)
	if err != nil {
		return nil, fmt.Errorf("compress withdrawal compliance epk: %w", err)
	}
	fields = append(
		fields,
		withdrawalEPK,
		witness.WithdrawalC2,
		witness.WithdrawalKeyConfirmation,
	)
	fields = append(fields, witness.WithdrawalEncryptedSenderAddress[:]...)
	if err := ensureFieldCount("shielded ICS-20 withdrawal", fields, expected); err != nil {
		return nil, err
	}
	return fields, nil
}

func reconstructedShieldedIcs20WithdrawalStatementHashFromWitness(
	witness *ShieldedIcs20WithdrawalWitnessBinary,
) (*big.Int, error) {
	fields, err := ReconstructedShieldedIcs20WithdrawalStatementFieldsFromWitness(witness)
	if err != nil {
		return nil, fmt.Errorf("reconstruct shielded ICS-20 withdrawal statement fields: %w", err)
	}
	nativeFields := make([]*big.Int, len(fields))
	for index := range fields {
		nativeFields[index] = primitives.LittleEndianBytesToBigInt(fields[index][:])
	}
	hash, err := primitives.ShieldedIcs20WithdrawalStatementHashNativeForShape(
		nativeFields,
		int(witness.NIn),
	)
	if err != nil {
		return nil, fmt.Errorf("hash reconstructed shielded ICS-20 withdrawal statement fields: %w", err)
	}
	return hash, nil
}
