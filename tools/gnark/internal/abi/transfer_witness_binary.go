package abi

import (
	"bytes"
	"fmt"

	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
)

const transferWitnessMagic = "PTWG"

type TransferComplianceCiphertextWitnessBinary struct {
	C2         [32]byte
	Ciphertext [][32]byte
	EPKAffine  PointAffineBinary
}

type TransferComplianceMetadataWitnessBinary struct {
	RingIDHash      [32]byte
	PolicyIDHash    [32]byte
	ResourceHash    [32]byte
	PermissionHash  [32]byte
	TargetTimestamp [32]byte
	SenderCoreSalt  [32]byte
	SenderExtSalt   [32]byte
	OutputCoreSalt  [32]byte
	OutputExtSalt   [32]byte
}

type TransferRequiredSpendWitnessBinary struct {
	Nullifier                   [32]byte
	SpentNoteBlinding           [32]byte
	SpentNoteAmount             [32]byte
	SpentNoteAssetID            [32]byte
	SpentNoteRecoveryCommitment [32]byte
	StateCommitmentPosition     uint64
	StateCommitmentAuthPath     [][3][32]byte
	SpendAuthRandomizer         [32]byte
	RKAffine                    PointAffineBinary
	HistoryRequired             bool
}

type TransferOptionalSpendWitnessBinary struct {
	Nullifier                   [32]byte
	SpentNoteBlinding           [32]byte
	SpentNoteAmount             [32]byte
	SpentNoteRecoveryCommitment [32]byte
	StateCommitmentPosition     uint64
	StateCommitmentAuthPath     [][3][32]byte
	SpendAuthRandomizer         [32]byte
	RKAffine                    PointAffineBinary
	IsDummy                     bool
	DummyNullifierSeed          [32]byte
	HistoryRequired             bool
}

type TransferReceiverOutputWitnessBinary struct {
	NoteCommitment                [32]byte
	RecoveryCommitment            [32]byte
	CreatedNoteBlinding           [32]byte
	CreatedNoteAmount             [32]byte
	RecoveryCapsule               RecoveryCapsuleWitnessBinary
	RecipientCompliancePath       MerklePathBinary
	RecipientCompliancePosition   uint64
	RecipientCapkAffine           PointAffineBinary
	RecipientRnkDhPkAffine        PointAffineBinary
	RecipientRnkCommitment        [32]byte
	RecipientStatus               [32]byte
	RecipientDiversifiedGenerator PointAffineBinary
	RecipientTransmissionKey      PointAffineBinary
}

type TransferChangeOutputWitnessBinary struct {
	NoteCommitment      [32]byte
	RecoveryCommitment  [32]byte
	CreatedNoteBlinding [32]byte
	CreatedNoteAmount   [32]byte
	RecoveryCapsule     RecoveryCapsuleWitnessBinary
}

type TransferVolumeAccumulatorWitnessBinary struct {
	Nullifier         [32]byte
	Commitment        [32]byte
	DayStart          [32]byte
	ProofContext      [32]byte
	UseReal           bool
	StartsNewDay      bool
	Subject           [32]byte
	PriorVolume       [32]byte
	PriorBlinding     [32]byte
	PriorCommitment   [32]byte
	PriorPosition     uint64
	PriorAuthPath     [][3][32]byte
	SuccessorVolume   [32]byte
	SuccessorBlinding [32]byte
}

