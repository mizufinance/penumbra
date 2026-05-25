package abi

import (
	"bytes"
	"fmt"

	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
)

const (
	shieldedIcs20WithdrawalWitnessV1Magic = "PIWG"
	shieldedIcs20WithdrawalWitnessVersion = 2
	maxShieldedIcs20WithdrawalInputs      = 2
	minShieldedIcs20WithdrawalSpendBytes  = 32*7 + 8 + 4 + 32 + 1 + 32 + 32 + 64*3
	minShieldedIcs20WithdrawalChangeBytes = 32*6 + 64*2
	minShieldedIcs20WithdrawalTailBytes   = 64 * 6
)

type ShieldedIcs20WithdrawalSpendWitnessV1Binary struct {
	Nullifier                 [32]byte
	SpentNoteBlinding         [32]byte
	SpentNoteAmount           [32]byte
	SpentNoteAssetID          [32]byte
	SpentTransmissionKey      [32]byte
	SpentClueKey              [32]byte
	StateCommitmentCommitment [32]byte
	StateCommitmentPosition   uint64
	StateCommitmentAuthPath   [][3][32]byte
	SpendAuthRandomizer       [32]byte
	IsDummy                   bool
	DummyNullifierSeed        [32]byte
	DummySpendAuthKey         [32]byte
	RKAffine                  PointAffineBinary
	SpentDivGenAffine         PointAffineBinary
	SpentTransmissionAffine   PointAffineBinary
}

type ShieldedIcs20WithdrawalChangeWitnessV1Binary struct {
	NoteCommitment            [32]byte
	CreatedNoteBlinding       [32]byte
	CreatedNoteAmount         [32]byte
	CreatedNoteAssetID        [32]byte
	CreatedTransmissionKey    [32]byte
	CreatedClueKey            [32]byte
	CreatedDivGenAffine       PointAffineBinary
	CreatedTransmissionAffine PointAffineBinary
}

type ShieldedIcs20WithdrawalWitnessV1Binary struct {
	TotalLength uint32
	FamilyID    uint32
	NIn         uint32

	Anchor                 [32]byte
	BalanceCommitment      [32]byte
	AssetAnchor            [32]byte
	ComplianceAnchor       [32]byte
	TargetTimestamp        [32]byte
	OutboundAssetID        [32]byte
	OutboundAmount         [32]byte
	WithdrawalEffectHashLo [32]byte
	WithdrawalEffectHashHi [32]byte
	ClaimedStatementHash   [32]byte
	StatementFields        [][32]byte
	ActionBalanceBlinding  [32]byte
	AK                     [32]byte
	NK                     [32]byte

	AssetPath                MerklePathBinary
	AssetPosition            uint64
	AssetIndexedLeaf         IndexedLeafBinary
	IsRegulated              bool
	SenderCompliancePath     MerklePathBinary
	SenderCompliancePosition uint64
	SenderAssetID            [32]byte
	SenderSlotID             [32]byte
	SenderSlotDerivation     [32]byte
	SenderD                  [32]byte

	Spends       []ShieldedIcs20WithdrawalSpendWitnessV1Binary
	ChangeOutput ShieldedIcs20WithdrawalChangeWitnessV1Binary

	BalanceCommitmentAffine    PointAffineBinary
	AKAffine                   PointAffineBinary
	AssetIndexedLeafDKPub      PointAffineBinary
	AssetIndexedLeafRingPK     PointAffineBinary
	SenderDiversifiedGenerator PointAffineBinary
	SenderTransmissionKey      PointAffineBinary
}

