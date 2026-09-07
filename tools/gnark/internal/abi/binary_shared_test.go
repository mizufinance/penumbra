package abi

import (
	"bytes"
	"encoding/binary"
	"math/big"
	"strings"
	"testing"

	decaf377 "github.com/mizufinance/decaf377-go"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

func encodedU32(value uint32) []byte {
	var out [4]byte
	binary.LittleEndian.PutUint32(out[:], value)
	return out[:]
}

func littleEndian32(value *big.Int) [32]byte {
	var out [32]byte
	be := value.Bytes()
	for index := range be {
		out[index] = be[len(be)-1-index]
	}
	return out
}

func TestRead32RejectsFqModulusAliases(t *testing.T) {
	one := littleEndian32(big.NewInt(1))
	if got, err := read32(bytes.NewReader(one[:])); err != nil || got != one {
		t.Fatalf("read canonical Fq one: got %x, err %v", got, err)
	}

	for _, value := range []*big.Int{
		primitives.ScalarField(),
		new(big.Int).Add(primitives.ScalarField(), big.NewInt(1)),
	} {
		encoded := littleEndian32(value)
		_, err := read32(bytes.NewReader(encoded[:]))
		if err == nil || !strings.Contains(err.Error(), "non-canonical Fq") {
			t.Fatalf("read Fq alias %s: expected canonicality error, got %v", value, err)
		}
	}
}

func TestReadFr32RejectsScalarModulusAliases(t *testing.T) {
	one := littleEndian32(big.NewInt(1))
	if got, err := readFr32(bytes.NewReader(one[:])); err != nil || got != one {
		t.Fatalf("read canonical Fr one: got %x, err %v", got, err)
	}

	for _, value := range []*big.Int{
		decaf377.ScalarOrder(),
		new(big.Int).Add(decaf377.ScalarOrder(), big.NewInt(1)),
	} {
		encoded := littleEndian32(value)
		_, err := readFr32(bytes.NewReader(encoded[:]))
		if err == nil || !strings.Contains(err.Error(), "non-canonical Fr") {
			t.Fatalf("read Fr alias %s: expected canonicality error, got %v", value, err)
		}
	}
}

func TestTransferComplianceMetadataRejectsFqModulusSalt(t *testing.T) {
	var encoded bytes.Buffer
	for index := 0; index < 7; index++ {
		encoded.Write(make([]byte, 32))
	}
	modulus := littleEndian32(primitives.ScalarField())
	encoded.Write(modulus[:])

	_, err := readTransferComplianceMetadata(bytes.NewReader(encoded.Bytes()))
	if err == nil || !strings.Contains(err.Error(), "non-canonical Fq") {
		t.Fatalf("read transfer metadata salt at Fq modulus: expected canonicality error, got %v", err)
	}
}

func TestReadU128FieldRejectsHighBits(t *testing.T) {
	var max [32]byte
	for index := 0; index < 16; index++ {
		max[index] = 0xff
	}
	if got, err := readU128Field(bytes.NewReader(max[:])); err != nil || got != max {
		t.Fatalf("read canonical u128 field: got %x, err %v", got, err)
	}

	oversized := max
	oversized[16] = 1
	_, err := readU128Field(bytes.NewReader(oversized[:]))
	if err == nil || !strings.Contains(err.Error(), "exceeds 128 bits") {
		t.Fatalf("read oversized u128 field: expected width error, got %v", err)
	}
}

func TestReadBoolAcceptsOnlyCanonicalBytes(t *testing.T) {
	for _, testCase := range []struct {
		value byte
		want  bool
	}{
		{value: 0, want: false},
		{value: 1, want: true},
	} {
		got, err := readBool(bytes.NewReader([]byte{testCase.value}))
		if err != nil {
			t.Fatalf("read canonical boolean %d: %v", testCase.value, err)
		}
		if got != testCase.want {
			t.Fatalf("read canonical boolean %d: got %v, want %v", testCase.value, got, testCase.want)
		}
	}

	for _, value := range []byte{2, 0xff} {
		_, err := readBool(bytes.NewReader([]byte{value}))
		if err == nil || !strings.Contains(err.Error(), "non-canonical boolean") {
			t.Fatalf("read non-canonical boolean %d: expected canonicality error, got %v", value, err)
		}
	}
}

func TestReadTriplePathRequiresExactDepth(t *testing.T) {
	for _, length := range []uint32{maxTriplePathLength - 1, maxTriplePathLength + 1} {
		_, err := readTriplePath(bytes.NewReader(encodedU32(length)))
		if err == nil || !strings.Contains(err.Error(), "triple path length") {
			t.Fatalf("read triple path length %d: expected depth error, got %v", length, err)
		}
	}
}

func TestReadMerklePathRequiresExactShape(t *testing.T) {
	for _, layers := range []uint32{maxMerklePathLayers - 1, maxMerklePathLayers + 1} {
		_, err := readMerklePath(bytes.NewReader(encodedU32(layers)))
		if err == nil || !strings.Contains(err.Error(), "merkle path layer count") {
			t.Fatalf("read merkle path with %d layers: expected depth error, got %v", layers, err)
		}
	}

	var malformed bytes.Buffer
	if err := binary.Write(&malformed, binary.LittleEndian, uint32(maxMerklePathLayers)); err != nil {
		t.Fatal(err)
	}
	if err := binary.Write(&malformed, binary.LittleEndian, uint32(maxMerklePathSiblings-1)); err != nil {
		t.Fatal(err)
	}
	_, err := readMerklePath(&malformed)
	if err == nil || !strings.Contains(err.Error(), "merkle path sibling count") {
		t.Fatalf("read merkle path with short sibling list: expected shape error, got %v", err)
	}
}

func TestAssignmentPathConversionRequiresExactDepth(t *testing.T) {
	for _, length := range []int{maxTriplePathLength - 1, maxTriplePathLength + 1} {
		_, err := statePathFromBinary(make([][3][32]byte, length))
		if err == nil || !strings.Contains(err.Error(), "state path") {
			t.Fatalf("convert state path length %d: expected depth error, got %v", length, err)
		}
	}

	for _, layers := range []int{maxMerklePathLayers - 1, maxMerklePathLayers + 1} {
		_, err := quadPathFromBinary(MerklePathBinary{Layers: make([][][32]byte, layers)})
		if err == nil || !strings.Contains(err.Error(), "path has") {
			t.Fatalf("convert compliance path length %d: expected depth error, got %v", layers, err)
		}
	}

	path := MerklePathBinary{Layers: make([][][32]byte, maxMerklePathLayers)}
	for index := range path.Layers {
		path.Layers[index] = make([][32]byte, maxMerklePathSiblings)
	}
	path.Layers[0] = make([][32]byte, maxMerklePathSiblings-1)
	_, err := quadPathFromBinary(path)
	if err == nil || !strings.Contains(err.Error(), "siblings") {
		t.Fatalf("convert compliance path with short sibling list: expected shape error, got %v", err)
	}
}
