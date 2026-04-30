package compliance

import (
	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

func DeriveSharedSecretsSpend(
	api frontend.API,
	esk frontend.Variable,
	ackCore gnarkte.Point,
	dkPub gnarkte.Point,
	isFlagged frontend.Variable,
	publishedEPK gnarkte.Point,
) (gnarkte.Point, gnarkte.Point, gnarkte.Point, error) {
	api.AssertIsBoolean(isFlagged)
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, err
	}
	generator, err := decafGeneratorPoint()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, err
	}
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, err
	}
	nBits := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()

	computedEPK := ScalarMulLE(api, curve, generator, esk, nBits)
	decafgnark.AssertEquivalent(api, computedEPK, publishedEPK)

	ssCoreUser := ScalarMulLE(api, curve, ackCore, esk, nBits)
	ssIssuer := ScalarMulLE(api, curve, dkPub, esk, nBits)
	ssCore := gnarkte.Point{
		X: api.Select(isFlagged, ssIssuer.X, ssCoreUser.X),
		Y: api.Select(isFlagged, ssIssuer.Y, ssCoreUser.Y),
	}
	return ssIssuer, ssCoreUser, ssCore, nil
}