type TransferWitnessBinary struct {
	TotalLength uint32

	Anchor                [32]byte
	AssetAnchor           [32]byte
	ComplianceAnchor      [32]byte
	TargetTimestamp       [32]byte
	ClaimedStatementHash  [32]byte
	RoutingTags           [2][32]byte
	RoutingParameterSetID [32]byte
	RecentPositionFloor   [32]byte
	VolumeAccumulator     TransferVolumeAccumulatorWitnessBinary

	ActionBalanceBlinding    [32]byte
	NK                       [32]byte
	AssetPath                MerklePathBinary
	AssetPosition            uint64
	AssetIndexedLeaf         IndexedLeafBinary
	IsRegulated              bool
	RegulatedPrecision       uint8
	UnregulatedPrecision     uint8
	RoutingAsOfHeight        uint64
	SenderCompliancePath     MerklePathBinary
	SenderCompliancePosition uint64
	SenderCapkAffine         PointAffineBinary
	SenderRnkDhPkAffine      PointAffineBinary
	SenderRnkCommitment      [32]byte
	SenderStatus             [32]byte
	TransferNonceRoot        [32]byte

	DetectionCiphertext       [][32]byte
	SenderCoreKeyConfirmation [32]byte
	OutputCoreKeyConfirmation [32]byte
	Metadata                  TransferComplianceMetadataWitnessBinary
	SenderCore                TransferComplianceCiphertextWitnessBinary
	SenderExt                 TransferComplianceCiphertextWitnessBinary
	OutputCore                TransferComplianceCiphertextWitnessBinary
	OutputExt                 TransferComplianceCiphertextWitnessBinary
	SenderRCore               [32]byte
	SenderRExt                [32]byte
	OutputRCore               [32]byte
	OutputRExt                [32]byte

	RequiredSpend  TransferRequiredSpendWitnessBinary
	OptionalSpend  TransferOptionalSpendWitnessBinary
	ReceiverOutput TransferReceiverOutputWitnessBinary
	ChangeOutput   TransferChangeOutputWitnessBinary

	AKAffine                   PointAffineBinary
	AssetIndexedLeafDKPub      PointAffineBinary
	AssetIndexedLeafRingPK     PointAffineBinary
	SenderDiversifiedGenerator PointAffineBinary
	SenderTransmissionKey      PointAffineBinary
}

func DecodeTransferWitness(
	payload []byte,
) (*TransferWitnessBinary, generated.TransferFamilySpec, error) {
	family, ok := generated.TransferFamilyByLabel("transfer")
	if !ok {
		return nil, generated.TransferFamilySpec{}, fmt.Errorf("missing generated transfer spec")
	}
	witness, err := decodeTransferWitness("Transfer", payload)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	return witness, family, nil
}

func decodeTransferWitness(
	label string,
	payload []byte,
) (*TransferWitnessBinary, error) {
	reader := bytes.NewReader(payload)

	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, err
	}
	if string(magic) != transferWitnessMagic {
		return nil, fmt.Errorf("invalid %sWitness magic %q", label, string(magic))
	}
	totalLength, err := readU32(reader)
	if err != nil {
		return nil, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, fmt.Errorf("payload length mismatch: header=%d actual=%d", totalLength, len(payload))
	}

	witness := &TransferWitnessBinary{TotalLength: totalLength}
	if witness.Anchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.AssetAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ComplianceAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.TargetTimestamp, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ClaimedStatementHash, err = read32(reader); err != nil {
		return nil, err
	}
	for i := range witness.RoutingTags {
		if witness.RoutingTags[i], err = read32(reader); err != nil {
			return nil, err
		}
	}
	if witness.RoutingParameterSetID, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.RecentPositionFloor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.VolumeAccumulator, err = readTransferVolumeAccumulator(reader); err != nil {
		return nil, fmt.Errorf("decode transfer volume accumulator: %w", err)
	}
	if witness.ActionBalanceBlinding, err = readFr32(reader); err != nil {
		return nil, err
	}
	if witness.NK, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.AssetPath, err = readMerklePath(reader); err != nil {
		return nil, err
	}
	if witness.AssetPosition, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.AssetIndexedLeaf, err = readIndexedLeaf(reader); err != nil {
		return nil, err
	}
	if witness.IsRegulated, err = readBool(reader); err != nil {
		return nil, err
	}
	if witness.RegulatedPrecision, err = readU8(reader); err != nil {
		return nil, err
	}
	if witness.UnregulatedPrecision, err = readU8(reader); err != nil {
		return nil, err
	}
	if witness.RoutingAsOfHeight, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.SenderCompliancePath, err = readMerklePath(reader); err != nil {
		return nil, err
	}
	if witness.SenderCompliancePosition, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.SenderCapkAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.SenderRnkDhPkAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.SenderRnkCommitment, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.SenderStatus, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.TransferNonceRoot, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DetectionCiphertext, err = readVec32(reader); err != nil {
		return nil, err
	}
	if witness.SenderCoreKeyConfirmation, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.OutputCoreKeyConfirmation, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Metadata, err = readTransferComplianceMetadata(reader); err != nil {
		return nil, err
	}
	if witness.SenderCore, err = readTransferComplianceTier(reader); err != nil {
		return nil, err
	}
	if witness.SenderExt, err = readTransferComplianceTier(reader); err != nil {
		return nil, err
	}
	if witness.OutputCore, err = readTransferComplianceTier(reader); err != nil {
		return nil, err
	}
	if witness.OutputExt, err = readTransferComplianceTier(reader); err != nil {
		return nil, err
	}
	if witness.SenderRCore, err = readFr32(reader); err != nil {
		return nil, err
	}
	if witness.SenderRExt, err = readFr32(reader); err != nil {
		return nil, err
	}
	if witness.OutputRCore, err = readFr32(reader); err != nil {
		return nil, err
	}
	if witness.OutputRExt, err = readFr32(reader); err != nil {
		return nil, err
	}

	if witness.RequiredSpend, err = readTransferRequiredSpend(reader); err != nil {
		return nil, fmt.Errorf("decode required transfer spend: %w", err)
	}
	if witness.OptionalSpend, err = readTransferOptionalSpend(reader); err != nil {
		return nil, fmt.Errorf("decode optional transfer spend: %w", err)
	}
	if witness.ReceiverOutput, err = readTransferReceiverOutput(reader); err != nil {
		return nil, fmt.Errorf("decode transfer receiver output: %w", err)
	}
	if witness.ChangeOutput, err = readTransferChangeOutput(reader); err != nil {
		return nil, fmt.Errorf("decode transfer change output: %w", err)
	}

	if witness.AKAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.AssetIndexedLeafDKPub, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.AssetIndexedLeafRingPK, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.SenderDiversifiedGenerator, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.SenderTransmissionKey, err = readPointAffine(reader); err != nil {
		return nil, err
	}

	if extra, err := readExact(reader, int(reader.Len())); err != nil {
		return nil, err
	} else if len(extra) != 0 {
		return nil, fmt.Errorf("%s witness has %d trailing bytes", label, len(extra))
	}

	return witness, nil
}

