package abi

import (
	"bytes"
	"fmt"

	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
)

const (
	shieldedWithdrawalWitnessMagic          = "PIW2"
	maxShieldedWithdrawalInputs             = 2
	minShieldedWithdrawalRequiredSpendBytes = 32*4 + 8 + 4 + 32 + 64 + 1
	minShieldedWithdrawalOptionalSpendBytes = minShieldedWithdrawalRequiredSpendBytes + 1 + 32
	minShieldedWithdrawalChangeBytes        = 32 * 13
	minShieldedWithdrawalTailBytes          = 64 * 2
)

type ShieldedWithdrawalRequiredSpendWitnessBinary struct {
	Nullifier                   [32]byte
	SpentNoteBlinding           [32]byte
	SpentNoteAmount             [32]byte
	SpentNoteRecoveryCommitment [32]byte
	StateCommitmentPosition     uint64
	StateCommitmentAuthPath     [][3][32]byte
	SpendAuthRandomizer         [32]byte
	RKAffine                    PointAffineBinary
	HistoryRequired             bool
}

type ShieldedWithdrawalOptionalSpendWitnessBinary struct {
	ShieldedWithdrawalRequiredSpendWitnessBinary
	IsDummy            bool
	DummyNullifierSeed [32]byte
}

type ShieldedWithdrawalChangeWitnessBinary struct {
	NoteCommitment      [32]byte
	RecoveryCommitment  [32]byte
	CreatedNoteBlinding [32]byte
	CreatedNoteAmount   [32]byte
	RecoveryCapsule     RecoveryCapsuleWitnessBinary
}

type ShieldedWithdrawalWitnessBinary struct {
	TotalLength uint32
	FamilyID    uint32
	NIn         uint32

	Anchor                           [32]byte
	AssetAnchor                      [32]byte
	ComplianceAnchor                 [32]byte
	TargetTimestamp                  [32]byte
	OutboundAssetID                  [32]byte
	OutboundAmount                   [32]byte
	WithdrawalEffectHashLimbs        [4][32]byte
	ClaimedStatementHash             [32]byte
	RoutingTag                       [32]byte
	RoutingParameterSetID            [32]byte
	WithdrawalEPKAffine              PointAffineBinary
	WithdrawalC2                     [32]byte
	WithdrawalKeyConfirmation        [32]byte
	WithdrawalEncryptedSenderAddress [3][32]byte
	RecentPositionFloor              [32]byte
	ActionBalanceBlinding            [32]byte
	NK                               [32]byte
	VolumeAccumulator                TransferVolumeAccumulatorWitnessBinary
	VolumeAccumulatorSeed            [32]byte

	AssetPath                MerklePathBinary
	AssetPosition            uint64
	AssetIndexedLeaf         IndexedLeafBinary
	IsRegulated              bool
	RegulatedPrecision       uint8
	UnregulatedPrecision     uint8
	RoutingAsOfHeight        uint64
	RoutingNonce             [32]byte
	SenderCompliancePath     MerklePathBinary
	SenderCompliancePosition uint64
	SenderCapkAffine         PointAffineBinary
	SenderRnkDhPkAffine      PointAffineBinary
	SenderRnkCommitment      [32]byte
	SenderStatus             [32]byte
	SenderAuditKeys          AuditKeysBinary
	WithdrawalSeed           [32]byte
	WithdrawalRandomizer     [32]byte

	RequiredSpend ShieldedWithdrawalRequiredSpendWitnessBinary
	OptionalSpend ShieldedWithdrawalOptionalSpendWitnessBinary
	ChangeOutput  ShieldedWithdrawalChangeWitnessBinary

	AKAffine                     PointAffineBinary
	AssetIndexedLeafDKPubAffine  PointAffineBinary
	AssetIndexedLeafRingPKAffine PointAffineBinary
	SenderDiversifiedGenerator   PointAffineBinary
}

