package compliance

import (
	"errors"
	"fmt"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
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
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	x, ok := new(big.Int).SetString(vectors.Decaf377CompanionCurve.GeneratorX, 10)
	if !ok {
		return gnarkte.Point{}, fmt.Errorf("invalid decaf generator x %q", vectors.Decaf377CompanionCurve.GeneratorX)
	}
	y, ok := new(big.Int).SetString(vectors.Decaf377CompanionCurve.GeneratorY, 10)
	if !ok {
		return gnarkte.Point{}, fmt.Errorf("invalid decaf generator y %q", vectors.Decaf377CompanionCurve.GeneratorY)
	}
	return gnarkte.Point{
		X: x,
		Y: y,
	}, nil
}

func pointSub(curve gnarkte.Curve, left, right gnarkte.Point) gnarkte.Point {
	return curve.Add(left, curve.Neg(right))
}

func ScalarMulLE(api frontend.API, curve gnarkte.Curve, base gnarkte.Point, scalar frontend.Variable, nBits int) gnarkte.Point {
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

func AssertEqualIf(api frontend.API, left, right, cond frontend.Variable) {
	api.AssertIsEqual(api.Mul(api.Sub(left, right), cond), 0)
}

// VerifyDLEQ mirrors Penumbra's verify_dleq_r1cs gadget for a single tier.
func VerifyDLEQ(
	api frontend.API,
	r frontend.Variable,
	ack gnarkte.Point,
	sPoint gnarkte.Point,
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

	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return err
	}
	orderBitLen := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()
	_ = orderBitLen

	rRec := curve.DoubleBaseScalarMul(generator, curve.Neg(epk), publishedS, publishedC)

	rpRec := curve.DoubleBaseScalarMul(ack, curve.Neg(sPoint), publishedS, publishedC)

	ackFq, err := primitives.Decaf377CompressToField(api, ack)
	if err != nil {
		return err
	}
	epkFq, err := primitives.Decaf377CompressToField(api, epk)
	if err != nil {
		return err
	}
	sFq, err := primitives.Decaf377CompressToField(api, sPoint)
	if err != nil {
		return err
	}
	rFq, err := primitives.Decaf377CompressToField(api, rRec)
	if err != nil {
		return err
	}
	rpFq, err := primitives.Decaf377CompressToField(api, rpRec)
	if err != nil {
		return err
	}

	challenge, err := primitives.Poseidon377Hash7(api, dleqChallengeDomain(), [7]frontend.Variable{
		metadataHash,
		primitives.MustBigInt(vectors.Decaf377CompanionCurve.GeneratorCompressToField),
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
	if keepBits > primitives.Decaf377FieldBits {
		return fmt.Errorf("invalid DLEQ challenge bit length %d for field bit length %d", keepBits, primitives.Decaf377FieldBits)
	}

	cBits := api.ToBinary(publishedC, primitives.Decaf377FieldBits)
	computedBits := api.ToBinary(challenge, primitives.Decaf377FieldBits)

	for _, bit := range cBits[keepBits:] {
		api.AssertIsEqual(api.Mul(bit, isRegulated), 0)
	}
	for i := 0; i < keepBits; i++ {
		AssertEqualIf(api, computedBits[i], cBits[i], isRegulated)
	}

	return nil
}
