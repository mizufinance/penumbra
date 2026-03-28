package prototype

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
)

type indexedLeafCommitmentCircuit struct {
	Value          frontend.Variable
	NextIndex      frontend.Variable
	NextValue      frontend.Variable
	DKPubX         frontend.Variable
	DKPubY         frontend.Variable
	Threshold      frontend.Variable
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
	commitment, err := IndexedLeafCommitment(api, indexedLeafInputs{
		Value:          c.Value,
		NextIndex:      c.NextIndex,
		NextValue:      c.NextValue,
		DKPub:          gnarkte.Point{X: c.DKPubX, Y: c.DKPubY},
		Threshold:      c.Threshold,
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
	Path     [complianceQuadTreeDepth][3]frontend.Variable

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

func TestIndexedLeafCommitmentNativeMatchesAssetFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	inputs, err := indexedLeafInputsFromFixture(fixture)
	if err != nil {
		t.Fatalf("decode indexed leaf inputs: %v", err)
	}

	commitment, err := IndexedLeafCommitmentNative(inputs)
	if err != nil {
		t.Fatalf("compute indexed leaf commitment: %v", err)
	}

	path, err := quadPathFromFixture(fixture.Private.AssetPath)
	if err != nil {
		t.Fatalf("decode asset path: %v", err)
	}
	root, err := VerifyQuadPathNative(commitment, path, fixture.Private.AssetPosition)
	if err != nil {
		t.Fatalf("verify asset path natively: %v", err)
	}

	if got, want := root.String(), fixture.Public.AssetAnchor; got != want {
		t.Fatalf("asset root mismatch: got %s want %s", got, want)
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

func TestAssetIndexedLeafCircuitMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	inputs, err := indexedLeafInputsFromFixture(fixture)
	if err != nil {
		t.Fatalf("decode indexed leaf inputs: %v", err)
	}
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

func TestAssetQuadPathCircuitMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	inputs, err := indexedLeafInputsFromFixture(fixture)
	if err != nil {
		t.Fatalf("decode indexed leaf inputs: %v", err)
	}
	commitment, err := IndexedLeafCommitmentNative(inputs)
	if err != nil {
		t.Fatalf("compute indexed leaf commitment: %v", err)
	}
	path, err := quadPathFromFixture(fixture.Private.AssetPath)
	if err != nil {
		t.Fatalf("decode asset path: %v", err)
	}

	var assignmentPath [complianceQuadTreeDepth][3]frontend.Variable
	for i := 0; i < complianceQuadTreeDepth; i++ {
		for j := 0; j < 3; j++ {
			assignmentPath[i][j] = path[i][j].String()
		}
	}

	assignment := &quadPathCircuit{
		LeafHash:     commitment.String(),
		Position:     fixture.Private.AssetPosition,
		Path:         assignmentPath,
		ExpectedRoot: fixture.Public.AssetAnchor,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&quadPathCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestComplianceQuadPathCircuitMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	leafCommitment := fixture.Private.UserLeafCommitment
	path, err := quadPathFromFixture(fixture.Private.CompliancePath)
	if err != nil {
		t.Fatalf("decode compliance path: %v", err)
	}

	var assignmentPath [complianceQuadTreeDepth][3]frontend.Variable
	for i := 0; i < complianceQuadTreeDepth; i++ {
		for j := 0; j < 3; j++ {
			assignmentPath[i][j] = path[i][j].String()
		}
	}

	assignment := &quadPathCircuit{
		LeafHash:     leafCommitment,
		Position:     fixture.Private.CompliancePosition,
		Path:         assignmentPath,
		ExpectedRoot: fixture.Public.ComplianceAnchor,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&quadPathCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}
