package compliance

import (
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

var issuerDhDLEQDomain = littleEndianConstant([]byte("shieldd.issuer.dh_evidence.dleq.v1\x00"))

// DLEQChallengeBits bounds challenge scalars below the Decaf scalar modulus.
const DLEQChallengeBits = 250

func littleEndianConstant(value []byte) *big.Int {
	reversed := make([]byte, len(value))
	for index := range value {
		reversed[len(value)-1-index] = value[index]
	}
	return new(big.Int).SetBytes(reversed)
}

// VerifyIssuerDhDLEQ proves log_G(issuerDKPub) = log_EPK(sharedPoint).
// Its Poseidon transcript is the exact circuit counterpart of IssuerDhEvidence.
func VerifyIssuerDhDLEQ(
	api frontend.API,
	assetID frontend.Variable,
	issuerDKPub gnarkte.Point,
	ephemeralPublicKey gnarkte.Point,
	sharedPoint gnarkte.Point,
	commitmentGenerator gnarkte.Point,
	commitmentEPK gnarkte.Point,
	response frontend.Variable,
) error {
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return err
	}
	generator, err := decafGeneratorPoint()
	if err != nil {
		return err
	}
	for _, point := range []gnarkte.Point{
		generator,
		issuerDKPub,
		ephemeralPublicKey,
		sharedPoint,
		commitmentGenerator,
		commitmentEPK,
	} {
		curve.AssertIsOnCurve(point)
		api.AssertIsDifferent(point.X, 0)
	}

	compressed := make([]frontend.Variable, 0, 6)
	for _, point := range []gnarkte.Point{
		generator,
		issuerDKPub,
		ephemeralPublicKey,
		sharedPoint,
		commitmentGenerator,
		commitmentEPK,
	} {
		value, err := decafgnark.CompressToField(api, point)
		if err != nil {
			return err
		}
		compressed = append(compressed, value)
	}
	challengeFq, err := primitives.Poseidon377Hash7(
		api,
		issuerDhDLEQDomain,
		[7]frontend.Variable{
			assetID,
			compressed[0],
			compressed[1],
			compressed[2],
			compressed[3],
			compressed[4],
			compressed[5],
		},
	)
	if err != nil {
		return err
	}

	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return err
	}
	order := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order)
	responseBits := api.ToBinary(response, decafgnark.FieldBits)
	api.AssertIsEqual(LessThanConstant253(api, responseBits, order), 1)
	challengeBits := api.ToBinary(challengeFq, decafgnark.FieldBits)
	challengeBits = challengeBits[:DLEQChallengeBits]

	responseG := ScalarMulWindow2LEBits(api, curve, generator, responseBits[:order.BitLen()])
	challengeDK := ScalarMulWindow2LEBits(api, curve, issuerDKPub, challengeBits)
	expectedG := curve.Add(commitmentGenerator, challengeDK)
	api.AssertIsEqual(responseG.X, expectedG.X)
	api.AssertIsEqual(responseG.Y, expectedG.Y)

	responseEPK := ScalarMulWindow2LEBits(
		api,
		curve,
		ephemeralPublicKey,
		responseBits[:order.BitLen()],
	)
	challengeShared := ScalarMulWindow2LEBits(api, curve, sharedPoint, challengeBits)
	expectedEPK := curve.Add(commitmentEPK, challengeShared)
	api.AssertIsEqual(responseEPK.X, expectedEPK.X)
	api.AssertIsEqual(responseEPK.Y, expectedEPK.Y)
	return nil
}
