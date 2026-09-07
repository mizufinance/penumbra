package primitives

import (
	"fmt"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
)

type poseidon377Hash7Circuit struct {
	Domain frontend.Variable
	In0    frontend.Variable
	In1    frontend.Variable
	In2    frontend.Variable
	In3    frontend.Variable
	In4    frontend.Variable
	In5    frontend.Variable
	In6    frontend.Variable

	Expected frontend.Variable `gnark:",public"`
}

func (c *poseidon377Hash7Circuit) Define(api frontend.API) error {
	result, err := Poseidon377Hash7(api, c.Domain, [7]frontend.Variable{
		c.In0, c.In1, c.In2, c.In3, c.In4, c.In5, c.In6,
	})
	if err != nil {
		return err
	}
	api.AssertIsEqual(result, c.Expected)
	return nil
}

func TestPoseidon377Hash7MatchesShielddVectors(t *testing.T) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}

	if got, want := len(vectors.Poseidon377.Hash7Inputs), 7; got != want {
		t.Fatalf("hash7 input length mismatch: got %d want %d", got, want)
	}

	assignment := &poseidon377Hash7Circuit{
		Domain:   vectors.Poseidon377.Hash7Domain,
		In0:      vectors.Poseidon377.Hash7Inputs[0],
		In1:      vectors.Poseidon377.Hash7Inputs[1],
		In2:      vectors.Poseidon377.Hash7Inputs[2],
		In3:      vectors.Poseidon377.Hash7Inputs[3],
		In4:      vectors.Poseidon377.Hash7Inputs[4],
		In5:      vectors.Poseidon377.Hash7Inputs[5],
		In6:      vectors.Poseidon377.Hash7Inputs[6],
		Expected: vectors.Poseidon377.Hash7Output,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&poseidon377Hash7Circuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

type decaf377CompressToFieldCircuit struct {
	X frontend.Variable
	Y frontend.Variable

	Expected frontend.Variable `gnark:",public"`
}

func (c *decaf377CompressToFieldCircuit) Define(api frontend.API) error {
	point := gnarkte.Point{
		X: c.X,
		Y: c.Y,
	}
	result, err := decafgnark.CompressToField(api, point)
	if err != nil {
		return err
	}
	api.AssertIsEqual(result, c.Expected)
	return nil
}

type decaf377EncodeToCurveCircuit struct {
	Input frontend.Variable

	ExpectedX        frontend.Variable `gnark:",public"`
	ExpectedY        frontend.Variable `gnark:",public"`
	ExpectedCompress frontend.Variable `gnark:",public"`
}

func (c *decaf377EncodeToCurveCircuit) Define(api frontend.API) error {
	point, err := decafgnark.EncodeToCurve(api, c.Input)
	if err != nil {
		return err
	}
	api.AssertIsEqual(point.X, c.ExpectedX)
	api.AssertIsEqual(point.Y, c.ExpectedY)

	compressed, err := decafgnark.CompressToField(api, point)
	if err != nil {
		return err
	}
	api.AssertIsEqual(compressed, c.ExpectedCompress)
	return nil
}

func TestDecaf377CompressToFieldMatchesShielddVectors(t *testing.T) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	if len(vectors.Decaf377Compress) == 0 {
		t.Fatalf("missing decaf377 compress vectors")
	}

	assert := test.NewAssert(t)
	for _, vector := range vectors.Decaf377Compress {
		vector := vector
		assert.Run(func(assert *test.Assert) {
			assignment := &decaf377CompressToFieldCircuit{
				X:        vector.X,
				Y:        vector.Y,
				Expected: vector.CompressToField,
			}

			assert.CheckCircuit(
				&decaf377CompressToFieldCircuit{},
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithValidAssignment(assignment),
			)
		}, "scalar="+vector.Scalar)
	}
}

func TestDecaf377EncodeToCurveMatchesShielddVectors(t *testing.T) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	if len(vectors.Decaf377Encode) == 0 {
		t.Fatalf("missing decaf377 encode vectors")
	}

	assert := test.NewAssert(t)
	for _, vector := range vectors.Decaf377Encode {
		vector := vector
		assert.Run(func(assert *test.Assert) {
			assignment := &decaf377EncodeToCurveCircuit{
				Input:            vector.Input,
				ExpectedX:        vector.X,
				ExpectedY:        vector.Y,
				ExpectedCompress: vector.CompressToField,
			}

			assert.CheckCircuit(
				&decaf377EncodeToCurveCircuit{},
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithValidAssignment(assignment),
			)
		}, "input="+vector.Input)
	}
}

func TestDecaf377EncodeToCurveNativeMatchesShielddVectors(t *testing.T) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}

	for _, vector := range vectors.Decaf377Encode {
		point, err := decafgnark.EncodeToCurveNative(MustBigInt(vector.Input))
		if err != nil {
			t.Fatalf("encode_to_curve(%s): %v", vector.Input, err)
		}
		if got, want := point.X.(*big.Int).String(), vector.X; got != want {
			t.Fatalf("encode_to_curve(%s) x mismatch: got %s want %s", vector.Input, got, want)
		}
		if got, want := point.Y.(*big.Int).String(), vector.Y; got != want {
			t.Fatalf("encode_to_curve(%s) y mismatch: got %s want %s", vector.Input, got, want)
		}
	}
}

func TestPoseidon377Hash7Compiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &poseidon377Hash7Circuit{})
	if err != nil {
		t.Fatalf("compile poseidon377 hash7 circuit: %v", err)
	}
}

