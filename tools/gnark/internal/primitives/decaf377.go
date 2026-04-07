package primitives

import (
	"errors"
	"math/big"

	fr "github.com/consensys/gnark-crypto/ecc/bls12-377/fr"
	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/constraint/solver"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
)

const Decaf377FieldBits = 253

type decaf377Constants struct {
	a       *big.Int
	d       *big.Int
	aMinusD *big.Int
	zeta    *big.Int
}

func init() {
	solver.RegisterHint(decaf377SqrtRatioZetaHint)
}

func loadDecaf377Constants() (decaf377Constants, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return decaf377Constants{}, err
	}
	return decaf377Constants{
		a:       MustBigInt(vectors.Decaf377CompanionCurve.A),
		d:       MustBigInt(vectors.Decaf377CompanionCurve.D),
		aMinusD: MustBigInt(vectors.Decaf377CompanionCurve.AMinusD),
		zeta:    MustBigInt(vectors.Decaf377CompanionCurve.Zeta),
	}, nil
}

func decaf377SqrtRatioZetaHint(field *big.Int, inputs []*big.Int, outputs []*big.Int) error {
	if len(inputs) != 1 || len(outputs) != 2 {
		return errors.New("decaf377SqrtRatioZetaHint expects one input and two outputs")
	}

	constants, err := loadDecaf377Constants()
	if err != nil {
		return err
	}

	var den, invDen, y, zeta, zetaInvDen, zero fr.Element
	den.SetBigInt(inputs[0])
	if den.Equal(&zero) {
		outputs[0].SetInt64(0)
		outputs[1].SetInt64(0)
		return nil
	}

	invDen.Inverse(&den)
	if y.Sqrt(&invDen) != nil {
		outputs[0].SetInt64(1)
		y.BigInt(outputs[1])
		return nil
	}

	zeta.SetBigInt(constants.zeta)
	zetaInvDen.Mul(&zeta, &invDen)
	if y.Sqrt(&zetaInvDen) == nil {
		return errors.New("sqrt_ratio_zeta produced no square root")
	}

	outputs[0].SetInt64(0)
	y.BigInt(outputs[1])
	return nil
}

func decaf377Abs(api frontend.API, value frontend.Variable) frontend.Variable {
	bits := api.ToBinary(value, Decaf377FieldBits)
	isNonnegative := api.Sub(1, bits[0])
	return api.Select(isNonnegative, value, api.Neg(value))
}

func decaf377Isqrt(api frontend.API, den frontend.Variable) (frontend.Variable, frontend.Variable, error) {
	constants, err := loadDecaf377Constants()
	if err != nil {
		return nil, nil, err
	}

	result, err := api.Compiler().NewHint(decaf377SqrtRatioZetaHint, 2, den)
	if err != nil {
		return nil, nil, err
	}

	wasSquare := result[0]
	y := result[1]
	api.AssertIsBoolean(wasSquare)

	denIsZero := api.IsZero(den)
	safeDen := api.Select(denIsZero, 1, den)
	safeDenInv := api.Inverse(safeDen)
	ySquared := api.Mul(y, y)
	api.AssertIsEqual(api.Mul(wasSquare, denIsZero), 0)

	notSquare := api.Sub(1, wasSquare)
	denNonZero := api.Sub(1, denIsZero)
	inCase3 := api.And(notSquare, denIsZero)
	inCase4 := api.And(notSquare, denNonZero)

	api.AssertIsEqual(api.Mul(wasSquare, api.Sub(ySquared, safeDenInv)), 0)
	api.AssertIsEqual(api.Mul(inCase3, ySquared), 0)
	api.AssertIsEqual(api.Mul(inCase4, api.Sub(ySquared, api.Mul(constants.zeta, safeDenInv))), 0)
	api.AssertIsEqual(api.Add(wasSquare, inCase3, inCase4), 1)

	return wasSquare, y, nil
}

// Decaf377CompressToField implements the minimal Penumbra decaf377 quotient gadget
// needed by spend: quotient-group compression to a single field element.
func Decaf377CompressToField(api frontend.API, point gnarkte.Point) (frontend.Variable, error) {
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return nil, err
	}
	curve.AssertIsOnCurve(point)

	constants, err := loadDecaf377Constants()
	if err != nil {
		return nil, err
	}

	x := point.X
	y := point.Y
	t := api.Mul(x, y)

	u1 := api.Mul(api.Add(x, t), api.Sub(x, t))
	den := api.Mul(u1, constants.aMinusD, api.Mul(x, x))

	_, v, err := decaf377Isqrt(api, den)
	if err != nil {
		return nil, err
	}

	u2 := decaf377Abs(api, api.Mul(v, u1))
	u3 := api.Sub(u2, t)
	return decaf377Abs(api, api.Mul(constants.aMinusD, v, u3, x)), nil
}

