package circuits

import (
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

func NullifierFromFixtureNative(fixture SpendFixture) (*big.Int, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash3Native(
		MustBigInt(vectors.Poseidon377.NullifierDomain),
		[3]*big.Int{
			MustBigInt(fixture.Private.NK),
			MustBigInt(fixture.Private.StateCommitmentProof.Commitment),
			new(big.Int).SetUint64(fixture.Private.StateCommitmentProof.Position),
		},
	)
}

func Nullifier(
	api frontend.API,
	nk frontend.Variable,
	stateCommitment frontend.Variable,
	position frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash3(
		api,
		MustBigInt(vectors.Poseidon377.NullifierDomain),
		[3]frontend.Variable{nk, stateCommitment, position},
	)
}

func IncomingViewingKeyReductionNative(fixture SpendFixture) (*big.Int, uint64, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, 0, err
	}
	akFq, err := Decaf377CompressToFieldNative(PointAffineToNative(fixture.Private.AKAffine))
	if err != nil {
		return nil, 0, err
	}
	ivkModQ, err := Poseidon377Hash2Native(
		MustBigInt(vectors.Poseidon377.IVKDomain),
		[2]*big.Int{MustBigInt(fixture.Private.NK), akFq},
	)
	if err != nil {
		return nil, 0, err
	}
	rModulus := MustBigInt(vectors.Decaf377CompanionCurve.Order)
	ivkModR := new(big.Int).Mod(new(big.Int).Set(ivkModQ), rModulus)
	quotient := new(big.Int).Sub(ivkModQ, ivkModR)
	quotient.Div(quotient, rModulus)
	return ivkModR, quotient.Uint64(), nil
}

func DiversifiedTransmissionKeyNative(fixture SpendFixture) (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	ivk, _, err := IncomingViewingKeyReductionNative(fixture)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return ScalarMulNative(
		PointAffineToNative(fixture.Private.DiversifiedGeneratorAffine),
		ivk,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
}

func IncomingViewingKey(
	api frontend.API,
	nk frontend.Variable,
	ak gnarkte.Point,
	ivkReduced frontend.Variable,
	quotientA frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	akFq, err := Decaf377CompressToField(api, ak)
	if err != nil {
		return nil, err
	}
	ivkModQ, err := Poseidon377Hash2(
		api,
		MustBigInt(vectors.Poseidon377.IVKDomain),
		[2]frontend.Variable{nk, akFq},
	)
	if err != nil {
		return nil, err
	}

	rModulus := MustBigInt(vectors.Decaf377CompanionCurve.Order)
	api.AssertIsEqual(ivkModQ, api.Add(api.Mul(rModulus, quotientA), ivkReduced))

	// a(a-1)(a-2)(a-3)(a-4) = 0
	poly := quotientA
	for i := 1; i <= 4; i++ {
		poly = api.Mul(poly, api.Sub(quotientA, i))
	}
	api.AssertIsEqual(poly, 0)

	isLess, err := IsLessThanConstant(api, ivkReduced, rModulus)
	if err != nil {
		return nil, err
	}
	api.AssertIsEqual(isLess, 1)

	qMinus4R := new(big.Int).Sub(ScalarField(), new(big.Int).Mul(big.NewInt(4), rModulus))
	isLessThanQMinus4R, err := IsLessThanConstant(api, ivkReduced, qMinus4R)
	if err != nil {
		return nil, err
	}
	isA4 := api.IsZero(api.Sub(quotientA, 4))
	api.AssertIsEqual(api.Mul(isA4, api.Sub(1, isLessThanQMinus4R)), 0)

	return ivkReduced, nil
}

func DiversifiedTransmissionKey(
	api frontend.API,
	nk frontend.Variable,
	ak gnarkte.Point,
	diversifiedGenerator gnarkte.Point,
	ivkReduced frontend.Variable,
	quotientA frontend.Variable,
) (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	ivk, err := IncomingViewingKey(api, nk, ak, ivkReduced, quotientA)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return ScalarMulLE(
		api,
		curve,
		diversifiedGenerator,
		ivk,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	), nil
}

func RandomizedVerificationKeyNative(fixture SpendFixture) (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}

	ak := PointAffineToNative(fixture.Private.AKAffine)
	generator := gnarkte.Point{
		X: MustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
		Y: MustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
	}
	randomizedPart, err := ScalarMulNative(
		generator,
		MustBigInt(fixture.Private.SpendAuthRandomizer),
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return PointAddNative(ak, randomizedPart)
}

func RandomizedVerificationKey(
	api frontend.API,
	ak gnarkte.Point,
	spendAuthRandomizer frontend.Variable,
) (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	generator := gnarkte.Point{
		X: MustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
		Y: MustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
	}
	randomizedPart := ScalarMulLE(
		api,
		curve,
		generator,
		spendAuthRandomizer,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	return curve.Add(ak, randomizedPart), nil
}
