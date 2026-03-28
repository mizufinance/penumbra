package prototype

import (
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

func TestPoseidon377Hash7MatchesPenumbraVectors(t *testing.T) {
	vectors, err := loadPrototypeVectors()
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
	result, err := Decaf377CompressToField(api, point)
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
	point, err := Decaf377EncodeToCurve(api, c.Input)
	if err != nil {
		return err
	}
	api.AssertIsEqual(point.X, c.ExpectedX)
	api.AssertIsEqual(point.Y, c.ExpectedY)

	compressed, err := Decaf377CompressToField(api, point)
	if err != nil {
		return err
	}
	api.AssertIsEqual(compressed, c.ExpectedCompress)
	return nil
}

func TestDecaf377CompressToFieldMatchesPenumbraVectors(t *testing.T) {
	vectors, err := loadPrototypeVectors()
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

func TestDecaf377EncodeToCurveMatchesPenumbraVectors(t *testing.T) {
	vectors, err := loadPrototypeVectors()
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

func TestDecaf377EncodeToCurveNativeMatchesPenumbraVectors(t *testing.T) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}

	for _, vector := range vectors.Decaf377Encode {
		point, err := Decaf377EncodeToCurveNative(mustBigInt(vector.Input))
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