func readTransferVolumeAccumulator(
	reader *bytes.Reader,
) (TransferVolumeAccumulatorWitnessBinary, error) {
	var value TransferVolumeAccumulatorWitnessBinary
	var err error
	if value.Nullifier, err = read32(reader); err != nil {
		return value, err
	}
	if value.Commitment, err = read32(reader); err != nil {
		return value, err
	}
	if value.DayStart, err = read32(reader); err != nil {
		return value, err
	}
	if value.ProofContext, err = read32(reader); err != nil {
		return value, err
	}
	if value.UseReal, err = readBool(reader); err != nil {
		return value, err
	}
	if value.StartsNewDay, err = readBool(reader); err != nil {
		return value, err
	}
	if value.Subject, err = read32(reader); err != nil {
		return value, err
	}
	if value.PriorVolume, err = read32(reader); err != nil {
		return value, err
	}
	if value.PriorBlinding, err = read32(reader); err != nil {
		return value, err
	}
	if value.PriorCommitment, err = read32(reader); err != nil {
		return value, err
	}
	if value.PriorPosition, err = readU64(reader); err != nil {
		return value, err
	}
	if value.PriorAuthPath, err = readTriplePath(reader); err != nil {
		return value, err
	}
	if value.SuccessorVolume, err = read32(reader); err != nil {
		return value, err
	}
	if value.SuccessorBlinding, err = read32(reader); err != nil {
		return value, err
	}
	return value, nil
}

func readTransferRequiredSpend(
	reader *bytes.Reader,
) (TransferRequiredSpendWitnessBinary, error) {
	var spend TransferRequiredSpendWitnessBinary
	var err error
	if spend.Nullifier, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.SpentNoteBlinding, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.SpentNoteAmount, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.SpentNoteAssetID, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.SpentNoteRecoveryCommitment, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.StateCommitmentPosition, err = readU64(reader); err != nil {
		return spend, err
	}
	if spend.StateCommitmentAuthPath, err = readTriplePath(reader); err != nil {
		return spend, err
	}
	if spend.SpendAuthRandomizer, err = readFr32(reader); err != nil {
		return spend, err
	}
	if spend.RKAffine, err = readPointAffine(reader); err != nil {
		return spend, err
	}
	if spend.HistoryRequired, err = readBool(reader); err != nil {
		return spend, err
	}
	return spend, nil
}

