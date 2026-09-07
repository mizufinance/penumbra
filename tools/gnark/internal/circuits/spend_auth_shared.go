package circuits

import (
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

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

func IncomingViewingKey(
	api frontend.API,
	nk frontend.Variable,
	ak gnarkte.Point,
	ivkReduced frontend.Variable,
	quotientA frontend.Variable,
) (frontend.Variable, error) {
	ivk, _, err := incomingViewingKeyWithBits(api, nk, ak, ivkReduced, quotientA)
	return ivk, err
}

// incomingViewingKeyWithBits is IncomingViewingKey plus the 253-bit
// decomposition already produced by IVKModRDecomposition, so callers that
// need ivkReduced's bits (T1-h: DiversifiedTransmissionKey's ladder) don't
// pay for a second api.ToBinary of the same wire.
func incomingViewingKeyWithBits(
	api frontend.API,
	nk frontend.Variable,
	ak gnarkte.Point,
	ivkReduced frontend.Variable,
	quotientA frontend.Variable,
) (frontend.Variable, []frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, nil, err
	}

	akFq, err := decafgnark.CompressToField(api, ak)
	if err != nil {
		return nil, nil, err
	}
	ivkModQ, err := Poseidon377Hash2(
		api,
		MustBigInt(vectors.Poseidon377.IVKDomain),
		[2]frontend.Variable{nk, akFq},
	)
	if err != nil {
		return nil, nil, err
	}

	bits, err := IVKModRDecomposition(api, ivkModQ, quotientA, ivkReduced)
	if err != nil {
		return nil, nil, err
	}

	return ivkReduced, bits, nil
}

// IVKModRDecomposition pins ivkModQ = r*quotientA + ivkReduced with
// quotientA in {0..4}, ivkReduced < r, and (when quotientA = 4)
// ivkReduced < q - 4r so the split cannot wrap mod q. Returns the 253-bit
// little-endian decomposition of ivkReduced so callers can reuse it (T1-h)
// instead of re-decomposing the same wire.
func IVKModRDecomposition(
	api frontend.API,
	ivkModQ frontend.Variable,
	quotientA frontend.Variable,
	ivkReduced frontend.Variable,
) ([]frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	rModulus := MustBigInt(vectors.Decaf377CompanionCurve.Order)
	api.AssertIsEqual(ivkModQ, api.Add(api.Mul(rModulus, quotientA), ivkReduced))

	poly := quotientA
	for i := 1; i <= 4; i++ {
		poly = api.Mul(poly, api.Sub(quotientA, i))
	}
	api.AssertIsEqual(poly, 0)

	bits := api.ToBinary(ivkReduced, 253)
	isLess := LessThanConstant253(api, bits, rModulus)
	api.AssertIsEqual(isLess, 1)

	qMinus4R := new(big.Int).Sub(ScalarField(), new(big.Int).Mul(big.NewInt(4), rModulus))
	isLessThanQMinus4R := LessThanConstant253(api, bits, qMinus4R)
	isA4 := api.IsZero(api.Sub(quotientA, 4))
	api.AssertIsEqual(api.Mul(isA4, api.Sub(1, isLessThanQMinus4R)), 0)

	return bits, nil
}

// AssertIncomingViewingKeyNonzero rejects the zero reduced IVK before it can
// collapse every diversified transmission key to the Decaf identity.
func AssertIncomingViewingKeyNonzero(
	api frontend.API,
	ivkReduced frontend.Variable,
) {
	api.AssertIsDifferent(ivkReduced, 0)
}

// DiversifiedTransmissionKey computes ivkReduced * diversifiedGenerator.
//
// T1-h: LessThanConstant253(bits, r) inside IVKModRDecomposition forces
// ivkReduced < r < 2^251 (Decaf377CompanionCurve.Order.BitLen() == 251) in
// any satisfying assignment, so bits[251] and bits[252] of the 253-bit
// decomposition are always 0 — a 251-bit ladder over bits[0:251] equals the
// full ivkReduced*G. Reusing those bits here (instead of a second
// api.ToBinary(ivk, 251) inside ScalarMulLE) removes one redundant 251-bit
// decomposition per DTK instance.
func DiversifiedTransmissionKey(
	api frontend.API,
	nk frontend.Variable,
	ak gnarkte.Point,
	diversifiedGenerator gnarkte.Point,
	ivkReduced frontend.Variable,
	quotientA frontend.Variable,
) (gnarkte.Point, error) {
	AssertIncomingViewingKeyNonzero(api, ivkReduced)
	return diversifiedTransmissionKeyAfterIvkNonzero(
		api,
		nk,
		ak,
		diversifiedGenerator,
		ivkReduced,
		quotientA,
	)
}

// diversifiedTransmissionKeyAfterIvkNonzero is the exact DTK gadget body used
// after callers expose the shared nonzero-IVK assertion as its own trace row.
func diversifiedTransmissionKeyAfterIvkNonzero(
	api frontend.API,
	nk frontend.Variable,
	ak gnarkte.Point,
	diversifiedGenerator gnarkte.Point,
	ivkReduced frontend.Variable,
	quotientA frontend.Variable,
) (gnarkte.Point, error) {
	point, _, err := diversifiedTransmissionKeyAndBitsAfterIvkNonzero(
		api, nk, ak, diversifiedGenerator, ivkReduced, quotientA,
	)
	return point, err
}

func diversifiedTransmissionKeyAndBitsAfterIvkNonzero(
	api frontend.API,
	nk frontend.Variable,
	ak gnarkte.Point,
	diversifiedGenerator gnarkte.Point,
	ivkReduced frontend.Variable,
	quotientA frontend.Variable,
) (gnarkte.Point, []frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, nil, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, nil, err
	}
	_, ivkBits, err := incomingViewingKeyWithBits(api, nk, ak, ivkReduced, quotientA)
	if err != nil {
		return gnarkte.Point{}, nil, err
	}
	order := MustBigInt(vectors.Decaf377CompanionCurve.Order)
	point := ScalarMulWindow2LEBits(
		api,
		curve,
		diversifiedGenerator,
		ivkBits[:order.BitLen()],
	)
	return point, ivkBits[:order.BitLen()], nil
}

// RegulatedNullifierKey derives the address-scoped key shared by the wallet
// IVK and Orbis's diversified ring public key.
func RegulatedNullifierKey(
	api frontend.API,
	ivkBits []frontend.Variable,
	rnkDhPk gnarkte.Point,
	diversifiedGeneratorFq frontend.Variable,
	transmissionKeyFq frontend.Variable,
	assetID frontend.Variable,
	ringPk gnarkte.Point,
) (frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return nil, err
	}
	curve.AssertIsOnCurve(rnkDhPk)
	curve.AssertIsOnCurve(ringPk)
	AssertDecafNonIdentity(api, rnkDhPk)
	AssertDecafNonIdentity(api, ringPk)
	shared := ScalarMulWindow2LEBits(api, curve, rnkDhPk, ivkBits)
	AssertDecafNonIdentity(api, shared)
	sharedFq, err := decafgnark.CompressToField(api, shared)
	if err != nil {
		return nil, err
	}
	ringPkFq, err := decafgnark.CompressToField(api, ringPk)
	if err != nil {
		return nil, err
	}
	return Poseidon377Hash5(
		api,
		MustBigInt(vectors.Poseidon377.ComplianceNullifierDerivationDomain),
		[5]frontend.Variable{
			sharedFq,
			diversifiedGeneratorFq,
			transmissionKeyFq,
			assetID,
			ringPkFq,
		},
	)
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
