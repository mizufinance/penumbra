package primitives

import (
	"math/big"
	"testing"
)

func TestModInverseOrErrorRejectsZero(t *testing.T) {
	if _, err := modInverseOrError(big.NewInt(0), ScalarField(), "zero"); err == nil {
		t.Fatalf("expected zero inverse to fail")
	}
}