func Decaf377EncodeToCurve(api frontend.API, r0 frontend.Variable) (gnarkte.Point, error) {
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}

	constants, err := loadDecaf377Constants()
	if err != nil {
		return gnarkte.Point{}, err
	}

	r := api.Mul(constants.zeta, api.Mul(r0, r0))
	den := api.Mul(
		api.Sub(api.Mul(constants.d, r), api.Sub(constants.d, constants.a)),
		api.Sub(api.Mul(api.Sub(constants.d, constants.a), r), constants.d),
	)
	num := api.Mul(api.Add(r, 1), api.Sub(constants.a, api.Mul(2, constants.d)))
	x := api.Mul(num, den)

	iss, isri, err := decaf377Isqrt(api, x)
	if err != nil {
		return gnarkte.Point{}, err
	}

	sgn := api.Select(iss, 1, -1)
	twiddle := api.Select(iss, 1, r0)
	isri = api.Mul(isri, twiddle)

	s := api.Mul(isri, num)
	aMinusTwoD := api.Sub(constants.a, api.Mul(2, constants.d))
	t := api.Sub(
		api.Mul(
			api.Neg(sgn),
			isri,
			s,
			api.Sub(r, 1),
			api.Mul(aMinusTwoD, aMinusTwoD),
		),
		1,
	)

	isNegative := api.ToBinary(s, Decaf377FieldBits)[0]
	condNegate := api.IsZero(api.Sub(isNegative, iss))
	s = api.Select(condNegate, api.Neg(s), s)

	sSquared := api.Mul(s, s)
	affineXNum := api.Mul(2, s)
	affineXDen := api.Add(1, api.Mul(constants.a, sSquared))
	affineYNum := api.Sub(1, api.Mul(constants.a, sSquared))
	point := gnarkte.Point{
		X: api.Mul(affineXNum, api.Inverse(affineXDen)),
		Y: api.Mul(affineYNum, api.Inverse(t)),
	}
	curve.AssertIsOnCurve(point)
	return point, nil
}

func Decaf377EncodeToCurveNative(input *big.Int) (gnarkte.Point, error) {
	constants, err := loadDecaf377Constants()
	if err != nil {
		return gnarkte.Point{}, err
	}

	modulus := ScalarField()
	one := big.NewInt(1)
	two := big.NewInt(2)
	negOne := new(big.Int).Sub(modulus, one)

	r := new(big.Int).Mul(input, input)
	r.Mod(r, modulus)
	r.Mul(r, constants.zeta)
	r.Mod(r, modulus)

	dMinusA := new(big.Int).Sub(constants.d, constants.a)
	dMinusA.Mod(dMinusA, modulus)
	first := new(big.Int).Mul(constants.d, r)
	first.Sub(first, dMinusA)
	first.Mod(first, modulus)
	second := new(big.Int).Mul(dMinusA, r)
	second.Sub(second, constants.d)
	second.Mod(second, modulus)
	den := new(big.Int).Mul(first, second)
	den.Mod(den, modulus)

	aMinusTwoD := new(big.Int).Mul(two, constants.d)
	aMinusTwoD.Sub(constants.a, aMinusTwoD)
	aMinusTwoD.Mod(aMinusTwoD, modulus)

	num := new(big.Int).Add(r, one)
	num.Mul(num, aMinusTwoD)
	num.Mod(num, modulus)

	x := new(big.Int).Mul(num, den)
	x.Mod(x, modulus)

	iss, isri, err := decaf377SqrtRatioZetaNative(x)
	if err != nil {
		return gnarkte.Point{}, err
	}

	sgn := one
	twiddle := one
	if !iss {
		sgn = negOne
		twiddle = input
	}

	isri.Mul(isri, twiddle)
	isri.Mod(isri, modulus)

	s := new(big.Int).Mul(isri, num)
	s.Mod(s, modulus)

	t := new(big.Int).Sub(r, one)
	t.Mod(t, modulus)
	aMinusTwoDSquared := new(big.Int).Mul(aMinusTwoD, aMinusTwoD)
	aMinusTwoDSquared.Mod(aMinusTwoDSquared, modulus)
	t.Mul(t, isri)
	t.Mod(t, modulus)
	t.Mul(t, s)
	t.Mod(t, modulus)
	t.Mul(t, aMinusTwoDSquared)
	t.Mod(t, modulus)
	t.Mul(t, sgn)
	t.Mod(t, modulus)
	t.Neg(t)
	t.Sub(t, one)
	t.Mod(t, modulus)

	sNegative := s.Bit(0) == 1
	if sNegative == iss {
		s.Neg(s)
		s.Mod(s, modulus)
	}

	sSquared := new(big.Int).Mul(s, s)
	sSquared.Mod(sSquared, modulus)
	affineXNum := new(big.Int).Mul(two, s)
	affineXNum.Mod(affineXNum, modulus)
	affineXDen := new(big.Int).Mul(constants.a, sSquared)
	affineXDen.Add(affineXDen, one)
	affineXDen.Mod(affineXDen, modulus)
	affineYNum := new(big.Int).Mul(constants.a, sSquared)
	affineYNum.Sub(one, affineYNum)
	affineYNum.Mod(affineYNum, modulus)
	affineXDenInv := new(big.Int).ModInverse(affineXDen, modulus)
	tInv := new(big.Int).ModInverse(t, modulus)
	if affineXDenInv == nil || tInv == nil {
		return gnarkte.Point{}, errors.New("encode_to_curve encountered non-invertible denominator")
	}
	affineX := new(big.Int).Mul(affineXNum, affineXDenInv)
	affineX.Mod(affineX, modulus)
	affineY := new(big.Int).Mul(affineYNum, tInv)
	affineY.Mod(affineY, modulus)

	return gnarkte.Point{
		X: affineX,
		Y: affineY,
	}, nil
}

