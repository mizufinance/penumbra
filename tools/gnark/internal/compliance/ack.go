package compliance

import (
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

func DeriveACKFromLeafDNative(ringPK gnarkte.Point, d *big.Int) (gnarkte.Point, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	return primitives.ScalarMulNative(ringPK, d, primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen())
}

func DeriveACKFromLeafD(api frontend.API, ringPK gnarkte.Point, d frontend.Variable) (gnarkte.Point, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return ScalarMulLE(api, curve, ringPK, d, primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()), nil
}
