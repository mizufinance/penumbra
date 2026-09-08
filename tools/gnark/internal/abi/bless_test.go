//go:build fixtures

package abi

import (
	"os"
	"path/filepath"
	"testing"
)

func TestBlessNoteSeizureWitness(t *testing.T) {
	payload, err := EncodeNoteSeizureWitness(validNoteSeizureWitness(t))
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join("..", "testfixtures", "vectors", "note_seizure_witness.bin")
	if err := os.WriteFile(path, payload, 0o644); err != nil {
		t.Fatal(err)
	}
}
