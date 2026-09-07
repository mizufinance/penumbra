package abi

import (
	"bytes"
	"fmt"

	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
)

const (
	shieldedIcs20WithdrawalWitnessMagic   = "PIWG"
	maxShieldedIcs20WithdrawalInputs      = 2
	minShieldedIcs20RequiredSpendBytes    = 32*4 + 8 + 4 + 32 + 64 + 1
	minShieldedIcs20OptionalSpendBytes    = minShieldedIcs20RequiredSpendBytes + 1 + 32
	minShieldedIcs20WithdrawalChangeBytes = 32 * 13
	minShieldedIcs20WithdrawalTailBytes   = 64 * 2
)

type ShieldedIcs20WithdrawalRequiredSpendWitnessBinary struct {
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

type ShieldedIcs20WithdrawalOptionalSpendWitnessBinary struct {
	ShieldedIcs20WithdrawalRequiredSpendWitnessBinary
	IsDummy            bool
	DummyNullifierSeed [32]byte
}

type ShieldedIcs20WithdrawalChangeWitnessBinary struct {
	NoteCommitment      [32]byte
	RecoveryCommitment  [32]byte
	CreatedNoteBlinding [32]byte
	CreatedNoteAmount   [32]byte
	RecoveryCapsule     RecoveryCapsuleWitnessBinary
}

type ShieldedIcs20WithdrawalWitnessBinary struct {
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
	WithdrawalSeed           [32]byte
	WithdrawalRandomizer     [32]byte

	RequiredSpend ShieldedIcs20WithdrawalRequiredSpendWitnessBinary
	OptionalSpend ShieldedIcs20WithdrawalOptionalSpendWitnessBinary
	ChangeOutput  ShieldedIcs20WithdrawalChangeWitnessBinary

	AKAffine                     PointAffineBinary
	AssetIndexedLeafDKPubAffine  PointAffineBinary
	AssetIndexedLeafRingPKAffine PointAffineBinary
	SenderDiversifiedGenerator   PointAffineBinary
}

func DecodeShieldedIcs20WithdrawalWitness(payload []byte) (*ShieldedIcs20WithdrawalWitnessBinary, generated.ShieldedIcs20WithdrawalFamilySpec, error) {
	reader := bytes.NewReader(payload)

	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, err
	}
	if string(magic) != shieldedIcs20WithdrawalWitnessMagic {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("invalid shielded ICS-20 withdrawal witness magic %q", string(magic))
	}
	totalLength, err := readU32(reader)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("payload length mismatch: header=%d actual=%d", totalLength, len(payload))
	}
	familyID, err := readU32(reader)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, err
	}
	family, ok := generated.ShieldedIcs20WithdrawalFamilyByID(familyID)
	if !ok {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("unknown shielded ICS-20 withdrawal family id %d", familyID)
	}
	nIn, err := readU32(reader)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, err
	}
	if int(nIn) != family.NIn {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("shielded ICS-20 withdrawal witness shape mismatch: got %d inputs, expected %d", nIn, family.NIn)
	}
	if nIn > maxShieldedIcs20WithdrawalInputs {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("shielded ICS-20 withdrawal witness nIn %d exceeds max %d", nIn, maxShieldedIcs20WithdrawalInputs)
	}
	minRemaining := minShieldedIcs20RequiredSpendBytes + minShieldedIcs20OptionalSpendBytes +
		minShieldedIcs20WithdrawalChangeBytes + minShieldedIcs20WithdrawalTailBytes
	if reader.Len() < minRemaining {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf(
			"shielded ICS-20 withdrawal witness too short for %d spends: remaining=%d min=%d",
			nIn, reader.Len(), minRemaining,
		)
	}

	out := &ShieldedIcs20WithdrawalWitnessBinary{
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
	if out.WithdrawalSeed, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.WithdrawalRandomizer, err = readFr32(reader); err != nil {
		return nil, family, err
	}
	if out.RequiredSpend, err = readShieldedIcs20WithdrawalRequiredSpend(reader); err != nil {
		return nil, family, err
	}
	if out.OptionalSpend, err = readShieldedIcs20WithdrawalOptionalSpend(reader); err != nil {
		return nil, family, err
	}
	if out.ChangeOutput, err = readShieldedIcs20WithdrawalChange(reader); err != nil {
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
		return nil, family, fmt.Errorf("trailing bytes in shielded ICS-20 withdrawal witness: %d", reader.Len())
	}
	return out, family, nil
}

func readShieldedIcs20WithdrawalRequiredSpend(reader *bytes.Reader) (ShieldedIcs20WithdrawalRequiredSpendWitnessBinary, error) {
	var out ShieldedIcs20WithdrawalRequiredSpendWitnessBinary
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

func readShieldedIcs20WithdrawalOptionalSpend(reader *bytes.Reader) (ShieldedIcs20WithdrawalOptionalSpendWitnessBinary, error) {
	required, err := readShieldedIcs20WithdrawalRequiredSpend(reader)
	if err != nil {
		return ShieldedIcs20WithdrawalOptionalSpendWitnessBinary{}, err
	}
	isDummy, err := readBool(reader)
	if err != nil {
		return ShieldedIcs20WithdrawalOptionalSpendWitnessBinary{}, err
	}
	dummyNullifierSeed, err := read32(reader)
	if err != nil {
		return ShieldedIcs20WithdrawalOptionalSpendWitnessBinary{}, err
	}
	return ShieldedIcs20WithdrawalOptionalSpendWitnessBinary{
		ShieldedIcs20WithdrawalRequiredSpendWitnessBinary: required,
		IsDummy:            isDummy,
		DummyNullifierSeed: dummyNullifierSeed,
	}, nil
}

func readShieldedIcs20WithdrawalChange(reader *bytes.Reader) (ShieldedIcs20WithdrawalChangeWitnessBinary, error) {
	var out ShieldedIcs20WithdrawalChangeWitnessBinary
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