func TestDecaf377EncodeToCurveCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &decaf377EncodeToCurveCircuit{})
	if err != nil {
		t.Fatalf("compile decaf377 encode_to_curve circuit: %v", err)
	}
}

// Composite-derivation differential tests: the nullifier and the note
// commitment are re-implemented in-circuit as Poseidon hashes over their
// respective domains. Rust golden outputs (poseidon377::hash_3 / hash_6, real
// crate) are checked in `phase05_vectors.json`; these assert the gnark gadgets
// reproduce them exactly, pinning RATE_3 / RATE_6 output correctness.

type nullifierDerivationCircuit struct {
	Domain frontend.Variable
	In0    frontend.Variable
	In1    frontend.Variable
	In2    frontend.Variable

	Expected frontend.Variable `gnark:",public"`
}

func (c *nullifierDerivationCircuit) Define(api frontend.API) error {
	result, err := Poseidon377Hash3(api, c.Domain, [3]frontend.Variable{c.In0, c.In1, c.In2})
	if err != nil {
		return err
	}
	api.AssertIsEqual(result, c.Expected)
	return nil
}

func TestNullifierDerivationMatchesShielddVectors(t *testing.T) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	if got, want := len(vectors.Poseidon377.NullifierInputs), 3; got != want {
		t.Fatalf("nullifier input length mismatch: got %d want %d", got, want)
	}

	assignment := &nullifierDerivationCircuit{
		Domain:   vectors.Poseidon377.NullifierDomain,
		In0:      vectors.Poseidon377.NullifierInputs[0],
		In1:      vectors.Poseidon377.NullifierInputs[1],
		In2:      vectors.Poseidon377.NullifierInputs[2],
		Expected: vectors.Poseidon377.NullifierOutput,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&nullifierDerivationCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

type noteCommitmentDerivationCircuit struct {
	Domain frontend.Variable
	In0    frontend.Variable
	In1    frontend.Variable
	In2    frontend.Variable
	In3    frontend.Variable
	In4    frontend.Variable
	In5    frontend.Variable

	Expected frontend.Variable `gnark:",public"`
}

func (c *noteCommitmentDerivationCircuit) Define(api frontend.API) error {
	result, err := Poseidon377Hash6(api, c.Domain, [6]frontend.Variable{
		c.In0, c.In1, c.In2, c.In3, c.In4, c.In5,
	})
	if err != nil {
		return err
	}
	api.AssertIsEqual(result, c.Expected)
	return nil
}

func TestNoteCommitmentDerivationMatchesShielddVectors(t *testing.T) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	if got, want := len(vectors.Poseidon377.NoteCommitInputs), 6; got != want {
		t.Fatalf("note commit input length mismatch: got %d want %d", got, want)
	}

	assignment := &noteCommitmentDerivationCircuit{
		Domain:   vectors.Poseidon377.NoteCommitDomain,
		In0:      vectors.Poseidon377.NoteCommitInputs[0],
		In1:      vectors.Poseidon377.NoteCommitInputs[1],
		In2:      vectors.Poseidon377.NoteCommitInputs[2],
		In3:      vectors.Poseidon377.NoteCommitInputs[3],
		In4:      vectors.Poseidon377.NoteCommitInputs[4],
		In5:      vectors.Poseidon377.NoteCommitInputs[5],
		Expected: vectors.Poseidon377.NoteCommitOutput,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&noteCommitmentDerivationCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

type noteReshapeStatementSeamCircuit struct {
	Fields   []frontend.Variable
	Expected frontend.Variable `gnark:",public"`
}

func statementVariables(values []string) []frontend.Variable {
	fields := make([]frontend.Variable, len(values))
	for index, value := range values {
		fields[index] = value
	}
	return fields
}

func (c *noteReshapeStatementSeamCircuit) Define(api frontend.API) error {
	var label string
	var nIn, nOut int
	switch len(c.Fields) {
	case 33:
		label, nIn, nOut = "note_reshape8x1", 8, 1
	case 26:
		label, nIn, nOut = "note_reshape1x8", 1, 8
	default:
		return fmt.Errorf("unsupported note reshape statement field count %d", len(c.Fields))
	}
	h, err := NoteReshapeStatementHashForShape(api, label, nIn, nOut, c.Fields)
	if err != nil {
		return err
	}
	api.AssertIsEqual(h, c.Expected)
	return nil
}

func TestNoteReshapeStatementSeamsMatchShielddVectors(t *testing.T) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	if got, want := len(vectors.NoteReshapeStatements), 2; got != want {
		t.Fatalf("statement vector count mismatch: got %d want %d", got, want)
	}
	for _, fx := range vectors.NoteReshapeStatements {
		if got, want := len(fx.Fields), len(fx.FieldRoles); got != want {
			t.Fatalf("%s role count mismatch: got %d want %d", fx.Label, got, want)
		}
		validFields := append([]string(nil), fx.Fields...)
		swappedFields := append([]string(nil), fx.Fields...)
		swappedFields[0], swappedFields[1] = swappedFields[1], swappedFields[0]
		assert := test.NewAssert(t)
		assert.CheckCircuit(
			&noteReshapeStatementSeamCircuit{Fields: make([]frontend.Variable, len(fx.Fields))},
			test.WithCurves(ecc.BLS12_377),
			test.WithBackends(backend.GROTH16),
			test.WithValidAssignment(&noteReshapeStatementSeamCircuit{Fields: statementVariables(validFields), Expected: fx.StatementHash}),
			test.WithInvalidAssignment(&noteReshapeStatementSeamCircuit{Fields: statementVariables(swappedFields), Expected: fx.StatementHash}),
		)
	}
}
