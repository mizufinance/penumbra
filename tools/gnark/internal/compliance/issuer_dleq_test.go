package compliance

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

type issuerDLEQCircuit struct {
	AssetID             frontend.Variable
	IssuerDKPub         twistededwards.Point
	EphemeralPublicKey  twistededwards.Point
	SharedPoint         twistededwards.Point
	CommitmentGenerator twistededwards.Point
	CommitmentEPK       twistededwards.Point
	Response            frontend.Variable
}

func (c *issuerDLEQCircuit) Define(api frontend.API) error {
	return VerifyIssuerDhDLEQ(
		api,
		c.AssetID,
		c.IssuerDKPub,
		c.EphemeralPublicKey,
		c.SharedPoint,
		c.CommitmentGenerator,
		c.CommitmentEPK,
		c.Response,
	)
}

func issuerDLEQPoint(t *testing.T, generator twistededwards.Point, scalar *big.Int) twistededwards.Point {
	t.Helper()
	point, err := decafgnark.ScalarMulNative(generator, scalar, 251)
	if err != nil {
		t.Fatal(err)
	}
	return point
}

func issuerDLEQAssignment(t *testing.T) *issuerDLEQCircuit {
	t.Helper()
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatal(err)
	}
	generator := twistededwards.Point{
		X: primitives.MustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
		Y: primitives.MustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
	}
	order := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order)
	secret := big.NewInt(17)
	ephemeralSecret := big.NewInt(23)
	nonce := big.NewInt(29)
	assetID := big.NewInt(31)
	issuerDKPub := issuerDLEQPoint(t, generator, secret)
	ephemeralPublicKey := issuerDLEQPoint(t, generator, ephemeralSecret)
	sharedPoint := issuerDLEQPoint(t, ephemeralPublicKey, secret)
	commitmentGenerator := issuerDLEQPoint(t, generator, nonce)
	commitmentEPK := issuerDLEQPoint(t, ephemeralPublicKey, nonce)

	compress := func(point twistededwards.Point) *big.Int {
		value, err := decafgnark.CompressToFieldNative(point)
		if err != nil {
			t.Fatal(err)
		}
		return value
	}
	challengeFq, err := primitives.Poseidon377Hash7Native(
		issuerDhDLEQDomain,
		[7]*big.Int{
			assetID,
			compress(generator),
			compress(issuerDKPub),
			compress(ephemeralPublicKey),
			compress(sharedPoint),
			compress(commitmentGenerator),
			compress(commitmentEPK),
		},
	)
	if err != nil {
		t.Fatal(err)
	}
	challenge := new(big.Int).And(
		challengeFq,
		new(big.Int).Sub(
			new(big.Int).Lsh(big.NewInt(1), DLEQChallengeBits),
			big.NewInt(1),
		),
	)
	response := new(big.Int).Mul(challenge, secret)
	response.Add(response, nonce)
	response.Mod(response, order)

	return &issuerDLEQCircuit{
		AssetID:             assetID,
		IssuerDKPub:         issuerDKPub,
		EphemeralPublicKey:  ephemeralPublicKey,
		SharedPoint:         sharedPoint,
		CommitmentGenerator: commitmentGenerator,
		CommitmentEPK:       commitmentEPK,
		Response:            response,
	}
}

func TestIssuerDhDLEQAcceptsExactRelationAndRejectsMutations(t *testing.T) {
	valid := issuerDLEQAssignment(t)
	if err := test.IsSolved(&issuerDLEQCircuit{}, valid, ecc.BLS12_377.ScalarField()); err != nil {
		t.Fatal(err)
	}

	mutations := []func(*issuerDLEQCircuit){
		func(value *issuerDLEQCircuit) { value.AssetID = big.NewInt(32) },
		func(value *issuerDLEQCircuit) {
			value.Response = new(big.Int).Add(value.Response.(*big.Int), big.NewInt(1))
		},
		func(value *issuerDLEQCircuit) { value.SharedPoint = value.IssuerDKPub },
	}
	for index, mutate := range mutations {
		assignment := *valid
		mutate(&assignment)
		if err := test.IsSolved(
			&issuerDLEQCircuit{},
			&assignment,
			ecc.BLS12_377.ScalarField(),
		); err == nil {
			t.Fatalf("mutation %d must fail", index)
		}
	}
}
