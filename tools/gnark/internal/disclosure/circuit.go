package disclosure

import (
	"fmt"
	"github.com/consensys/gnark/frontend"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

const Capacity = 32
const CircuitID = "shieldd.disclosure.bls12-377.groth16.v1.32"

type Predicate struct{ Op, Lower, Upper, Result frontend.Variable }
type PublicSlot struct {
	Active, Commitment                         frontend.Variable
	RevealAmount, RevealAsset, RevealRecipient frontend.Variable
	Amount, Asset, Generator, Transmission     frontend.Variable
	Predicate                                  Predicate
}
type Opening struct{ Blinding, Amount, Asset, Generator, Transmission, Recovery frontend.Variable }
type Circuit struct {
	Context                                            [2]frontend.Variable `gnark:",public"`
	ContextHash                                        frontend.Variable    `gnark:",public"`
	Slots                                              [Capacity]PublicSlot `gnark:",public"`
	TotalEnabled, TotalReveal, TotalAmount, TotalAsset frontend.Variable    `gnark:",public"`
	TotalPredicate                                     Predicate            `gnark:",public"`
	Notes                                              [Capacity]Opening
}

func predicate(api frontend.API, value frontend.Variable, p Predicate) {
	api.ToBinary(value, 128)
	api.ToBinary(p.Lower, 128)
	api.ToBinary(p.Upper, 128)
	api.AssertIsBoolean(p.Result)
	selectors := make([]frontend.Variable, 6)
	for i := range selectors {
		selectors[i] = api.IsZero(api.Sub(p.Op, i))
	}
	api.AssertIsEqual(api.Add(selectors[0], selectors[1], selectors[2], selectors[3], selectors[4], selectors[5]), 1)
	lo := api.Cmp(value, p.Lower)
	hi := api.Cmp(value, p.Upper)
	gt := api.IsZero(api.Sub(lo, 1))
	lt := api.IsZero(api.Add(lo, 1))
	ge := api.Sub(1, lt)
	le := api.Sub(1, gt)
	rangeOK := api.Mul(ge, api.Sub(1, api.IsZero(api.Sub(hi, 1))))
	expected := api.Add(api.Mul(selectors[1], gt), api.Mul(selectors[2], lt), api.Mul(selectors[3], ge), api.Mul(selectors[4], le), api.Mul(selectors[5], rangeOK))
	api.AssertIsEqual(api.Mul(selectors[0], p.Lower), 0)
	api.AssertIsEqual(api.Mul(api.Sub(1, selectors[5]), p.Upper), 0)
	api.AssertIsEqual(p.Result, expected)
	api.AssertIsEqual(api.Mul(selectors[5], api.IsZero(api.Sub(api.Cmp(p.Lower, p.Upper), 1))), 0)
}
func (c *Circuit) Define(api frontend.API) error {
	for _, x := range c.Context {
		api.ToBinary(x, 128)
	}
	h, err := primitives.Poseidon377Hash2(api, 332, c.Context)
	if err != nil {
		return err
	}
	api.AssertIsEqual(h, c.ContextHash)
	api.AssertIsBoolean(c.TotalEnabled)
	api.AssertIsBoolean(c.TotalReveal)
	api.AssertIsEqual(api.Mul(c.TotalReveal, api.Sub(1, c.TotalEnabled)), 0)
	sum := frontend.Variable(0)
	for i, s := range c.Slots {
		n := c.Notes[i]
		api.AssertIsBoolean(s.Active)
		if i == 0 {
			api.AssertIsEqual(s.Active, 1)
		} else {
			api.AssertIsEqual(api.Mul(s.Active, api.Sub(1, c.Slots[i-1].Active)), 0)
		}
		for _, v := range []frontend.Variable{s.RevealAmount, s.RevealAsset, s.RevealRecipient} {
			api.AssertIsBoolean(v)
		}
		inactive := api.Sub(1, s.Active)
		for _, v := range []frontend.Variable{s.Commitment, s.RevealAmount, s.RevealAsset, s.RevealRecipient, s.Amount, s.Asset, s.Generator, s.Transmission, s.Predicate.Op, s.Predicate.Lower, s.Predicate.Upper, s.Predicate.Result, n.Blinding, n.Amount, n.Asset, n.Generator, n.Transmission, n.Recovery} {
			api.AssertIsEqual(api.Mul(inactive, v), 0)
		}
		api.ToBinary(n.Amount, 128)
		api.AssertIsEqual(api.Mul(s.Active, api.IsZero(n.Amount)), 0)
		commitment, err := circuits.NoteCommitmentWithCompressedDivGen(api, n.Blinding, n.Amount, n.Asset, n.Generator, n.Transmission, n.Recovery)
		if err != nil {
			return fmt.Errorf("note commitment: %w", err)
		}
		api.AssertIsEqual(api.Mul(s.Active, api.Sub(commitment, s.Commitment)), 0)
		api.AssertIsEqual(s.Amount, api.Mul(s.Active, s.RevealAmount, n.Amount))
		api.AssertIsEqual(s.Asset, api.Mul(s.Active, s.RevealAsset, n.Asset))
		api.AssertIsEqual(s.Generator, api.Mul(s.Active, s.RevealRecipient, n.Generator))
		api.AssertIsEqual(s.Transmission, api.Mul(s.Active, s.RevealRecipient, n.Transmission))
		predicate(api, n.Amount, s.Predicate)
		api.AssertIsEqual(api.Mul(c.TotalEnabled, s.Active, api.Sub(n.Asset, c.TotalAsset)), 0)
		sum = api.Add(sum, api.Mul(s.Active, n.Amount))
	}
	// Bound only a requested sum: otherwise independent large notes remain valid.
	sum = api.Mul(c.TotalEnabled, sum)
	api.ToBinary(sum, 128)
	api.AssertIsEqual(c.TotalAmount, api.Mul(c.TotalReveal, sum))
	predicate(api, sum, c.TotalPredicate)
	return nil
}
