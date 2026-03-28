package prototype

import (
	"encoding/hex"
	"fmt"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
)

func pointAffineToNative(point pointAffineFixture) gnarkte.Point {
	return gnarkte.Point{
		X: mustBigInt(point.X),
		Y: mustBigInt(point.Y),
	}
}

func compressedLEHexToBigInt(value string) (*big.Int, error) {
	bytes, err := hex.DecodeString(value)
	if err != nil {
		return nil, fmt.Errorf("decode hex %q: %w", value, err)
	}
	if len(bytes) != 32 {
		return nil, fmt.Errorf("expected 32 bytes, got %d", len(bytes))
	}
	reversed := make([]byte, len(bytes))
	for i := range bytes {
		reversed[len(bytes)-1-i] = bytes[i]
	}
	return new(big.Int).SetBytes(reversed), nil
}

func NoteCommitmentFromFixtureNative(fixture spendFixture) (*big.Int, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	diversifiedGenerator := pointAffineToNative(fixture.Private.DiversifiedGeneratorAffine)
	diversifiedGeneratorFq, err := Decaf377CompressToFieldNative(diversifiedGenerator)
	if err != nil {
		return nil, err
	}
	transmissionKeyS, err := compressedLEHexToBigInt(fixture.Private.TransmissionKeyHex)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash6Native(
		mustBigInt(vectors.Poseidon377.NoteCommitDomain),
		[6]*big.Int{
			mustBigInt(fixture.Private.NoteBlinding),
			mustBigInt(fixture.Private.NoteAmount),
			mustBigInt(fixture.Private.NoteAssetID),
			diversifiedGeneratorFq,
			transmissionKeyS,
			mustBigInt(fixture.Private.ClueKey),
		},
	)
}

func NullifierFromFixtureNative(fixture spendFixture) (*big.Int, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash3Native(
		mustBigInt(vectors.Poseidon377.NullifierDomain),
		[3]*big.Int{
			mustBigInt(fixture.Private.NK),
			mustBigInt(fixture.Private.StateCommitmentProof.Commitment),
			new(big.Int).SetUint64(fixture.Private.StateCommitmentProof.Position),
		},
	)
}

func NoteCommitment(
	api frontend.API,
	noteBlinding frontend.Variable,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	diversifiedGenerator gnarkte.Point,
	transmissionKeyS frontend.Variable,
	clueKey frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	diversifiedGeneratorFq, err := Decaf377CompressToField(api, diversifiedGenerator)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash6(
		api,
		mustBigInt(vectors.Poseidon377.NoteCommitDomain),
		[6]frontend.Variable{
			noteBlinding,
			noteAmount,
			noteAssetID,
			diversifiedGeneratorFq,
			transmissionKeyS,
			clueKey,
		},
	)
}

func Nullifier(
	api frontend.API,
	nk frontend.Variable,
	stateCommitment frontend.Variable,
	position frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash3(
		api,
		mustBigInt(vectors.Poseidon377.NullifierDomain),
		[3]frontend.Variable{nk, stateCommitment, position},
	)
}

func IncomingViewingKeyReductionNative(fixture spendFixture) (*big.Int, uint64, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, 0, err
	}
	akFq, err := Decaf377CompressToFieldNative(pointAffineToNative(fixture.Private.AKAffine))
	if err != nil {
		return nil, 0, err
	}
	ivkModQ, err := Poseidon377Hash2Native(
		mustBigInt(vectors.Poseidon377.IVKDomain),
		[2]*big.Int{mustBigInt(fixture.Private.NK), akFq},
	)
	if err != nil {
		return nil, 0, err
	}
	rModulus := mustBigInt(vectors.Decaf377CompanionCurve.Order)
	ivkModR := new(big.Int).Mod(new(big.Int).Set(ivkModQ), rModulus)
	quotient := new(big.Int).Sub(ivkModQ, ivkModR)
	quotient.Div(quotient, rModulus)
	return ivkModR, quotient.Uint64(), nil
}

