package primitives

import (
	"fmt"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
)

func modInverseOrError(value, modulus *big.Int, label string) (*big.Int, error) {
	inv := new(big.Int).ModInverse(value, modulus)
	if inv == nil {
		return nil, fmt.Errorf("mod inverse does not exist for %s", label)
	}
	return inv, nil
}

func PointAddNative(left, right gnarkte.Point) (gnarkte.Point, error) {
	curve, err := gnarkte.GetCurveParams(curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	x1 := left.X.(*big.Int)
	y1 := left.Y.(*big.Int)
	x2 := right.X.(*big.Int)
	y2 := right.Y.(*big.Int)
	field := ScalarField()

	x1x2 := new(big.Int).Mul(x1, x2)
	x1x2.Mod(x1x2, field)
	y1y2 := new(big.Int).Mul(y1, y2)
	y1y2.Mod(y1y2, field)
	dxxyy := new(big.Int).Mul(curve.D, x1x2)
	dxxyy.Mul(dxxyy, y1y2)
	dxxyy.Mod(dxxyy, field)

	xNum := new(big.Int).Mul(x1, y2)
	xNum.Add(xNum, new(big.Int).Mul(y1, x2))
	xNum.Mod(xNum, field)
	xDen := new(big.Int).Add(big.NewInt(1), dxxyy)
	xDen, err = modInverseOrError(xDen, field, "x denominator")
	if err != nil {
		return gnarkte.Point{}, err
	}
	x := xNum.Mul(xNum, xDen)
	x.Mod(x, field)

	yNum := new(big.Int).Mul(y1, y2)
	ax1x2 := new(big.Int).Mul(curve.A, x1x2)
	yNum.Sub(yNum, ax1x2)
	yNum.Mod(yNum, field)
	yDen := new(big.Int).Sub(big.NewInt(1), dxxyy)
	yDen, err = modInverseOrError(yDen, field, "y denominator")
	if err != nil {
		return gnarkte.Point{}, err
	}
	y := yNum.Mul(yNum, yDen)
	y.Mod(y, field)

	return gnarkte.Point{X: x, Y: y}, nil
}

func ScalarMulNative(base gnarkte.Point, scalar *big.Int, nBits int) (gnarkte.Point, error) {
	result := gnarkte.Point{X: big.NewInt(0), Y: big.NewInt(1)}
	current := base
	for i := 0; i < nBits; i++ {
		if scalar.Bit(i) == 1 {
			var err error
			result, err = PointAddNative(result, current)
			if err != nil {
				return gnarkte.Point{}, err
			}
		}
		var err error
		current, err = PointAddNative(current, current)
		if err != nil {
			return gnarkte.Point{}, err
		}
	}
	return result, nil
}

func IsLessThanConstant(api frontend.API, value frontend.Variable, constant *big.Int) (frontend.Variable, error) {
	valueBits := api.ToBinary(value, Decaf377FieldBits)
	constantBits := make([]uint, Decaf377FieldBits)
	for i := 0; i < Decaf377FieldBits; i++ {
		constantBits[i] = constant.Bit(i)
	}

	prefixEqual := frontend.Variable(1)
	isLess := frontend.Variable(0)
	for i := Decaf377FieldBits - 1; i >= 0; i-- {
		valueBit := valueBits[i]
		if constantBits[i] == 1 {
			thisBitProvesLess := api.Mul(prefixEqual, api.Sub(1, valueBit))
			isLess = api.Sub(
				api.Add(isLess, thisBitProvesLess),
				api.Mul(isLess, thisBitProvesLess),
			)
			prefixEqual = api.Mul(prefixEqual, valueBit)
		} else {
			prefixEqual = api.Mul(prefixEqual, api.Sub(1, valueBit))
		}
	}
	return isLess, nil
}
