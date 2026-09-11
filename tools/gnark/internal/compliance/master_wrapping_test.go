package compliance

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	decaf377 "github.com/mizufinance/decaf377-go"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

type masterWrappingCircuit struct {
	Scalar, Flagged, Seed, EPK, Wrapping frontend.Variable
	Ring, IssuerShared                   gnarkte.Point
	Position                             int `gnark:"-"`
}

func (c *masterWrappingCircuit) Define(api frontend.API) error {
	return VerifyTransferMasterWrapping(api, api.ToBinary(c.Scalar, 251),
		c.Ring, c.IssuerShared, c.Flagged, c.Seed, c.EPK, c.Position, c.Wrapping)
}

func TestMasterWrappingBindsSeedPositionKeyAndFlag(t *testing.T) {
	generator, err := decaf377.Generator()
	if err != nil {
		t.Fatal(err)
	}
	ring, err := decaf377.ScalarMul(generator, big.NewInt(19))
	if err != nil {
		t.Fatal(err)
	}
	epk, err := decaf377.ScalarMul(generator, big.NewInt(17))
	if err != nil {
		t.Fatal(err)
	}
	issuerShared, err := decaf377.ScalarMul(epk, big.NewInt(23))
	if err != nil {
		t.Fatal(err)
	}
	masterShared, err := decaf377.ScalarMul(ring, big.NewInt(17))
	if err != nil {
		t.Fatal(err)
	}
	epkFq, err := decafgnark.CompressToFieldNative(gnarkte.Point{X: epk.X, Y: epk.Y})
	if err != nil {
		t.Fatal(err)
	}
	for flagged := 0; flagged < 2; flagged++ {
		selected := masterShared
		if flagged == 1 {
			selected = issuerShared
		}
		encoded, err := decafgnark.CompressToFieldNative(gnarkte.Point{X: selected.X, Y: selected.Y})
		if err != nil {
			t.Fatal(err)
		}
		for position := 0; position < 3; position++ {
			mask, err := primitives.Poseidon377Hash3Native(TransferMasterWrappingDomain,
				[3]*big.Int{big.NewInt(int64(position)), encoded, epkFq})
			if err != nil {
				t.Fatal(err)
			}
			wrapping := new(big.Int).Mod(new(big.Int).Add(mask, big.NewInt(29)), primitives.ScalarField())
			assignment := masterWrappingCircuit{
				Scalar: 17, Flagged: flagged, Seed: 29, EPK: epkFq, Wrapping: wrapping,
				Ring: gnarkte.Point{X: ring.X, Y: ring.Y}, IssuerShared: gnarkte.Point{X: issuerShared.X, Y: issuerShared.Y}, Position: position,
			}
			solve := func(a masterWrappingCircuit) error {
				return test.IsSolved(&masterWrappingCircuit{Position: a.Position}, &a, ecc.BLS12_377.ScalarField())
			}
			if err := solve(assignment); err != nil {
				t.Fatal(err)
			}
			for _, mutate := range []func(*masterWrappingCircuit){
				func(a *masterWrappingCircuit) { a.Seed = 30 },
				func(a *masterWrappingCircuit) { a.Position = (position + 1) % 3 },
				func(a *masterWrappingCircuit) { a.Flagged = 1 - flagged },
				func(a *masterWrappingCircuit) { a.Flagged = 2 },
				func(a *masterWrappingCircuit) { a.EPK = 1 },
				func(a *masterWrappingCircuit) { a.Wrapping = new(big.Int).Add(wrapping, big.NewInt(1)) },
			} {
				changed := assignment
				mutate(&changed)
				if err := solve(changed); err == nil {
					t.Fatal("altered master wrapping accepted")
				}
			}
		}
	}
}