func DiversifiedTransmissionKeyNative(fixture spendFixture) (gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	ivk, _, err := IncomingViewingKeyReductionNative(fixture)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return scalarMulNative(
		pointAffineToNative(fixture.Private.DiversifiedGeneratorAffine),
		ivk,
		mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
}

func IncomingViewingKey(
	api frontend.API,
	nk frontend.Variable,
	ak gnarkte.Point,
	ivkReduced frontend.Variable,
	quotientA frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	akFq, err := Decaf377CompressToField(api, ak)
	if err != nil {
		return nil, err
	}
	ivkModQ, err := Poseidon377Hash2(
		api,
		mustBigInt(vectors.Poseidon377.IVKDomain),
		[2]frontend.Variable{nk, akFq},
	)
	if err != nil {
		return nil, err
	}

	rModulus := mustBigInt(vectors.Decaf377CompanionCurve.Order)
	api.AssertIsEqual(ivkModQ, api.Add(api.Mul(rModulus, quotientA), ivkReduced))

	// a(a-1)(a-2)(a-3)(a-4) = 0
	poly := quotientA
	for i := 1; i <= 4; i++ {
		poly = api.Mul(poly, api.Sub(quotientA, i))
	}
	api.AssertIsEqual(poly, 0)

	isLess, err := isLessThanConstant(api, ivkReduced, rModulus)
	if err != nil {
		return nil, err
	}
	api.AssertIsEqual(isLess, 1)

	qMinus4R := new(big.Int).Sub(ScalarField(), new(big.Int).Mul(big.NewInt(4), rModulus))
	isLessThanQMinus4R, err := isLessThanConstant(api, ivkReduced, qMinus4R)
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
	vectors, err := loadPrototypeVectors()
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
	return scalarMulLE(
		api,
		curve,
		diversifiedGenerator,
		ivk,
		mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	), nil
}

func ComplianceLeafCommitmentFromFixtureNative(fixture spendFixture) (*big.Int, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	diversifiedGeneratorFq, err := Decaf377CompressToFieldNative(
		pointAffineToNative(fixture.Private.UserDiversifiedGeneratorAffine),
	)
	if err != nil {
		return nil, err
	}
	transmissionKeyFq, err := Decaf377CompressToFieldNative(
		pointAffineToNative(fixture.Private.UserTransmissionKeyAffine),
	)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash4Native(
		mustBigInt(vectors.Poseidon377.ComplianceLeafDomain),
		[4]*big.Int{
			diversifiedGeneratorFq,
			transmissionKeyFq,
			mustBigInt(fixture.Private.NoteAssetID),
			mustBigInt(fixture.Private.UserDDecimal),
		},
	)
}

func ComplianceLeafCommitment(
	api frontend.API,
	diversifiedGenerator gnarkte.Point,
	transmissionKey gnarkte.Point,
	assetID frontend.Variable,
	d frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	diversifiedGeneratorFq, err := Decaf377CompressToField(api, diversifiedGenerator)
	if err != nil {
		return nil, err
	}
	transmissionKeyFq, err := Decaf377CompressToField(api, transmissionKey)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash4(
		api,
		mustBigInt(vectors.Poseidon377.ComplianceLeafDomain),
		[4]frontend.Variable{
			diversifiedGeneratorFq,
			transmissionKeyFq,
			assetID,
			d,
		},
	)
}

func BlindSenderLeafFromFixtureNative(fixture spendFixture) (*big.Int, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	leafCommitment, err := ComplianceLeafCommitmentFromFixtureNative(fixture)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash3Native(
		mustBigInt(vectors.Poseidon377.SenderLeafDomain),
		[3]*big.Int{
			leafCommitment,
			mustBigInt(fixture.Private.TxBlindingNonce),
			big.NewInt(0),
		},
	)
}

func BlindSenderLeaf(
	api frontend.API,
	leafHash frontend.Variable,
	txBlindingNonce frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash3(
		api,
		mustBigInt(vectors.Poseidon377.SenderLeafDomain),
		[3]frontend.Variable{leafHash, txBlindingNonce, 0},
	)
}

func ValueGeneratorNative(assetID *big.Int) (gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	hashedAssetID, err := Poseidon377Hash1Native(mustBigInt(vectors.Poseidon377.ValueGeneratorDomain), assetID)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return Decaf377EncodeToCurveNative(hashedAssetID)
}

func ValueBlindingGeneratorNative() (gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	return gnarkte.Point{
		X: mustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorX),
		Y: mustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorY),
	}, nil
}

func BalanceCommitmentFromFixtureNative(fixture spendFixture) (gnarkte.Point, error) {
	valueGenerator, err := ValueGeneratorNative(mustBigInt(fixture.Private.NoteAssetID))
	if err != nil {
		return gnarkte.Point{}, err
	}
	valueBlindingGenerator, err := ValueBlindingGeneratorNative()
	if err != nil {
		return gnarkte.Point{}, err
	}

	valuePoint, err := scalarMulNative(valueGenerator, mustBigInt(fixture.Private.NoteAmount), 128)
	if err != nil {
		return gnarkte.Point{}, err
	}
	blindingPoint, err := scalarMulNative(valueBlindingGenerator, mustBigInt(fixture.Private.VBlinding), 256)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return pointAddNative(valuePoint, blindingPoint)
}

