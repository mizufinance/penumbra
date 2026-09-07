package abi

import (
	"encoding/binary"
	"fmt"
	"io"

	decaf377 "github.com/mizufinance/decaf377-go"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

const (
	maxVec32Length        = primitives.TransferStatementBaseFields + 2*primitives.TransferStatementFieldsPerInput + 2*primitives.TransferStatementFieldsPerOutput
	maxTriplePathLength   = circuits.StateCommitmentDepth
	maxMerklePathLayers   = compliance.ComplianceQuadTreeDepth
	maxMerklePathSiblings = 3
)

type MerklePathBinary struct {
	Layers [][][32]byte
}

type IndexedLeafBinary struct {
	Value            [32]byte
	NextIndex        uint64
	NextValue        [32]byte
	DailyVolumeLimit [16]byte
	RoutePolicyHash  [32]byte
	RingIDHash       [32]byte
	PolicyIDHash     [32]byte
	PermissionHash   [32]byte
	ResourceHash     [32]byte
}

type PointAffineBinary struct {
	X [32]byte
	Y [32]byte
}

func readExact(r io.Reader, n int) ([]byte, error) {
	buf := make([]byte, n)
	if _, err := io.ReadFull(r, buf); err != nil {
		return nil, err
	}
	return buf, nil
}

func readRaw32(r io.Reader) ([32]byte, error) {
	var out [32]byte
	_, err := io.ReadFull(r, out[:])
	return out, err
}

// read32 reads a canonical BLS12-377 scalar-field (Decaf377 Fq) value.
// Every 32-byte value in the witness ABI is a circuit field; narrower
// Decaf377 scalar values must use readFr32.
func read32(r io.Reader) ([32]byte, error) {
	out, err := readRaw32(r)
	if err != nil {
		return out, err
	}
	if primitives.LittleEndianBytesToBigInt(out[:]).Cmp(primitives.ScalarField()) >= 0 {
		return [32]byte{}, fmt.Errorf("non-canonical Fq encoding")
	}
	return out, nil
}

func readFr32(r io.Reader) ([32]byte, error) {
	out, err := readRaw32(r)
	if err != nil {
		return out, err
	}
	if primitives.LittleEndianBytesToBigInt(out[:]).Cmp(decaf377.ScalarOrder()) >= 0 {
		return [32]byte{}, fmt.Errorf("non-canonical Fr encoding")
	}
	return out, nil
}

func readU128Field(r io.Reader) ([32]byte, error) {
	out, err := read32(r)
	if err != nil {
		return out, err
	}
	for _, value := range out[16:] {
		if value != 0 {
			return [32]byte{}, fmt.Errorf("field encoding exceeds 128 bits")
		}
	}
	return out, nil
}

func read80(r io.Reader) ([80]byte, error) {
	var out [80]byte
	_, err := io.ReadFull(r, out[:])
	return out, err
}

func read160(r io.Reader) ([160]byte, error) {
	var out [160]byte
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
	switch out[0] {
	case 0:
		return false, nil
	case 1:
		return true, nil
	default:
		return false, fmt.Errorf("non-canonical boolean byte %d", out[0])
	}
}

func readU8(r io.Reader) (uint8, error) {
	value, err := readExact(r, 1)
	if err != nil {
		return 0, err
	}
	return value[0], nil
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
	if length != maxTriplePathLength {
		return nil, fmt.Errorf("triple path length %d must equal %d", length, maxTriplePathLength)
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
	if layerCount != maxMerklePathLayers {
		return MerklePathBinary{}, fmt.Errorf("merkle path layer count %d must equal %d", layerCount, maxMerklePathLayers)
	}
	path := MerklePathBinary{Layers: make([][][32]byte, layerCount)}
	for i := range path.Layers {
		siblingCount, err := readU32(r)
		if err != nil {
			return MerklePathBinary{}, err
		}
		if siblingCount != maxMerklePathSiblings {
			return MerklePathBinary{}, fmt.Errorf("merkle path sibling count %d must equal %d", siblingCount, maxMerklePathSiblings)
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
	if _, err := io.ReadFull(r, out.DailyVolumeLimit[:]); err != nil {
		return out, err
	}
	if out.RoutePolicyHash, err = read32(r); err != nil {
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
