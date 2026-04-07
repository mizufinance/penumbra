package abi

import (
	"math/big"
	"testing"

	"github.com/penumbra-zone/penumbra/tools/gnark/internal/circuits"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

func TestQuotientAsUint64RejectsOverflow(t *testing.T) {
	overflow := new(big.Int).Lsh(big.NewInt(1), 65)
	if _, err := quotientAsUint64(overflow); err == nil {
		t.Fatalf("expected quotient overflow to fail")
	}
}

func TestStatePathFromBinaryZeroInitializesTrailingEntries(t *testing.T) {
	path, err := statePathFromBinary([][3][32]byte{{{1}, {2}, {3}}})
	if err != nil {
		t.Fatalf("statePathFromBinary: %v", err)
	}
	if path[0][0] == 0 {
		t.Fatalf("expected populated first layer")
	}
	if path[1][0] != 0 || path[circuits.StateCommitmentDepth-1][2] != 0 {
		t.Fatalf("expected trailing state path entries to be zero initialized")
	}
}

func TestStatePathFromBinaryRejectsTooManyLayers(t *testing.T) {
	path := make([][3][32]byte, circuits.StateCommitmentDepth+1)
	if _, err := statePathFromBinary(path); err == nil {
		t.Fatalf("expected oversized state path to fail")
	}
}

func TestQuadPathFromBinaryZeroInitializesTrailingEntries(t *testing.T) {
	path, err := quadPathFromBinary(MerklePathBinary{
		Layers: [][][32]byte{
			{{1}, {2}, {3}},
		},
	})
	if err != nil {
		t.Fatalf("quadPathFromBinary: %v", err)
	}
	if path[0][0] == 0 {
		t.Fatalf("expected populated first quad layer")
	}
	if path[1][0] != 0 || path[len(path)-1][2] != 0 {
		t.Fatalf("expected trailing quad path entries to be zero initialized")
	}
}

func TestQuadPathFromBinaryRejectsBadSiblingCount(t *testing.T) {
	_, err := quadPathFromBinary(MerklePathBinary{
		Layers: [][][32]byte{
			{{1}, {2}},
		},
	})
	if err == nil {
		t.Fatalf("expected malformed quad layer to fail")
	}
}

func TestNewSpendCircuitAssignmentRejectsCiphertextCountMismatch(t *testing.T) {
	witness, err := DecodeSpendWitnessV1(primitives.LoadSpendWitnessV1())
	if err != nil {
		t.Fatalf("decode spend witness: %v", err)
	}
	witness.ComplianceCiphertext = witness.ComplianceCiphertext[:len(witness.ComplianceCiphertext)-1]
	if _, err := newSpendCircuitAssignment(witness); err == nil {
		t.Fatalf("expected spend ciphertext count mismatch to fail")
	}
}

func TestNewOutputCircuitAssignmentRejectsCiphertextCountMismatch(t *testing.T) {
	witness, err := DecodeOutputWitnessV1(primitives.LoadOutputWitnessV1())
	if err != nil {
		t.Fatalf("decode output witness: %v", err)
	}
	witness.ComplianceCiphertext = witness.ComplianceCiphertext[:len(witness.ComplianceCiphertext)-1]
	if _, err := newOutputCircuitAssignment(witness); err == nil {
		t.Fatalf("expected output ciphertext count mismatch to fail")
	}
}
