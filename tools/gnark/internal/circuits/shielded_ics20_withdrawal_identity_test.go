package circuits

import (
	"math/big"
	"testing"

	decaf377 "github.com/mizufinance/decaf377-go"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

type decafNonIdentityCircuit struct {
	Point Point2D
}

func (c *decafNonIdentityCircuit) Define(api frontend.API) error {
	AssertDecafNonIdentity(api, gnarkte.Point{X: c.Point.X, Y: c.Point.Y})
	return nil
}

func TestAssertDecafNonIdentityRejectsBothIdentityRepresentatives(t *testing.T) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}
	valid := &decafNonIdentityCircuit{
		Point: Point2D{
			X: primitives.MustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
			Y: primitives.MustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
		},
	}
	if err := test.IsSolved(
		&decafNonIdentityCircuit{},
		valid,
		ecc.BLS12_377.ScalarField(),
	); err != nil {
		t.Fatalf("non-identity generator rejected: %v", err)
	}

	minusOne := new(big.Int).Sub(ecc.BLS12_377.ScalarField(), big.NewInt(1))
	for name, identity := range map[string]*decafNonIdentityCircuit{
		"positive representative": {Point: Point2D{X: 0, Y: 1}},
		"negative representative": {Point: Point2D{X: 0, Y: minusOne}},
	} {
		t.Run(name, func(t *testing.T) {
			if err := test.IsSolved(
				&decafNonIdentityCircuit{},
				identity,
				ecc.BLS12_377.ScalarField(),
			); err == nil {
				t.Fatal("Decaf identity representative satisfied X != 0")
			}
		})
	}
}

type incomingViewingKeyNonzeroCircuit struct {
	IVKReduced frontend.Variable
}

func (c *incomingViewingKeyNonzeroCircuit) Define(api frontend.API) error {
	AssertIncomingViewingKeyNonzero(api, c.IVKReduced)
	return nil
}

func TestIncomingViewingKeyNonzeroRejectsZero(t *testing.T) {
	if err := test.IsSolved(
		&incomingViewingKeyNonzeroCircuit{},
		&incomingViewingKeyNonzeroCircuit{IVKReduced: 1},
		ecc.BLS12_377.ScalarField(),
	); err != nil {
		t.Fatalf("nonzero incoming viewing key rejected: %v", err)
	}
	if err := test.IsSolved(
		&incomingViewingKeyNonzeroCircuit{},
		&incomingViewingKeyNonzeroCircuit{IVKReduced: 0},
		ecc.BLS12_377.ScalarField(),
	); err == nil {
		t.Fatal("zero incoming viewing key satisfied the nonzero guard")
	}
}

// unguardedWithdrawalIdentitySenderCircuit reproduces the ownership alias.
type unguardedWithdrawalIdentitySenderCircuit struct {
	Anchor         frontend.Variable `gnark:",public"`
	NoteCommitment frontend.Variable `gnark:",public"`
	Nullifier      frontend.Variable `gnark:",public"`

	NK           frontend.Variable
	AK           Point2D
	DivGen       Point2D
	IVKReduced   frontend.Variable
	IVKQuotientA frontend.Variable
	NoteBlinding frontend.Variable
	NoteAmount   frontend.Variable
	AssetID      frontend.Variable
	Position     frontend.Variable
	Path         [StateCommitmentDepth][3]frontend.Variable
}

func (c *unguardedWithdrawalIdentitySenderCircuit) Define(api frontend.API) error {
	ak := gnarkte.Point{X: c.AK.X, Y: c.AK.Y}
	divGen := gnarkte.Point{X: c.DivGen.X, Y: c.DivGen.Y}

	divGenFq, err := decafgnark.CompressToField(api, divGen)
	if err != nil {
		return err
	}
	transmission, err := DiversifiedTransmissionKey(
		api,
		c.NK,
		ak,
		divGen,
		c.IVKReduced,
		c.IVKQuotientA,
	)
	if err != nil {
		return err
	}
	transmissionFq, err := decafgnark.CompressToField(api, transmission)
	if err != nil {
		return err
	}
	commitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		c.NoteBlinding,
		c.NoteAmount,
		c.AssetID,
		divGenFq,
		transmissionFq,
		0,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(commitment, c.NoteCommitment)

	path := make([][3]frontend.Variable, len(c.Path))
	copy(path, c.Path[:])
	root, err := primitives.VerifyStateCommitmentPath(
		api,
		c.NoteCommitment,
		c.Position,
		path,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(root, c.Anchor)

	nullifier, err := Nullifier(api, c.NK, c.NoteCommitment, c.Position)
	if err != nil {
		return err
	}
	api.AssertIsEqual(nullifier, c.Nullifier)
	return nil
}

type withdrawalIdentitySenderNativeValues struct {
	commitment *big.Int
	anchor     *big.Int
	position   uint64
	path       [StateCommitmentDepth][3]*big.Int
}

func withdrawalIdentitySenderValues(
	t *testing.T,
) withdrawalIdentitySenderNativeValues {
	t.Helper()
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}
	commitment, err := primitives.Poseidon377Hash6Native(
		primitives.MustBigInt(vectors.Poseidon377.NoteCommitDomain),
		[6]*big.Int{
			big.NewInt(1234),
			big.NewInt(55),
			big.NewInt(777),
			big.NewInt(0),
			big.NewInt(0),
			big.NewInt(0),
		},
	)
	if err != nil {
		t.Fatalf("compute identity-owned note commitment: %v", err)
	}

	var path [StateCommitmentDepth][3]*big.Int
	for i := range path {
		for j := range path[i] {
			path[i][j] = big.NewInt(int64(1000 + i*10 + j))
		}
	}
	const position = uint64(0x12345)
	return withdrawalIdentitySenderNativeValues{
		commitment: commitment,
		anchor:     withdrawalIdentitySenderRoot(t, commitment, position, path),
		position:   position,
		path:       path,
	}
}

