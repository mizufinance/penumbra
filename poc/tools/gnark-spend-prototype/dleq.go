package prototype

import (
	"errors"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
)

func dleqChallengeDomain() *big.Int {
	// Rust uses Fq::from_le_bytes_mod_order on this 24-byte ASCII string.
	// Since the input is shorter than the field modulus, this is just the
	// little-endian integer interpretation.
	input := []byte("elgamal-encrypt-proof-v1")
	reversed := make([]byte, len(input))
	for i := range input {
		reversed[len(input)-1-i] = input[i]
	}
	return new(big.Int).SetBytes(reversed)
}

func decafGeneratorPoint() (gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	return gnarkte.Point{
		X: mustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
		Y: mustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
	}, nil
}

func pointSub(curve gnarkte.Curve, left, right gnarkte.Point) gnarkte.Point {
	return curve.Add(left, curve.Neg(right))
}

func scalarMulLE(api frontend.API, curve gnarkte.Curve, base gnarkte.Point, scalar frontend.Variable, nBits int) gnarkte.Point {
	bits := api.ToBinary(scalar, nBits)
	result := gnarkte.Point{X: 0, Y: 1}
	current := base

	for _, bit := range bits {
		sum := curve.Add(result, current)
		result = gnarkte.Point{
			X: api.Select(bit, sum.X, result.X),
			Y: api.Select(bit, sum.Y, result.Y),
		}
		current = curve.Double(current)
	}

	return result
}

func assertEqualIf(api frontend.API, left, right, cond frontend.Variable) {
	api.AssertIsEqual(api.Mul(api.Sub(left, right), cond), 0)
}

// VerifyDLEQ mirrors Penumbra's verify_dleq_r1cs gadget for a single tier.
func VerifyDLEQ(
	api frontend.API,
	r frontend.Variable,
	ack gnarkte.Point,
	epk gnarkte.Point,
	metadataHash frontend.Variable,
	publishedC frontend.Variable,
	publishedS frontend.Variable,
	isRegulated frontend.Variable,
) error {
	api.AssertIsBoolean(isRegulated)

	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return err
	}
	curve.AssertIsOnCurve(ack)
	curve.AssertIsOnCurve(epk)

	generator, err := decafGeneratorPoint()
	if err != nil {
		return err
	}
	curve.AssertIsOnCurve(generator)

	vectors, err := loadPrototypeVectors()
	if err != nil {
		return err
	}
	orderBitLen := mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()

	sPoint := scalarMulLE(api, curve, ack, r, orderBitLen)

	sTimesG := scalarMulLE(api, curve, generator, publishedS, orderBitLen)
	cTimesEpk := scalarMulLE(api, curve, epk, publishedC, orderBitLen)
	rRec := pointSub(curve, sTimesG, cTimesEpk)

	sTimesAck := scalarMulLE(api, curve, ack, publishedS, orderBitLen)
	cTimesS := scalarMulLE(api, curve, sPoint, publishedC, orderBitLen)
	rpRec := pointSub(curve, sTimesAck, cTimesS)

	ackFq, err := Decaf377CompressToField(api, ack)
	if err != nil {
		return err
	}
	epkFq, err := Decaf377CompressToField(api, epk)
	if err != nil {
		return err
	}
	sFq, err := Decaf377CompressToField(api, sPoint)
	if err != nil {
		return err
	}
	rFq, err := Decaf377CompressToField(api, rRec)
	if err != nil {
		return err
	}
	rpFq, err := Decaf377CompressToField(api, rpRec)
	if err != nil {
		return err
	}

	challenge, err := Poseidon377Hash7(api, dleqChallengeDomain(), [7]frontend.Variable{
		metadataHash,
		mustBigInt(vectors.Decaf377CompanionCurve.GeneratorCompressToField),
		ackFq,
		epkFq,
		sFq,
		rFq,
		rpFq,
	})
	if err != nil {
		return err
	}

	keepBits := vectors.DleqFixture.ChallengeKeepBits
	if keepBits <= 0 {
		return errors.New("invalid DLEQ challenge bit length")
	}

	cBits := api.ToBinary(publishedC)
	computedBits := api.ToBinary(challenge)

	for _, bit := range cBits[keepBits:] {
		api.AssertIsEqual(api.Mul(bit, isRegulated), 0)
	}
	for i := 0; i < keepBits; i++ {
		assertEqualIf(api, computedBits[i], cBits[i], isRegulated)
	}

	return nil
}
