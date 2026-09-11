// Command audit-registry-experiment measures hidden certified-key membership.
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"time"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	p "github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

const depth = 32
const leafDomain = 832001
const nodeDomain = 832002

type circuit struct {
	Root     frontend.Variable `gnark:",public"`
	Leaves   [2][6]frontend.Variable
	Siblings [2][depth]frontend.Variable
	Right    [2][depth]frontend.Variable
}

func (c *circuit) Define(api frontend.API) error {
	for i := range c.Leaves {
		h, err := p.Poseidon377Hash6(api, leafDomain, c.Leaves[i])
		if err != nil {
			return err
		}
		for j := 0; j < depth; j++ {
			api.AssertIsBoolean(c.Right[i][j])
			left := api.Select(c.Right[i][j], c.Siblings[i][j], h)
			right := api.Select(c.Right[i][j], h, c.Siblings[i][j])
			h, err = p.Poseidon377Hash2(api, nodeDomain+j, [2]frontend.Variable{left, right})
			if err != nil {
				return err
			}
		}
		api.AssertIsEqual(h, c.Root)
	}
	return nil
}

func run() error {
	var a circuit
	var leaves [2]*big.Int
	for i := 0; i < 2; i++ {
		var fields [6]*big.Int
		for j := range fields {
			fields[j] = big.NewInt(int64(1 + i*6 + j))
			a.Leaves[i][j] = fields[j]
		}
		var err error
		leaves[i], err = p.Poseidon377Hash6Native(big.NewInt(leafDomain), fields)
		if err != nil {
			return err
		}
	}
	for i := 0; i < 2; i++ {
		h := leaves[i]
		for j := 0; j < depth; j++ {
			sibling := big.NewInt(int64(100 + j))
			bit := 0
			if j == 0 {
				sibling = leaves[1-i]
				bit = i
			}
			a.Siblings[i][j] = sibling
			a.Right[i][j] = bit
			pair := [2]*big.Int{h, sibling}
			if bit == 1 {
				pair = [2]*big.Int{sibling, h}
			}
			var err error
			h, err = p.Poseidon377Hash2Native(big.NewInt(nodeDomain+int64(j)), pair)
			if err != nil {
				return err
			}
		}
		a.Root = h
	}
	start := time.Now()
	ccs, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &circuit{})
	if err != nil {
		return err
	}
	m := map[string]any{"case": "two hidden 32-level Poseidon377 key memberships", "constraints": ccs.GetNbConstraints(), "compile_seconds": time.Since(start).Seconds(), "scope": "membership only; excludes existing encryption, key provisioning, and registry acceptance"}
	w, err := frontend.NewWitness(&a, ecc.BLS12_377.ScalarField())
	if err != nil {
		return err
	}
	start = time.Now()
	pk, vk, err := groth16.Setup(ccs)
	if err != nil {
		return err
	}
	m["setup_seconds"] = time.Since(start).Seconds()
	start = time.Now()
	proof, err := groth16.Prove(ccs, pk, w)
	if err != nil {
		return err
	}
	m["prove_seconds"] = time.Since(start).Seconds()
	pub, _ := w.Public()
	if err = groth16.Verify(proof, vk, pub); err != nil {
		return err
	}
	var b bytes.Buffer
	proof.WriteTo(&b)
	m["proof_bytes"] = b.Len()
	bad := a
	bad.Root = 1
	bw, _ := frontend.NewWitness(&bad, ecc.BLS12_377.ScalarField())
	bp, _ := bw.Public()
	if groth16.Verify(proof, vk, bp) == nil {
		return fmt.Errorf("changed root accepted")
	}
	for _, which := range []string{"leaf", "direction", "sibling"} {
		bad = a
		switch which {
		case "leaf":
			bad.Leaves[0][0] = 999
		case "direction":
			bad.Right[0][0] = 2
		case "sibling":
			bad.Siblings[0][4] = 999
		}
		bw, err = frontend.NewWitness(&bad, ecc.BLS12_377.ScalarField())
		if err != nil {
			return err
		}
		if _, err = ccs.Solve(bw); err == nil {
			return fmt.Errorf("changed %s accepted", which)
		}
	}
	m["negative_cases"] = "changed root, leaf, nonboolean direction, sibling rejected"
	return json.NewEncoder(os.Stdout).Encode(m)
}
func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
