package disclosure

import (
	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/test"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"math/big"
	"testing"
)

func fixture(t *testing.T) Circuit {
	t.Helper()
	c := Circuit{Context: [2]frontend.Variable{1, 2}, TotalEnabled: 0, TotalReveal: 0, TotalAmount: 0, TotalAsset: 0, TotalPredicate: Predicate{0, 0, 0, 0}}
	h, e := primitives.Poseidon377Hash2Native(big.NewInt(332), [2]*big.Int{big.NewInt(1), big.NewInt(2)})
	if e != nil {
		t.Fatal(e)
	}
	c.ContextHash = h
	for i := range c.Slots {
		c.Slots[i] = PublicSlot{0, 0, 0, 0, 0, 0, 0, 0, 0, Predicate{0, 0, 0, 0}}
		c.Notes[i] = Opening{0, 0, 0, 0, 0, 0}
	}
	v, e := primitives.LoadPrototypeVectors()
	if e != nil {
		t.Fatal(e)
	}
	commitment, e := primitives.Poseidon377Hash6Native(primitives.MustBigInt(v.Poseidon377.NoteCommitDomain), [6]*big.Int{big.NewInt(1), big.NewInt(42), big.NewInt(3), big.NewInt(4), big.NewInt(5), big.NewInt(6)})
	if e != nil {
		t.Fatal(e)
	}
	c.Slots[0] = PublicSlot{1, commitment, 0, 1, 0, 0, 3, 0, 0, Predicate{1, 42, 0, 0}}
	c.Notes[0] = Opening{1, 42, 3, 4, 5, 6}
	return c
}
func TestConstraints(t *testing.T) {
	cases := []struct {
		name   string
		change func(*Circuit)
		valid  bool
	}{
		{"false", func(c *Circuit) {}, true},
		{"true", func(c *Circuit) { c.Slots[0].Predicate = Predicate{3, 42, 0, 1} }, true},
		{"forged result", func(c *Circuit) { c.Slots[0].Predicate.Result = 1 }, false},
		{"commitment", func(c *Circuit) { c.Notes[0].Blinding = 2 }, false},
		{"selector", func(c *Circuit) { c.Slots[0].RevealAmount = 2 }, false},
		{"padding", func(c *Circuit) { c.Slots[2].Active = 1 }, false},
		{"context", func(c *Circuit) { c.Context[0] = 3 }, false},
		{"inverted range", func(c *Circuit) { c.Slots[0].Predicate = Predicate{5, 43, 42, 0} }, false},
		{"nonboolean", func(c *Circuit) { c.Slots[0].Predicate.Result = 2 }, false},
		{"total", func(c *Circuit) { c.TotalEnabled = 1; c.TotalAsset = 3; c.TotalReveal = 1; c.TotalAmount = 42 }, true},
		{"wrong total", func(c *Circuit) { c.TotalEnabled = 1; c.TotalAsset = 3; c.TotalReveal = 1; c.TotalAmount = 43 }, false},
		{"wrong asset", func(c *Circuit) { c.TotalEnabled = 1; c.TotalAsset = 4 }, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			c := fixture(t)
			tc.change(&c)
			e := test.IsSolved(&Circuit{}, &c, ecc.BLS12_377.ScalarField())
			if (e == nil) != tc.valid {
				t.Fatalf("valid=%v error=%v", tc.valid, e)
			}
		})
	}
}