func DecodeShieldedIcs20WithdrawalWitnessV1(payload []byte) (*ShieldedIcs20WithdrawalWitnessV1Binary, generated.ShieldedIcs20WithdrawalFamilySpec, error) {
	reader := bytes.NewReader(payload)

	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, err
	}
	if string(magic) != shieldedIcs20WithdrawalWitnessV1Magic {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("invalid shielded ICS-20 withdrawal witness magic %q", string(magic))
	}
	version, err := readU32(reader)
	if err != nil {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, err
	}
	if version != shieldedIcs20WithdrawalWitnessVersion {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("unsupported shielded ICS-20 withdrawal witness version %d", version)
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
	minRemaining := int(nIn)*minShieldedIcs20WithdrawalSpendBytes + minShieldedIcs20WithdrawalChangeBytes + minShieldedIcs20WithdrawalTailBytes
	if reader.Len() < minRemaining {
		return nil, generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf(
			"shielded ICS-20 withdrawal witness too short for %d spends: remaining=%d min=%d",
			nIn, reader.Len(), minRemaining,
		)
	}

	out := &ShieldedIcs20WithdrawalWitnessV1Binary{
		TotalLength: totalLength,
		FamilyID:    familyID,
		NIn:         nIn,
	}
	if out.Anchor, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.BalanceCommitment, err = read32(reader); err != nil {
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
	if out.WithdrawalEffectHashLo, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.WithdrawalEffectHashHi, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.ClaimedStatementHash, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.StatementFields, err = readVec32(reader); err != nil {
		return nil, family, err
	}
	if out.ActionBalanceBlinding, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.AK, err = read32(reader); err != nil {
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
	if out.SenderCompliancePath, err = readMerklePath(reader); err != nil {
		return nil, family, err
	}
	if out.SenderCompliancePosition, err = readU64(reader); err != nil {
		return nil, family, err
	}
	if out.SenderAssetID, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.SenderSlotID, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.SenderSlotDerivation, err = read32(reader); err != nil {
		return nil, family, err
	}
	if out.SenderD, err = read32(reader); err != nil {
		return nil, family, err
	}
	out.Spends = make([]ShieldedIcs20WithdrawalSpendWitnessV1Binary, nIn)
	for i := range out.Spends {
		if out.Spends[i], err = readShieldedIcs20WithdrawalSpend(reader); err != nil {
			return nil, family, err
		}
	}
	if out.ChangeOutput, err = readShieldedIcs20WithdrawalChange(reader); err != nil {
		return nil, family, err
	}
	if out.BalanceCommitmentAffine, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.AKAffine, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.AssetIndexedLeafDKPub, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.AssetIndexedLeafRingPK, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.SenderDiversifiedGenerator, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if out.SenderTransmissionKey, err = readPointAffine(reader); err != nil {
		return nil, family, err
	}
	if reader.Len() != 0 {
		return nil, family, fmt.Errorf("trailing bytes in shielded ICS-20 withdrawal witness: %d", reader.Len())
	}
	return out, family, nil
}

func readShieldedIcs20WithdrawalSpend(reader *bytes.Reader) (ShieldedIcs20WithdrawalSpendWitnessV1Binary, error) {
	var out ShieldedIcs20WithdrawalSpendWitnessV1Binary
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
	if out.SpentNoteAssetID, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentTransmissionKey, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentClueKey, err = read32(reader); err != nil {
		return out, err
	}
	if out.StateCommitmentCommitment, err = read32(reader); err != nil {
		return out, err
	}
	if out.StateCommitmentPosition, err = readU64(reader); err != nil {
		return out, err
	}
	if out.StateCommitmentAuthPath, err = readTriplePath(reader); err != nil {
		return out, err
	}
	if out.SpendAuthRandomizer, err = read32(reader); err != nil {
		return out, err
	}
	isDummy, err := readBool(reader)
	if err != nil {
		return out, err
	}
	out.IsDummy = isDummy
	if out.DummyNullifierSeed, err = read32(reader); err != nil {
		return out, err
	}
	if out.DummySpendAuthKey, err = read32(reader); err != nil {
		return out, err
	}
	if out.RKAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	if out.SpentDivGenAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	if out.SpentTransmissionAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	return out, nil
}

func readShieldedIcs20WithdrawalChange(reader *bytes.Reader) (ShieldedIcs20WithdrawalChangeWitnessV1Binary, error) {
	var out ShieldedIcs20WithdrawalChangeWitnessV1Binary
	var err error
	if out.NoteCommitment, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteBlinding, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteAmount, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteAssetID, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedTransmissionKey, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedClueKey, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedDivGenAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	if out.CreatedTransmissionAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	return out, nil
}