func BalanceCommitment(
	api frontend.API,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	vBlinding frontend.Variable,
) (gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}

	hashedAssetID, err := Poseidon377Hash1(api, mustBigInt(vectors.Poseidon377.ValueGeneratorDomain), noteAssetID)
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	valueGenerator, err := Decaf377EncodeToCurve(api, hashedAssetID)
	if err != nil {
		return gnarkte.Point{}, err
	}
	valueBlindingGenerator := gnarkte.Point{
		X: mustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorX),
		Y: mustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorY),
	}

	valuePoint := scalarMulLE(api, curve, valueGenerator, noteAmount, 128)
	blindingPoint := scalarMulLE(
		api,
		curve,
		valueBlindingGenerator,
		vBlinding,
		mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	return curve.Add(valuePoint, blindingPoint), nil
}

func RandomizedVerificationKeyNative(fixture spendFixture) (gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}

	ak := pointAffineToNative(fixture.Private.AKAffine)
	generator := gnarkte.Point{
		X: mustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
		Y: mustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
	}
	randomizedPart, err := scalarMulNative(
		generator,
		mustBigInt(fixture.Private.SpendAuthRandomizer),
		mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return pointAddNative(ak, randomizedPart)
}

func RandomizedVerificationKey(
	api frontend.API,
	ak gnarkte.Point,
	spendAuthRandomizer frontend.Variable,
) (gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	generator := gnarkte.Point{
		X: mustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
		Y: mustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
	}
	randomizedPart := scalarMulLE(
		api,
		curve,
		generator,
		spendAuthRandomizer,
		mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	return curve.Add(ak, randomizedPart), nil
}

func pointAddNative(left, right gnarkte.Point) (gnarkte.Point, error) {
	constants, err := loadDecaf377Constants()
	if err != nil {
		return gnarkte.Point{}, err
	}
	modulus := ScalarField()

	x1 := new(big.Int).Set(left.X.(*big.Int))
	y1 := new(big.Int).Set(left.Y.(*big.Int))
	x2 := new(big.Int).Set(right.X.(*big.Int))
	y2 := new(big.Int).Set(right.Y.(*big.Int))

	x1x2 := new(big.Int).Mul(x1, x2)
	x1x2.Mod(x1x2, modulus)
	y1y2 := new(big.Int).Mul(y1, y2)
	y1y2.Mod(y1y2, modulus)
	x1y2 := new(big.Int).Mul(x1, y2)
	x1y2.Mod(x1y2, modulus)
	y1x2 := new(big.Int).Mul(y1, x2)
	y1x2.Mod(y1x2, modulus)
	dxxyy := new(big.Int).Mul(constants.d, x1x2)
	dxxyy.Mod(dxxyy, modulus)
	dxxyy.Mul(dxxyy, y1y2)
	dxxyy.Mod(dxxyy, modulus)

	xNum := new(big.Int).Add(x1y2, y1x2)
	xNum.Mod(xNum, modulus)
	xDen := new(big.Int).Add(big.NewInt(1), dxxyy)
	xDen.Mod(xDen, modulus)

	yNum := new(big.Int).Add(y1y2, x1x2)
	yNum.Mod(yNum, modulus)
	yDen := new(big.Int).Sub(big.NewInt(1), dxxyy)
	yDen.Mod(yDen, modulus)

	xDenInv := new(big.Int).ModInverse(xDen, modulus)
	yDenInv := new(big.Int).ModInverse(yDen, modulus)
	if xDenInv == nil || yDenInv == nil {
		return gnarkte.Point{}, fmt.Errorf("non-invertible twisted Edwards denominator")
	}

	x3 := new(big.Int).Mul(xNum, xDenInv)
	x3.Mod(x3, modulus)
	y3 := new(big.Int).Mul(yNum, yDenInv)
	y3.Mod(y3, modulus)

	return gnarkte.Point{X: x3, Y: y3}, nil
}

func scalarMulNative(base gnarkte.Point, scalar *big.Int, nBits int) (gnarkte.Point, error) {
	result := gnarkte.Point{X: big.NewInt(0), Y: big.NewInt(1)}
	current := base

	for i := 0; i < nBits; i++ {
		if scalar.Bit(i) == 1 {
			var err error
			result, err = pointAddNative(result, current)
			if err != nil {
				return gnarkte.Point{}, err
			}
		}
		var err error
		current, err = pointAddNative(current, current)
		if err != nil {
			return gnarkte.Point{}, err
		}
	}
	return result, nil
}

func isLessThanConstant(api frontend.API, value frontend.Variable, constant *big.Int) (frontend.Variable, error) {
	valueBits := api.ToBinary(value, decaf377FieldBits)
	constantBits := make([]uint, decaf377FieldBits)
	for i := 0; i < decaf377FieldBits; i++ {
		constantBits[i] = constant.Bit(i)
	}

	prefixEqual := frontend.Variable(1)
	isLess := frontend.Variable(0)
	for i := decaf377FieldBits - 1; i >= 0; i-- {
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
