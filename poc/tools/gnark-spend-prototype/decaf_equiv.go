package prototype

import (
	"math/big"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
)

func AssertDecafEquivalent(api frontend.API, left, right gnarkte.Point) {
	api.AssertIsEqual(api.Mul(left.X, right.Y), api.Mul(right.X, left.Y))
}

func DecafEquivalentNative(left, right gnarkte.Point) bool {
	modulus := ScalarField()
	lhs := new(big.Int).Mul(left.X.(*big.Int), right.Y.(*big.Int))
	lhs.Mod(lhs, modulus)
	rhs := new(big.Int).Mul(right.X.(*big.Int), left.Y.(*big.Int))
	rhs.Mod(rhs, modulus)
	return lhs.Cmp(rhs) == 0
}
