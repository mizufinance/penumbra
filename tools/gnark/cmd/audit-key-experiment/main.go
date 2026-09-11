// Command audit-key-experiment measures a pairing relation, not a full IBE proof.
package main

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"math/big"
	"os"
	"time"

	"github.com/consensys/gnark-crypto/ecc"
	bls "github.com/consensys/gnark-crypto/ecc/bls12-381"
	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	sw "github.com/consensys/gnark/std/algebra/emulated/sw_bls12381"
)

type circuit struct {
	P      sw.G1Affine
	Result sw.GTEl `gnark:",public"`
}

func root() bls.G2Affine {
	_, _, _, g := bls.Generators()
	var q bls.G2Affine
	q.ScalarMultiplication(&g, big.NewInt(7))
	return q
}

func (c *circuit) Define(api frontend.API) error {
	p, err := sw.NewPairing(api)
	if err != nil {
		return err
	}
	q := sw.NewG2AffineFixed(root())
	p.AssertIsOnG1(&c.P)
	result, err := p.Pair([]*sw.G1Affine{&c.P}, []*sw.G2Affine{&q})
	if err != nil {
		return err
	}
	p.AssertIsEqual(result, &c.Result)
	return nil
}

func main() {
	prove := flag.Bool("prove", false, "run real development Groth16 setup/proof")
	flag.Parse()
	if err := run(*prove); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func run(prove bool) error {
	started := time.Now()
	ccs, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &circuit{})
	if err != nil {
		return err
	}
	metrics := map[string]any{"case": "BLS12-381 fixed-root pairing on Shieldd field",
		"constraints": ccs.GetNbConstraints(), "compile_seconds": time.Since(started).Seconds(),
		"scope": "one pairing and G1 subgroup check; excludes identity hash and IBE masks"}
	p, err := bls.HashToG1([]byte("synthetic Alice output"), []byte("BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_AUG_"))
	if err != nil {
		return err
	}
	r, err := bls.Pair([]bls.G1Affine{p}, []bls.G2Affine{root()})
	if err != nil {
		return err
	}
	assignment := circuit{P: sw.NewG1Affine(p), Result: sw.NewGTEl(r)}
	w, err := frontend.NewWitness(&assignment, ecc.BLS12_377.ScalarField())
	if err != nil {
		return err
	}
	// The Groth16 prover installs the commitment hints used by emulated arithmetic.
	// Raw ccs.Solve does not supply those hints; compile-only mode counts gates.
	if prove {
		started = time.Now()
		pk, vk, err := groth16.Setup(ccs)
		if err != nil {
			return err
		}
		metrics["setup_seconds"] = time.Since(started).Seconds()
		started = time.Now()
		proof, err := groth16.Prove(ccs, pk, w)
		if err != nil {
			return err
		}
		metrics["prove_seconds"] = time.Since(started).Seconds()
		public, err := w.Public()
		if err != nil {
			return err
		}
		started = time.Now()
		if err = groth16.Verify(proof, vk, public); err != nil {
			return err
		}
		metrics["verify_seconds"] = time.Since(started).Seconds()
		var buf bytes.Buffer
		if _, err = proof.WriteTo(&buf); err != nil {
			return err
		}
		metrics["proof_bytes"] = buf.Len()
		wrong := r
		wrong.Square(&wrong)
		bad := circuit{P: sw.NewG1Affine(p), Result: sw.NewGTEl(wrong)}
		badw, err := frontend.NewWitness(&bad, ecc.BLS12_377.ScalarField())
		if err != nil {
			return err
		}
		badpub, _ := badw.Public()
		if err = groth16.Verify(proof, vk, badpub); err == nil {
			return fmt.Errorf("altered result accepted")
		}
		metrics["altered_public_result"] = "rejected"
	}
	return json.NewEncoder(os.Stdout).Encode(metrics)
}
