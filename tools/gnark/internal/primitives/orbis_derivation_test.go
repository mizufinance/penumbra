package primitives

import (
	"crypto/sha512"
	"encoding/hex"
	"math/big"
	"testing"
)

func TestOrbisDerivationMatchesRustVector(t *testing.T) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatal(err)
	}
	derivation, err := hex.DecodeString(vectors.OrbisDerivation.DerivationHex)
	if err != nil {
		t.Fatal(err)
	}
	if len(derivation) != 48 {
		t.Fatalf("ordinary Shieldd address derivation must be 48 bytes, got %d", len(derivation))
	}
	transcript := append([]byte("elgamal-derivation-v1\x00\x00"), derivation...)
	digest := sha512.Sum512(transcript)
	wideLE := LittleEndianBytesToBigInt(digest[:])
	order, ok := new(big.Int).SetString(vectors.Decaf377CompanionCurve.Order, 10)
	if !ok {
		t.Fatal("invalid scalar field order vector")
	}
	actual := new(big.Int).Mod(wideLE, order)
	if actual.String() != vectors.OrbisDerivation.Scalar {
		t.Fatalf("Orbis derivation mismatch: got %s want %s", actual, vectors.OrbisDerivation.Scalar)
	}
}