func DecodeShieldedWithdrawalWitness(payload []byte) (*ShieldedWithdrawalWitnessBinary, generated.ShieldedWithdrawalFamilySpec, error) {
	reader := bytes.NewReader(payload)

	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, err
	}
	if string(magic) != shieldedWithdrawalWitnessMagic {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, fmt.Errorf("invalid shielded withdrawal witness magic %q", string(magic))
	}
	totalLength, err := readU32(reader)
	if err != nil {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, fmt.Errorf("payload length mismatch: header=%d actual=%d", totalLength, len(payload))
	}
	familyID, err := readU32(reader)
	if err != nil {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, err
	}
	family, ok := generated.ShieldedWithdrawalFamilyByID(familyID)
	if !ok {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, fmt.Errorf("unknown shielded withdrawal family id %d", familyID)
	}
	nIn, err := readU32(reader)
	if err != nil {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, err
	}
	if int(nIn) != family.NIn {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, fmt.Errorf("shielded withdrawal witness shape mismatch: got %d inputs, expected %d", nIn, family.NIn)
	}
	if nIn > maxShieldedWithdrawalInputs {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, fmt.Errorf("shielded withdrawal witness nIn %d exceeds max %d", nIn, maxShieldedWithdrawalInputs)
	}
	minRemaining := minShieldedWithdrawalRequiredSpendBytes + minShieldedWithdrawalOptionalSpendBytes +
		minShieldedWithdrawalChangeBytes + minShieldedWithdrawalTailBytes
	if reader.Len() < minRemaining {
		return nil, generated.ShieldedWithdrawalFamilySpec{}, fmt.Errorf(
			"shielded withdrawal witness too short for %d spends: remaining=%d min=%d",
			nIn, reader.Len(), minRemaining,
		)
	}

	out := &ShieldedWithdrawalWitnessBinary{
		TotalLength: totalLength,
		FamilyID:    familyID,
		NIn:         nIn,
	}
	if out.Anchor, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.AssetAnchor, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.ComplianceAnchor, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.TargetTimestamp, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.OutboundAssetID, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.OutboundAmount, err = read32(reader); err != nil {
		return nil, family, err
	}
	for index := range out.WithdrawalEffectHashLimbs {
		if out.WithdrawalEffectHashLimbs[index], err = readU128Field(reader); err != nil {
			return nil, family, err
		}
	}
	if out.ClaimedStatementHash, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.RoutingTag, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.RoutingParameterSetID, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.WithdrawalEPKAffine, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.WithdrawalC2, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.WithdrawalKeyConfirmation, err = read32(reader); err != nil {
		return nil, family, err
	}
	for index := range out.WithdrawalEncryptedSenderAddress {
		if out.WithdrawalEncryptedSenderAddress[index], err = read32(reader); err != nil {
			return nil, family, err
		}
	}
	if out.RecentPositionFloor, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.VolumeAccumulator, err = readTransferVolumeAccumulator(reader); err != nil {
		return nil, family, err
	}
	if out.VolumeAccumulatorSeed, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.ActionBalanceBlinding, err = readFr32(reader); err != nil {
		return nil, family, err
	}
	if out.NK, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.AssetPath, err = readMerklePath(reader); err != nil {
		return nil, family, err
	}
	if out.AssetPosition, err = readU64(reader); err != nil {
		return nil, family, err
	}
	if out.AssetIndexedLeaf, err = readIndexedLeaf(reader); err != nil {
		return nil, family, err
	}
	isRegulated, err := readBool(reader)
	if err != nil {
		return nil, family, err
	}
	out.IsRegulated = isRegulated
	if out.RegulatedPrecision, err = readU8(reader); err != nil {
		return nil, family, err
	}
	if out.UnregulatedPrecision, err = readU8(reader); err != nil {
		return nil, family, err
	}
	if out.RoutingAsOfHeight, err = readU64(reader); err != nil {
		return nil, family, err
	}
	if out.RoutingNonce, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.SenderCompliancePath, err = readMerklePath(reader); err != nil {
		return nil, family, err
	}
	if out.SenderCompliancePosition, err = readU64(reader); err != nil {
		return nil, family, err
	}
	if out.SenderCapkAffine, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.SenderRnkDhPkAffine, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.SenderRnkCommitment, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.SenderStatus, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.SenderAuditKeys, err = readAuditKeys(reader); err != nil {
		return nil, family, err
	}
	if out.WithdrawalSeed, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.WithdrawalRandomizer, err = readFr32(reader); err != nil {
		return nil, family, err
	}
	if out.RequiredSpend, err = readShieldedWithdrawalRequiredSpend(reader); err != nil {
		return nil, family, err
	}
	if out.OptionalSpend, err = readShieldedWithdrawalOptionalSpend(reader); err != nil {
		return nil, family, err
	}
	if out.ChangeOutput, err = readShieldedWithdrawalChange(reader); err != nil {
		return nil, family, err
	}
	if out.AKAffine, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.AssetIndexedLeafDKPubAffine, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.AssetIndexedLeafRingPKAffine, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.SenderDiversifiedGenerator, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if reader.Len() != 0 {
		return nil, family, fmt.Errorf("trailing bytes in shielded withdrawal witness: %d", reader.Len())
	}
	return out, family, nil
}

func readShieldedWithdrawalRequiredSpend(reader *bytes.Reader) (ShieldedWithdrawalRequiredSpendWitnessBinary, error) {
	var out ShieldedWithdrawalRequiredSpendWitnessBinary
	var err error
	if out.Nullifier, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentNoteBlinding, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentNoteAmount, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentNoteRecoveryCommitment, err = read32(reader); err != nil {
		return out, err
	}
	if out.StateCommitmentPosition, err = readU64(reader); err != nil {
		return out, err
	}
	if out.StateCommitmentAuthPath, err = readTriplePath(reader); err != nil {
		return out, err
	}
	if out.SpendAuthRandomizer, err = readFr32(reader); err != nil {
		return out, err
	}
	if out.RKAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	if out.HistoryRequired, err = readBool(reader); err != nil {
		return out, err
	}
	return out, nil
}

func readShieldedWithdrawalOptionalSpend(reader *bytes.Reader) (ShieldedWithdrawalOptionalSpendWitnessBinary, error) {
	required, err := readShieldedWithdrawalRequiredSpend(reader)
	if err != nil {
		return ShieldedWithdrawalOptionalSpendWitnessBinary{}, err
	}
	isDummy, err := readBool(reader)
	if err != nil {
		return ShieldedWithdrawalOptionalSpendWitnessBinary{}, err
	}
	dummyNullifierSeed, err := read32(reader)
	if err != nil {
		return ShieldedWithdrawalOptionalSpendWitnessBinary{}, err
	}
	return ShieldedWithdrawalOptionalSpendWitnessBinary{
		ShieldedWithdrawalRequiredSpendWitnessBinary: required,
		IsDummy:            isDummy,
		DummyNullifierSeed: dummyNullifierSeed,
	}, nil
}

func readShieldedWithdrawalChange(reader *bytes.Reader) (ShieldedWithdrawalChangeWitnessBinary, error) {
	var out ShieldedWithdrawalChangeWitnessBinary
	var err error
	if out.NoteCommitment, err = read32(reader); err != nil {
		return out, err
	}
	if out.RecoveryCommitment, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteBlinding, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteAmount, err = read32(reader); err != nil {
		return out, err
	}
	if out.RecoveryCapsule, err = readRecoveryCapsule(reader); err != nil {
		return out, err
	}
	return out, nil
}
