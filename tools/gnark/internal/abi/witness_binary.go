package abi

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"io"

	"github.com/penumbra-zone/penumbra/tools/gnark/internal/circuits"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

const (
	spendWitnessV1Magic   = "PSWG"
	spendWitnessV1Version = 1
	maxVec32Length        = primitives.TransferStatementBaseFields + 2*primitives.TransferStatementFieldsPerInput + 2*primitives.TransferStatementFieldsPerOutput
	maxTriplePathLength   = circuits.StateCommitmentDepth
	maxMerklePathLayers   = compliance.ComplianceQuadTreeDepth
	maxMerklePathSiblings = 3
)

type MerklePathBinary struct {
	Layers [][][32]byte
}

type IndexedLeafBinary struct {
	Value          [32]byte
	NextIndex      uint64
	NextValue      [32]byte
	DKPub          [32]byte
	Threshold      [16]byte
	ChannelsHash   [32]byte
	RingPK         [32]byte
	RingIDHash     [32]byte
	PolicyIDHash   [32]byte
	PermissionHash [32]byte
	ResourceHash   [32]byte
}

type PointAffineBinary struct {
	X [32]byte
	Y [32]byte
}

type SpendWitnessV1Binary struct {
	TotalLength uint32

	Anchor               [32]byte
	BalanceCommitment    [32]byte
	Nullifier            [32]byte
	RK                   [32]byte
	AssetAnchor          [32]byte
	ComplianceAnchor     [32]byte
	Epk                  [32]byte
	C2Core               [32]byte
	ComplianceCiphertext [][32]byte
	TargetTimestamp      [32]byte
	DleqC                [32]byte
	DleqS                [32]byte
	SenderLeafHash       [32]byte
	ClaimedStatementHash [32]byte
	StatementFields      [][32]byte

	NoteBlinding              [32]byte
	NoteAmount                [32]byte
	NoteAssetID               [32]byte
	DiversifiedGenerator      [32]byte
	TransmissionKey           [32]byte
	ClueKey                   [32]byte
	NoteBytes                 [160]byte
	StateCommitmentCommitment [32]byte
	StateCommitmentPosition   uint64
	StateCommitmentAuthPath   [][3][32]byte
	VBlinding                 [32]byte
	SpendAuthRandomizer       [32]byte
	AK                        [32]byte
	NK                        [32]byte
	AssetPath                 MerklePathBinary
	AssetPosition             uint64
	AssetIndexedLeaf          IndexedLeafBinary
	IsRegulated               bool
	CompliancePath            MerklePathBinary
	CompliancePosition        uint64
	UserAddress               [80]byte
	UserAssetID               [32]byte
	UserD                     [32]byte
	ComplianceEphemeralSecret [32]byte
	TxBlindingNonce           [32]byte
	IsFlagged                 bool
	Salt                      [32]byte

	BalanceCommitmentAffine    PointAffineBinary
	RKAffine                   PointAffineBinary
	EpkAffine                  PointAffineBinary
	DiversifiedGeneratorAffine PointAffineBinary
	TransmissionKeyAffine      PointAffineBinary
	AKAffine                   PointAffineBinary
	AssetIndexedLeafDKPub      PointAffineBinary
	AssetIndexedLeafRingPK     PointAffineBinary
	UserDiversifiedGenerator   PointAffineBinary
	UserTransmissionKey        PointAffineBinary
}

func DecodeSpendWitnessV1(payload []byte) (*SpendWitnessV1Binary, error) {
	reader := bytes.NewReader(payload)

	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, err
	}
	if string(magic) != spendWitnessV1Magic {
		return nil, fmt.Errorf("invalid SpendWitnessV1 magic %q", string(magic))
	}

	version, err := readU32(reader)
	if err != nil {
		return nil, err
	}
	if version != spendWitnessV1Version {
		return nil, fmt.Errorf("unsupported SpendWitnessV1 version %d", version)
	}

	totalLength, err := readU32(reader)
	if err != nil {
		return nil, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, fmt.Errorf("payload length mismatch: header=%d actual=%d", totalLength, len(payload))
	}

	witness := &SpendWitnessV1Binary{TotalLength: totalLength}

	if witness.Anchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.BalanceCommitment, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Nullifier, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.RK, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.AssetAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ComplianceAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Epk, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.C2Core, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ComplianceCiphertext, err = readVec32(reader); err != nil {
		return nil, err
	}
	if witness.TargetTimestamp, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DleqC, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DleqS, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.SenderLeafHash, err = read32(reader); err != nil {
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
	if witness.StateCommitmentCommitment, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.StateCommitmentPosition, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.StateCommitmentAuthPath, err = readTriplePath(reader); err != nil {
		return nil, err
	}
	if witness.VBlinding, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.SpendAuthRandomizer, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.AK, err = read32(reader); err != nil {
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
	if witness.ComplianceEphemeralSecret, err = read32(reader); err != nil {
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
	if witness.BalanceCommitmentAffine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read balance commitment affine: %w", err)
	}
	if witness.RKAffine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read rk affine: %w", err)
	}
	if witness.EpkAffine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read epk affine: %w", err)
	}
	if witness.DiversifiedGeneratorAffine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read diversified generator affine: %w", err)
	}
	if witness.TransmissionKeyAffine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read transmission key affine: %w", err)
	}
	if witness.AKAffine, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read ak affine: %w", err)
	}
	if witness.AssetIndexedLeafDKPub, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read asset indexed leaf dk_pub affine: %w", err)
	}
	if witness.AssetIndexedLeafRingPK, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read asset indexed leaf ring_pk affine: %w", err)
	}
	if witness.UserDiversifiedGenerator, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read user diversified generator affine: %w", err)
	}
	if witness.UserTransmissionKey, err = readPointAffine(reader); err != nil {
		return nil, fmt.Errorf("read user transmission key affine: %w", err)
	}

	if reader.Len() != 0 {
		return nil, fmt.Errorf("trailing bytes in SpendWitnessV1 payload: %d", reader.Len())
	}

	return witness, nil
}

