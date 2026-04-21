package abi

import (
	"encoding/binary"
	"fmt"
	"io"

	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/compliance"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
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
