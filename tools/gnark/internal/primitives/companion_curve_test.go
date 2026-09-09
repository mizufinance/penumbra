package primitives

import (
	"testing"

	te377 "github.com/consensys/gnark-crypto/ecc/bls12-377/twistededwards"
)

func TestCompanionCurveMatchesDecaf377(t *testing.T) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	curve := te377.GetEdwardsCurve()

	if got, want := curve.A.String(), vectors.Decaf377CompanionCurve.A; got != want {
		t.Fatalf("curve A mismatch: got %s want %s", got, want)
	}
	if got, want := curve.D.String(), vectors.Decaf377CompanionCurve.D; got != want {
		t.Fatalf("curve D mismatch: got %s want %s", got, want)
	}
	if got, want := curve.Order.String(), vectors.Decaf377CompanionCurve.Order; got != want {
		t.Fatalf("curve order mismatch: got %s want %s", got, want)
	}
}
