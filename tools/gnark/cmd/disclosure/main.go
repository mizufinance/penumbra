// disclosure is a local, bounded proof transport. Private witnesses enter on stdin.
package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/logger"
	d "github.com/mizufinance/shieldd/tools/gnark/internal/disclosure"
	"io"
	"os"
	"path/filepath"
	"runtime"
)

type manifest struct {
	Circuit     string `json:"circuit"`
	Development bool   `json:"development"`
	VK          string `json:"vk_sha256"`
	PK          string `json:"pk_sha256"`
}
type request struct {
	Assignment d.Circuit `json:"assignment"`
	Proof      []byte    `json:"proof"`
}
type response struct {
	Proof    []byte `json:"proof,omitempty"`
	Verified bool   `json:"verified"`
}

func digest(b []byte) string { h := sha256.Sum256(b); return hex.EncodeToString(h[:]) }
func run() error {
	if runtime.GOMAXPROCS(0) > 2 {
		runtime.GOMAXPROCS(2)
	}
	logger.Disable()
	if len(os.Args) != 3 {
		return errors.New("usage: disclosure setup-development|prove|verify artifact-directory")
	}
	op, dir := os.Args[1], os.Args[2]
	if op == "setup-development" {
		if _, e := os.Stat(dir); !os.IsNotExist(e) {
			return errors.New("artifact directory must not exist")
		}
		cs, e := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &d.Circuit{})
		if e != nil {
			return e
		}
		pk, vk, e := groth16.Setup(cs)
		if e != nil {
			return e
		}
		var p, v bytes.Buffer
		if _, e = pk.WriteTo(&p); e != nil {
			return e
		}
		if _, e = vk.WriteTo(&v); e != nil {
			return e
		}
		if e = os.MkdirAll(dir, 0700); e != nil {
			return e
		}
		if e = os.WriteFile(filepath.Join(dir, "pk.bin"), p.Bytes(), 0600); e != nil {
			return e
		}
		if e = os.WriteFile(filepath.Join(dir, "vk.bin"), v.Bytes(), 0600); e != nil {
			return e
		}
		m, _ := json.Marshal(manifest{d.CircuitID, true, digest(v.Bytes()), digest(p.Bytes())})
		return os.WriteFile(filepath.Join(dir, "manifest.json"), m, 0600)
	}
	if op != "prove" && op != "verify" {
		return errors.New("unknown operation")
	}
	var m manifest
	b, e := os.ReadFile(filepath.Join(dir, "manifest.json"))
	if e != nil {
		return e
	}
	if e = json.Unmarshal(b, &m); e != nil {
		return e
	}
	if m.Circuit != d.CircuitID {
		return errors.New("wrong circuit")
	}
	// This transport is also usable directly; release applications must approve their local manifest.
	vkBytes, e := os.ReadFile(filepath.Join(dir, "vk.bin"))
	if e != nil {
		return e
	}
	if digest(vkBytes) != m.VK {
		return errors.New("verification key digest mismatch")
	}
	vk := groth16.NewVerifyingKey(ecc.BLS12_377)
	if _, e = vk.ReadFrom(bytes.NewReader(vkBytes)); e != nil {
		return e
	}
	input, e := io.ReadAll(io.LimitReader(os.Stdin, 16*1024*1024+1))
	if e != nil {
		return e
	}
	if len(input) > 16*1024*1024 {
		return errors.New("input too large")
	}
	var r request
	dec := json.NewDecoder(bytes.NewReader(input))
	dec.DisallowUnknownFields()
	if e = dec.Decode(&r); e != nil {
		return errors.New("invalid input")
	}
	w, e := frontend.NewWitness(&r.Assignment, ecc.BLS12_377.ScalarField())
	if e != nil {
		return errors.New("invalid witness")
	}
	pub, e := w.Public()
	if e != nil {
		return e
	}
	if op == "verify" {
		p := groth16.NewProof(ecc.BLS12_377)
		reader := bytes.NewReader(r.Proof)
		if _, e = p.ReadFrom(reader); e != nil {
			return errors.New("invalid proof")
		}
		if reader.Len() != 0 {
			return errors.New("trailing proof bytes")
		}
		if e = groth16.Verify(p, vk, pub); e != nil {
			return errors.New("proof rejected")
		}
		return json.NewEncoder(os.Stdout).Encode(response{Verified: true})
	}
	pb, e := os.ReadFile(filepath.Join(dir, "pk.bin"))
	if e != nil {
		return e
	}
	if digest(pb) != m.PK {
		return errors.New("proving key digest mismatch")
	}
	pk := groth16.NewProvingKey(ecc.BLS12_377)
	if _, e = pk.ReadFrom(bytes.NewReader(pb)); e != nil {
		return e
	}
	cs, e := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &d.Circuit{})
	if e != nil {
		return e
	}
	p, e := groth16.Prove(cs, pk, w)
	if e != nil {
		return errors.New("proving failed")
	}
	if e = groth16.Verify(p, vk, pub); e != nil {
		return errors.New("generated proof rejected")
	}
	var out bytes.Buffer
	if _, e = p.WriteTo(&out); e != nil {
		return e
	}
	return json.NewEncoder(os.Stdout).Encode(response{Proof: out.Bytes(), Verified: true})
}
func main() {
	if e := run(); e != nil {
		fmt.Fprintln(os.Stderr, e)
		os.Exit(1)
	}
}
