package abi

import (
	"bytes"
	"fmt"
)

const (
	outputWitnessV1Magic   = "POWG"
	outputWitnessV1Version = 1
)

type OutputWitnessV1Binary struct {
	TotalLength uint32

	NoteCommitment       [32]byte
	BalanceCommitment    [32]byte
	Epk1                 [32]byte
	Epk2                 [32]byte
	Epk3                 [32]byte
	C2Core               [32]byte
	C2Ext                [32]byte
	C2Sext               [32]byte
	ComplianceCiphertext [][32]byte
	TargetTimestamp      [32]byte
	DleqC1               [32]byte
	DleqS1               [32]byte
	DleqC2               [32]byte
	DleqS2               [32]byte
	DleqC3               [32]byte
	DleqS3               [32]byte
	AssetAnchor          [32]byte
	ComplianceAnchor     [32]byte
	CounterpartyLeafHash [32]byte
	ClaimedStatementHash [32]byte
	StatementFields      [][32]byte

	NoteBlinding                     [32]byte
	NoteAmount                       [32]byte
	NoteAssetID                      [32]byte
	DiversifiedGenerator             [32]byte
	TransmissionKey                  [32]byte
	ClueKey                          [32]byte
	NoteBytes                        [160]byte
	BalanceBlinding                  [32]byte
	AssetPath                        MerklePathBinary
	AssetPosition                    uint64
	AssetIndexedLeaf                 IndexedLeafBinary
	IsRegulated                      bool
	CompliancePath                   MerklePathBinary
	CompliancePosition               uint64
	UserAddress                      [80]byte
	UserAssetID                      [32]byte
	UserD                            [32]byte
	ComplianceEphemeral              [32]byte
	R2                               [32]byte
	R3                               [32]byte
	CounterpartyAddress              [80]byte
	CounterpartyAssetID              [32]byte
	CounterpartyD                    [32]byte
	TxBlindingNonce                  [32]byte
	IsFlagged                        bool
	Salt                             [32]byte
	BalanceCommitmentXY              PointAffineBinary
	Epk1Affine                       PointAffineBinary
	Epk2Affine                       PointAffineBinary
	Epk3Affine                       PointAffineBinary
	NoteDivGenAffine                 PointAffineBinary
	NoteTransmissionAffine           PointAffineBinary
	AssetIndexedLeafDKPub            PointAffineBinary
	AssetIndexedLeafRingPK           PointAffineBinary
	UserDiversifiedGenerator         PointAffineBinary
	UserTransmissionKey              PointAffineBinary
	CounterpartyDiversifiedGenerator PointAffineBinary
	CounterpartyTransmissionKey      PointAffineBinary
}

func DecodeOutputWitnessV1(payload []byte) (*OutputWitnessV1Binary, error) {
	reader := bytes.NewReader(payload)

	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, err
	}
	if string(magic) != outputWitnessV1Magic {
		return nil, fmt.Errorf("invalid OutputWitnessV1 magic %q", string(magic))
	}
	version, err := readU32(reader)
	if err != nil {
		return nil, err
	}
	if version != outputWitnessV1Version {
		return nil, fmt.Errorf("unsupported OutputWitnessV1 version %d", version)
	}
	totalLength, err := readU32(reader)
	if err != nil {
		return nil, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, fmt.Errorf("payload length mismatch: header=%d actual=%d", totalLength, len(payload))
	}

	witness := &OutputWitnessV1Binary{TotalLength: totalLength}
	if witness.NoteCommitment, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.BalanceCommitment, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Epk1, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Epk2, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Epk3, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.C2Core, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.C2Ext, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.C2Sext, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ComplianceCiphertext, err = readVec32(reader); err != nil {
		return nil, err
	}
	if witness.TargetTimestamp, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DleqC1, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DleqS1, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DleqC2, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DleqS2, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DleqC3, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DleqS3, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.AssetAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ComplianceAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.CounterpartyLeafHash, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ClaimedStatementHash, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.StatementFields, err = readVec32(reader); err != nil {
		return nil, err
	}
	if witness.NoteBlinding, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.NoteAmount, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.NoteAssetID, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DiversifiedGenerator, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.TransmissionKey, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ClueKey, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.NoteBytes, err = read160(reader); err != nil {
		return nil, err
	}
	if witness.BalanceBlinding, err = read32(reader); err != nil {
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
	if witness.CompliancePath, err = readMerklePath(reader); err != nil {
		return nil, err
	}
	if witness.CompliancePosition, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.UserAddress, err = read80(reader); err != nil {
		return nil, err
	}
	if witness.UserAssetID, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.UserD, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ComplianceEphemeral, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.R2, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.R3, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.CounterpartyAddress, err = read80(reader); err != nil {
		return nil, err
	}
	if witness.CounterpartyAssetID, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.CounterpartyD, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.TxBlindingNonce, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.IsFlagged, err = readBool(reader); err != nil {
		return nil, err
	}
	if witness.Salt, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.BalanceCommitmentXY, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output balance commitment affine: %w", err)
	}
	if witness.Epk1Affine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output epk1 affine: %w", err)
	}
	if witness.Epk2Affine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output epk2 affine: %w", err)
	}
	if witness.Epk3Affine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output epk3 affine: %w", err)
	}
	if witness.NoteDivGenAffine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output note div gen affine: %w", err)
	}
	if witness.NoteTransmissionAffine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output note transmission affine: %w", err)
	}
	if witness.AssetIndexedLeafDKPub, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output indexed leaf dk_pub affine: %w", err)
	}
	if witness.AssetIndexedLeafRingPK, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output indexed leaf ring_pk affine: %w", err)
	}
	if witness.UserDiversifiedGenerator, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output user div gen affine: %w", err)
	}
	if witness.UserTransmissionKey, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output user transmission affine: %w", err)
	}
	if witness.CounterpartyDiversifiedGenerator, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output counterparty div gen affine: %w", err)
	}
	if witness.CounterpartyTransmissionKey, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read output counterparty transmission affine: %w", err)
	}

	if reader.Len() != 0 {
		return nil, fmt.Errorf("trailing bytes in OutputWitnessV1 payload: %d", reader.Len())
	}
	return witness, nil
}
