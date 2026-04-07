package abi

import (
	"fmt"
	"strings"

	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

func CrossCheckRandomizedVerificationKeyWitnessV1(payload []byte) (string, error) {
	witness, err := DecodeSpendWitnessV1(payload)
	if err != nil {
		return "", err
	}
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return "", err
	}

	ak := gnarkte.Point{
		X: LittleEndianBytesToBigInt(witness.AKAffine.X[:]),
		Y: LittleEndianBytesToBigInt(witness.AKAffine.Y[:]),
	}
	generator := gnarkte.Point{
		X: MustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
		Y: MustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
	}
	randomizer := LittleEndianBytesToBigInt(witness.SpendAuthRandomizer[:])
	randomizedPart, err := ScalarMulNative(
		generator,
		randomizer,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	if err != nil {
		return "", err
	}
	rkNative, err := PointAddNative(ak, randomizedPart)
	if err != nil {
		return "", err
	}

	var out strings.Builder
	fmt.Fprintf(&out, "crosscheck.rk.randomizer=%s\n", randomizer.String())
	fmt.Fprintf(&out, "crosscheck.rk.ak.x=%s\n", ak.X)
	fmt.Fprintf(&out, "crosscheck.rk.ak.y=%s\n", ak.Y)
	fmt.Fprintf(&out, "crosscheck.rk.generator.x=%s\n", generator.X)
	fmt.Fprintf(&out, "crosscheck.rk.generator.y=%s\n", generator.Y)
	fmt.Fprintf(&out, "crosscheck.rk.randomized_part.x=%s\n", randomizedPart.X)
	fmt.Fprintf(&out, "crosscheck.rk.randomized_part.y=%s\n", randomizedPart.Y)
	fmt.Fprintf(&out, "crosscheck.rk.expected.x=%s\n", LittleEndianBytesToBigInt(witness.RKAffine.X[:]))
	fmt.Fprintf(&out, "crosscheck.rk.expected.y=%s\n", LittleEndianBytesToBigInt(witness.RKAffine.Y[:]))
	fmt.Fprintf(&out, "crosscheck.rk.native.x=%s\n", rkNative.X)
	fmt.Fprintf(&out, "crosscheck.rk.native.y=%s\n", rkNative.Y)
	return out.String(), nil
}