func withdrawalIdentitySenderRoot(
	t *testing.T,
	commitment *big.Int,
	position uint64,
	path [StateCommitmentDepth][3]*big.Int,
) *big.Int {
	t.Helper()
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}
	domain := primitives.MustBigInt(vectors.Poseidon377.TCTDomain)
	current, err := primitives.Poseidon377Hash1Native(domain, commitment)
	if err != nil {
		t.Fatalf("hash state commitment leaf: %v", err)
	}
	for height := 1; height <= len(path); height++ {
		shift := 2 * (height - 1)
		index := int(
			(position>>shift)&1 +
				2*((position>>(shift+1))&1),
		)
		siblings := path[len(path)-height]
		var children [4]*big.Int
		siblingIndex := 0
		for childIndex := range children {
			if childIndex == index {
				children[childIndex] = current
			} else {
				children[childIndex] = siblings[siblingIndex]
				siblingIndex++
			}
		}
		heightDomain := new(big.Int).Add(domain, big.NewInt(int64(height)))
		heightDomain.Mod(heightDomain, primitives.ScalarField())
		current, err = primitives.Poseidon377Hash4Native(heightDomain, children)
		if err != nil {
			t.Fatalf("hash state commitment node %d: %v", height, err)
		}
	}
	return current
}

func unguardedWithdrawalIdentitySenderAssignment(
	t *testing.T,
	nk *big.Int,
	values withdrawalIdentitySenderNativeValues,
) (*unguardedWithdrawalIdentitySenderCircuit, *big.Int) {
	t.Helper()
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}
	ivkModQ, err := primitives.Poseidon377Hash2Native(
		primitives.MustBigInt(vectors.Poseidon377.IVKDomain),
		[2]*big.Int{nk, big.NewInt(0)},
	)
	if err != nil {
		t.Fatalf("compute incoming viewing key: %v", err)
	}
	ivkQuotient := new(big.Int)
	ivkReduced := new(big.Int)
	ivkQuotient.QuoRem(ivkModQ, decaf377.ScalarOrder(), ivkReduced)

	nullifier, err := primitives.Poseidon377Hash3Native(
		primitives.MustBigInt(vectors.Poseidon377.NullifierDomain),
		[3]*big.Int{nk, values.commitment, new(big.Int).SetUint64(values.position)},
	)
	if err != nil {
		t.Fatalf("compute nullifier: %v", err)
	}

	var path [StateCommitmentDepth][3]frontend.Variable
	for i := range path {
		for j := range path[i] {
			path[i][j] = values.path[i][j]
		}
	}
	return &unguardedWithdrawalIdentitySenderCircuit{
		Anchor:         values.anchor,
		NoteCommitment: values.commitment,
		Nullifier:      nullifier,
		NK:             nk,
		AK:             Point2D{X: 0, Y: 1},
		DivGen:         Point2D{X: 0, Y: 1},
		IVKReduced:     ivkReduced,
		IVKQuotientA:   ivkQuotient,
		NoteBlinding:   1234,
		NoteAmount:     55,
		AssetID:        777,
		Position:       values.position,
		Path:           path,
	}, nullifier
}

func TestWithdrawalUnguardedIdentitySenderAllowsOwnershipAlias(
	t *testing.T,
) {
	values := withdrawalIdentitySenderValues(t)
	first, firstNullifier := unguardedWithdrawalIdentitySenderAssignment(
		t,
		big.NewInt(11),
		values,
	)
	second, secondNullifier := unguardedWithdrawalIdentitySenderAssignment(
		t,
		big.NewInt(12),
		values,
	)
	if first.NoteCommitment != second.NoteCommitment ||
		first.Anchor != second.Anchor {
		t.Fatal("regression setup did not reuse the same anchored note commitment")
	}
	if firstNullifier.Cmp(secondNullifier) == 0 {
		t.Fatal("distinct nullifier keys unexpectedly produced one nullifier")
	}
	for name, assignment := range map[string]*unguardedWithdrawalIdentitySenderCircuit{
		"first nullifier key":  first,
		"second nullifier key": second,
	} {
		t.Run(name, func(t *testing.T) {
			if err := test.IsSolved(
				&unguardedWithdrawalIdentitySenderCircuit{},
				assignment,
				ecc.BLS12_377.ScalarField(),
			); err != nil {
				t.Fatalf("unguarded identity-sender relation should be satisfiable: %v", err)
			}
		})
	}
}
