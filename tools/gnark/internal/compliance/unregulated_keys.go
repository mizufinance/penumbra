package compliance

import (
	"fmt"
	"golang.org/x/crypto/blake2b"
	"math/big"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

func blackHoleACKScalar() (*big.Int, error) {
	hash := blake2b.Sum512([]byte("penumbra.compliance.black_hole_ack"))
	scalar := primitives.LittleEndianBytesToBigInt(hash[:])

	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return nil, fmt.Errorf("load prototype vectors: %w", err)
	}

	order := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order)
	scalar.Mod(scalar, order)
	return scalar, nil
}

func UnregulatedComplianceKeys() (gnarkte.Point, gnarkte.Point, error) {
	generator, err := decafGeneratorPoint()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}

	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}

	blackHoleScalar, err := blackHoleACKScalar()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}

	blackHoleACK, err := primitives.ScalarMulNative(
		generator,
		blackHoleScalar,
		primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, fmt.Errorf("derive black hole ACK: %w", err)
	}

	return blackHoleACK, generator, nil
}

func SelectPoint(
	api frontend.API,
	cond frontend.Variable,
	ifTrue gnarkte.Point,
	ifFalse gnarkte.Point,
) gnarkte.Point {
	return gnarkte.Point{
		X: api.Select(cond, ifTrue.X, ifFalse.X),
		Y: api.Select(cond, ifTrue.Y, ifFalse.Y),
	}
}