func readTransferOptionalSpend(
	reader *bytes.Reader,
) (TransferOptionalSpendWitnessBinary, error) {
	var spend TransferOptionalSpendWitnessBinary
	var err error
	if spend.Nullifier, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.SpentNoteBlinding, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.SpentNoteAmount, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.SpentNoteRecoveryCommitment, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.StateCommitmentPosition, err = readU64(reader); err != nil {
		return spend, err
	}
	if spend.StateCommitmentAuthPath, err = readTriplePath(reader); err != nil {
		return spend, err
	}
	if spend.SpendAuthRandomizer, err = readFr32(reader); err != nil {
		return spend, err
	}
	if spend.RKAffine, err = readPointAffine(reader); err != nil {
		return spend, err
	}
	if spend.IsDummy, err = readBool(reader); err != nil {
		return spend, err
	}
	if spend.DummyNullifierSeed, err = read32(reader); err != nil {
		return spend, err
	}
	if spend.HistoryRequired, err = readBool(reader); err != nil {
		return spend, err
	}
	return spend, nil
}

func readTransferReceiverOutput(
	reader *bytes.Reader,
) (TransferReceiverOutputWitnessBinary, error) {
	var output TransferReceiverOutputWitnessBinary
	var err error
	if output.NoteCommitment, err = read32(reader); err != nil {
		return output, err
	}
	if output.RecoveryCommitment, err = read32(reader); err != nil {
		return output, err
	}
	if output.CreatedNoteBlinding, err = read32(reader); err != nil {
		return output, err
	}
	if output.CreatedNoteAmount, err = read32(reader); err != nil {
		return output, err
	}
	if output.RecoveryCapsule, err = readRecoveryCapsule(reader); err != nil {
		return output, err
	}
	if output.RecipientCompliancePath, err = readMerklePath(reader); err != nil {
		return output, err
	}
	if output.RecipientCompliancePosition, err = readU64(reader); err != nil {
		return output, err
	}
	if output.RecipientCapkAffine, err = readPointAffine(reader); err != nil {
		return output, err
	}
	if output.RecipientRnkDhPkAffine, err = readPointAffine(reader); err != nil {
		return output, err
	}
	if output.RecipientRnkCommitment, err = read32(reader); err != nil {
		return output, err
	}
	if output.RecipientStatus, err = read32(reader); err != nil {
		return output, err
	}
	if output.RecipientDiversifiedGenerator, err = readPointAffine(reader); err != nil {
		return output, err
	}
	if output.RecipientTransmissionKey, err = readPointAffine(reader); err != nil {
		return output, err
	}
	return output, nil
}

func readTransferChangeOutput(
	reader *bytes.Reader,
) (TransferChangeOutputWitnessBinary, error) {
	var output TransferChangeOutputWitnessBinary
	var err error
	if output.NoteCommitment, err = read32(reader); err != nil {
		return output, err
	}
	if output.RecoveryCommitment, err = read32(reader); err != nil {
		return output, err
	}
	if output.CreatedNoteBlinding, err = read32(reader); err != nil {
		return output, err
	}
	if output.CreatedNoteAmount, err = read32(reader); err != nil {
		return output, err
	}
	if output.RecoveryCapsule, err = readRecoveryCapsule(reader); err != nil {
		return output, err
	}
	return output, nil
}

func readTransferComplianceTier(
	reader *bytes.Reader,
) (TransferComplianceCiphertextWitnessBinary, error) {
	var tier TransferComplianceCiphertextWitnessBinary
	var err error
	if tier.C2, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.Ciphertext, err = readVec32(reader); err != nil {
		return tier, err
	}
	if tier.EPKAffine, err = readPointAffine(reader); err != nil {
		return tier, err
	}
	return tier, nil
}

func readTransferComplianceMetadata(
	reader *bytes.Reader,
) (TransferComplianceMetadataWitnessBinary, error) {
	var metadata TransferComplianceMetadataWitnessBinary
	var err error
	if metadata.RingIDHash, err = read32(reader); err != nil {
		return metadata, err
	}
	if metadata.PolicyIDHash, err = read32(reader); err != nil {
		return metadata, err
	}
	if metadata.ResourceHash, err = read32(reader); err != nil {
		return metadata, err
	}
	if metadata.PermissionHash, err = read32(reader); err != nil {
		return metadata, err
	}
	if metadata.TargetTimestamp, err = read32(reader); err != nil {
		return metadata, err
	}
	if metadata.SenderCoreSalt, err = read32(reader); err != nil {
		return metadata, err
	}
	if metadata.SenderExtSalt, err = read32(reader); err != nil {
		return metadata, err
	}
	if metadata.OutputCoreSalt, err = read32(reader); err != nil {
		return metadata, err
	}
	if metadata.OutputExtSalt, err = read32(reader); err != nil {
		return metadata, err
	}
	return metadata, nil
}
