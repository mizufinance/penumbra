package primitives

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/test"
)

type stateCommitmentPathCircuit struct {
	Commitment frontend.Variable
	Position   frontend.Variable
	Path       [24][3]frontend.Variable

	ExpectedRoot frontend.Variable `gnark:",public"`
}

func (c *stateCommitmentPathCircuit) Define(api frontend.API) error {
	path := make([][3]frontend.Variable, len(c.Path))
	copy(path, c.Path[:])
	root, err := VerifyStateCommitmentPath(api, c.Commitment, c.Position, path)
	if err != nil {
		return err
	}
	api.AssertIsEqual(root, c.ExpectedRoot)
	return nil
}

func syntheticStateCommitmentPath() ([24][3]*big.Int, uint64) {
	var path [24][3]*big.Int
	for i := range path {
		for j := range path[i] {
			path[i][j] = big.NewInt(int64(1000 + i*10 + j))
		}
	}
	return path, 0x12345
}

func computeStateCommitmentRootNative(
	t *testing.T,
	commitment *big.Int,
	position uint64,
	path [24][3]*big.Int,
) *big.Int {
	t.Helper()

	vectors, err := LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}

	domain := MustBigInt(vectors.Poseidon377.TCTDomain)
	current, err := Poseidon377Hash1Native(domain, commitment)
	if err != nil {
		t.Fatalf("hash leaf: %v", err)
	}

	for height := 1; height <= len(path); height++ {
		shift := 2 * (height - 1)
		bit0 := (position >> shift) & 1
		bit1 := (position >> (shift + 1)) & 1
		index := int(bit0 + 2*bit1)
		siblings := path[len(path)-height]

		var children [4]*big.Int
		siblingIdx := 0
		for i := 0; i < 4; i++ {
			if i == index {
				children[i] = current
				continue
			}
			children[i] = siblings[siblingIdx]
			siblingIdx++
		}

		heightDomain := new(big.Int).Add(domain, big.NewInt(int64(height)))
		heightDomain.Mod(heightDomain, ScalarField())
		current, err = Poseidon377Hash4Native(heightDomain, children)
		if err != nil {
			t.Fatalf("hash node at height %d: %v", height, err)
		}
	}

	return current
}

func statePathAssignment(path [24][3]*big.Int) [24][3]frontend.Variable {
	var out [24][3]frontend.Variable
	for i := range path {
		for j := range path[i] {
			out[i][j] = path[i][j].String()
		}
	}
	return out
}

func TestStateCommitmentPathCircuitMatchesSyntheticFixture(t *testing.T) {
	commitment := big.NewInt(987654321)
	path, position := syntheticStateCommitmentPath()
	root := computeStateCommitmentRootNative(t, commitment, position, path)

	assignment := &stateCommitmentPathCircuit{
		Commitment:   commitment.String(),
		Position:     position,
		Path:         statePathAssignment(path),
		ExpectedRoot: root.String(),
	}
	positionAlias := &stateCommitmentPathCircuit{
		Commitment:   commitment.String(),
		Position:     position | (uint64(1) << 48),
		Path:         statePathAssignment(path),
		ExpectedRoot: root.String(),
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&stateCommitmentPathCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
		test.WithInvalidAssignment(positionAlias),
	)
}