func readExact(r io.Reader, n int) ([]byte, error) {
	buf := make([]byte, n)
	if _, err := io.ReadFull(r, buf); err != nil {
		return nil, err
	}
	return buf, nil
}

func read32(r io.Reader) ([32]byte, error) {
	var out [32]byte
	_, err := io.ReadFull(r, out[:])
	return out, err
}

func read80(r io.Reader) ([80]byte, error) {
	var out [80]byte
	_, err := io.ReadFull(r, out[:])
	return out, err
}

func readPointAffine(r io.Reader) (PointAffineBinary, error) {
	x, err := read32(r)
	if err != nil {
		return PointAffineBinary{}, err
	}
	y, err := read32(r)
	if err != nil {
		return PointAffineBinary{}, err
	}
	return PointAffineBinary{X: x, Y: y}, nil
}

func read160(r io.Reader) ([160]byte, error) {
	var out [160]byte
	_, err := io.ReadFull(r, out[:])
	return out, err
}

func readU32(r io.Reader) (uint32, error) {
	var out uint32
	err := binary.Read(r, binary.LittleEndian, &out)
	return out, err
}

func readU64(r io.Reader) (uint64, error) {
	var out uint64
	err := binary.Read(r, binary.LittleEndian, &out)
	return out, err
}

func readBool(r io.Reader) (bool, error) {
	var out [1]byte
	if _, err := io.ReadFull(r, out[:]); err != nil {
		return false, err
	}
	return out[0] != 0, nil
}

func readVec32(r io.Reader) ([][32]byte, error) {
	length, err := readU32(r)
	if err != nil {
		return nil, err
	}
	if length > maxVec32Length {
		return nil, fmt.Errorf("vec32 length %d exceeds max %d", length, maxVec32Length)
	}
	out := make([][32]byte, length)
	for i := range out {
		if out[i], err = read32(r); err != nil {
			return nil, err
		}
	}
	return out, nil
}

func readTriplePath(r io.Reader) ([][3][32]byte, error) {
	length, err := readU32(r)
	if err != nil {
		return nil, err
	}
	if length > maxTriplePathLength {
		return nil, fmt.Errorf("triple path length %d exceeds max %d", length, maxTriplePathLength)
	}
	out := make([][3][32]byte, length)
	for i := range out {
		for j := 0; j < 3; j++ {
			if out[i][j], err = read32(r); err != nil {
				return nil, err
			}
		}
	}
	return out, nil
}

func readMerklePath(r io.Reader) (MerklePathBinary, error) {
	layerCount, err := readU32(r)
	if err != nil {
		return MerklePathBinary{}, err
	}
	if layerCount > maxMerklePathLayers {
		return MerklePathBinary{}, fmt.Errorf("merkle path layer count %d exceeds max %d", layerCount, maxMerklePathLayers)
	}
	path := MerklePathBinary{Layers: make([][][32]byte, layerCount)}
	for i := range path.Layers {
		siblingCount, err := readU32(r)
		if err != nil {
			return MerklePathBinary{}, err
		}
		if siblingCount > maxMerklePathSiblings {
			return MerklePathBinary{}, fmt.Errorf("merkle path sibling count %d exceeds max %d", siblingCount, maxMerklePathSiblings)
		}
		path.Layers[i] = make([][32]byte, siblingCount)
		for j := range path.Layers[i] {
			if path.Layers[i][j], err = read32(r); err != nil {
				return MerklePathBinary{}, err
			}
		}
	}
	return path, nil
}

func readIndexedLeaf(r io.Reader) (IndexedLeafBinary, error) {
	var out IndexedLeafBinary
	var err error
	if out.Value, err = read32(r); err != nil {
		return out, err
	}
	if out.NextIndex, err = readU64(r); err != nil {
		return out, err
	}
	if out.NextValue, err = read32(r); err != nil {
		return out, err
	}
	if out.DKPub, err = read32(r); err != nil {
		return out, err
	}
	if _, err := io.ReadFull(r, out.Threshold[:]); err != nil {
		return out, err
	}
	if out.ChannelsHash, err = read32(r); err != nil {
		return out, err
	}
	if out.RingPK, err = read32(r); err != nil {
		return out, err
	}
	if out.RingIDHash, err = read32(r); err != nil {
		return out, err
	}
	if out.PolicyIDHash, err = read32(r); err != nil {
		return out, err
	}
	if out.PermissionHash, err = read32(r); err != nil {
		return out, err
	}
	if out.ResourceHash, err = read32(r); err != nil {
		return out, err
	}
	return out, nil
}
