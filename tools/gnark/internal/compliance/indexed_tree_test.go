package compliance

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

type indexedLeafCommitmentCircuit struct {
	Value          frontend.Variable
	NextIndex      frontend.Variable
	NextValue      frontend.Variable
	DKPubX         frontend.Variable
	DKPubY         frontend.Variable
	Threshold      frontend.Variable
	SlotCount      frontend.Variable
	ChannelsHash   frontend.Variable
	RingPKX        frontend.Variable
	RingPKY        frontend.Variable
	RingIDHash     frontend.Variable
	PolicyIDHash   frontend.Variable
	PermissionHash frontend.Variable
	ResourceHash   frontend.Variable

	Expected frontend.Variable `gnark:",public"`
}

func (c *indexedLeafCommitmentCircuit) Define(api frontend.API) error {
	commitment, err := IndexedLeafCommitment(api, IndexedLeafInputs{
		Value:          c.Value,
		NextIndex:      c.NextIndex,
		NextValue:      c.NextValue,
		DKPub:          gnarkte.Point{X: c.DKPubX, Y: c.DKPubY},
		Threshold:      c.Threshold,
		SlotCount:      c.SlotCount,
		ChannelsHash:   c.ChannelsHash,
		RingPK:         gnarkte.Point{X: c.RingPKX, Y: c.RingPKY},
		RingIDHash:     c.RingIDHash,
		PolicyIDHash:   c.PolicyIDHash,
		PermissionHash: c.PermissionHash,
		ResourceHash:   c.ResourceHash,
	})
	if err != nil {
		return err
	}
	api.AssertIsEqual(commitment, c.Expected)
	return nil
}

type quadPathCircuit struct {
	LeafHash frontend.Variable
	Position frontend.Variable
	Path     [ComplianceQuadTreeDepth][3]frontend.Variable

	ExpectedRoot frontend.Variable `gnark:",public"`
}

func (c *quadPathCircuit) Define(api frontend.API) error {
	root, err := VerifyQuadPath(api, c.LeafHash, c.Path, c.Position)
	if err != nil {
		return err
	}
	api.AssertIsEqual(root, c.ExpectedRoot)
	return nil
}

func syntheticIndexedLeafInputs(t *testing.T) IndexedLeafInputs {
	t.Helper()

	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}

	return IndexedLeafInputs{
		Value:     big.NewInt(11),
		NextIndex: uint64(7),
		NextValue: big.NewInt(22),
		DKPub: gnarkte.Point{
			X: primitives.MustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
			Y: primitives.MustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
		},
		Threshold:    "5",
		SlotCount:    "10",
		ChannelsHash: big.NewInt(33),
		RingPK: gnarkte.Point{
			X: primitives.MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorX),
			Y: primitives.MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorY),
		},
		RingIDHash:     big.NewInt(44),
		PolicyIDHash:   big.NewInt(55),
		PermissionHash: big.NewInt(66),
		ResourceHash:   big.NewInt(77),
	}
}

func syntheticQuadPath() ([ComplianceQuadTreeDepth][3]*big.Int, uint64) {
	var path [ComplianceQuadTreeDepth][3]*big.Int
	for i := 0; i < ComplianceQuadTreeDepth; i++ {
		for j := 0; j < 3; j++ {
			path[i][j] = big.NewInt(int64(100 + i*10 + j))
		}
	}
	return path, 0x9a35
}

func quadPathAssignment(path [ComplianceQuadTreeDepth][3]*big.Int) [ComplianceQuadTreeDepth][3]frontend.Variable {
	var out [ComplianceQuadTreeDepth][3]frontend.Variable
	for i := range path {
		for j := range path[i] {
			out[i][j] = path[i][j].String()
		}
	}
	return out
}

func TestIndexedLeafCommitmentNativeMatchesQuadPath(t *testing.T) {
	inputs := syntheticIndexedLeafInputs(t)
	commitment, err := IndexedLeafCommitmentNative(inputs)
	if err != nil {
		t.Fatalf("compute indexed leaf commitment: %v", err)
	}

	path, position := syntheticQuadPath()
	root, err := VerifyQuadPathNative(commitment, path, position)
	if err != nil {
		t.Fatalf("verify quad path natively: %v", err)
	}
	if root.Sign() == 0 {
		t.Fatal("expected non-zero quad path root")
	}
}

func TestIndexedLeafCommitmentCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &indexedLeafCommitmentCircuit{})
	if err != nil {
		t.Fatalf("compile indexed leaf commitment circuit: %v", err)
	}
}

func TestQuadPathCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &quadPathCircuit{})
	if err != nil {
		t.Fatalf("compile quad path circuit: %v", err)
	}
}

func TestIndexedLeafCircuitMatchesNativeCommitment(t *testing.T) {
	inputs := syntheticIndexedLeafInputs(t)
	commitment, err := IndexedLeafCommitmentNative(inputs)
	if err != nil {
		t.Fatalf("compute indexed leaf commitment: %v", err)
	}

	assignment := &indexedLeafCommitmentCircuit{
		Value:          inputs.Value,
		NextIndex:      inputs.NextIndex,
		NextValue:      inputs.NextValue,
		DKPubX:         inputs.DKPub.X,
		DKPubY:         inputs.DKPub.Y,
		Threshold:      inputs.Threshold,
		SlotCount:      inputs.SlotCount,
		ChannelsHash:   inputs.ChannelsHash,
		RingPKX:        inputs.RingPK.X,
		RingPKY:        inputs.RingPK.Y,
		RingIDHash:     inputs.RingIDHash,
		PolicyIDHash:   inputs.PolicyIDHash,
		PermissionHash: inputs.PermissionHash,
		ResourceHash:   inputs.ResourceHash,
		Expected:       commitment.String(),
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&indexedLeafCommitmentCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestQuadPathCircuitMatchesNativeRoot(t *testing.T) {
	leafHash := big.NewInt(123456789)
	path, position := syntheticQuadPath()
	root, err := VerifyQuadPathNative(leafHash, path, position)
	if err != nil {
		t.Fatalf("compute quad path root: %v", err)
	}

	assignment := &quadPathCircuit{
		LeafHash:     leafHash.String(),
		Position:     position,
		Path:         quadPathAssignment(path),
		ExpectedRoot: root.String(),
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&quadPathCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}