func Decaf377CompressToFieldNative(point gnarkte.Point) (*big.Int, error) {
	constants, err := loadDecaf377Constants()
	if err != nil {
		return nil, err
	}

	modulus := ScalarField()
	x := new(big.Int).Set(point.X.(*big.Int))
	y := new(big.Int).Set(point.Y.(*big.Int))
	t := new(big.Int).Mul(x, y)
	t.Mod(t, modulus)

	xPlusT := new(big.Int).Add(x, t)
	xPlusT.Mod(xPlusT, modulus)
	xMinusT := new(big.Int).Sub(x, t)
	xMinusT.Mod(xMinusT, modulus)
	u1 := new(big.Int).Mul(xPlusT, xMinusT)
	u1.Mod(u1, modulus)

	xSquared := new(big.Int).Mul(x, x)
	xSquared.Mod(xSquared, modulus)
	den := new(big.Int).Mul(u1, constants.aMinusD)
	den.Mod(den, modulus)
	den.Mul(den, xSquared)
	den.Mod(den, modulus)

	_, v, err := decaf377SqrtRatioZetaNative(den)
	if err != nil {
		return nil, err
	}
	u2 := decaf377AbsNative(new(big.Int).Mul(v, u1))
	u2.Mod(u2, modulus)
	u3 := new(big.Int).Sub(u2, t)
	u3.Mod(u3, modulus)

	out := new(big.Int).Mul(constants.aMinusD, v)
	out.Mod(out, modulus)
	out.Mul(out, u3)
	out.Mod(out, modulus)
	out.Mul(out, x)
	out.Mod(out, modulus)
	return decaf377AbsNative(out), nil
}

func decaf377SqrtRatioZetaNative(den *big.Int) (bool, *big.Int, error) {
	constants, err := loadDecaf377Constants()
	if err != nil {
		return false, nil, err
	}
	var denEl, invDen, y, zetaInvDen, zero fr.Element
	denEl.SetBigInt(den)
	if denEl.Equal(&zero) {
		return false, big.NewInt(0), nil
	}
	invDen.Inverse(&denEl)
	if y.Sqrt(&invDen) != nil {
		out := new(big.Int)
		y.BigInt(out)
		return true, out, nil
	}
	var zeta fr.Element
	zeta.SetBigInt(constants.zeta)
	zetaInvDen.Mul(&zeta, &invDen)
	if y.Sqrt(&zetaInvDen) == nil {
		return false, nil, errors.New("sqrt_ratio_zeta produced no square root")
	}
	out := new(big.Int)
	y.BigInt(out)
	return false, out, nil
}

func decaf377AbsNative(value *big.Int) *big.Int {
	modulus := ScalarField()
	v := new(big.Int).Mod(new(big.Int).Set(value), modulus)
	if v.Bit(0) == 0 {
		return v
	}
	return v.Neg(v).Mod(v, modulus)
}
