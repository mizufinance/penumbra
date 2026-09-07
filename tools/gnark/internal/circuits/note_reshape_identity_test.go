package circuits_test

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	decaf377 "github.com/mizufinance/decaf377-go"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

type noteReshapeIdentityOwnershipAliasProbe struct {
	Anchor         frontend.Variable `gnark:",public"`
	NoteCommitment frontend.Variable `gnark:",public"`
	Nullifier0     frontend.Variable `gnark:",public"`
	Nullifier1     frontend.Variable `gnark:",public"`

	AK           circuits.Point2D
	DivGen       circuits.Point2D
	NK0          frontend.Variable
	NK1          frontend.Variable
	IVKReduced0  frontend.Variable
	IVKQuotient0 frontend.Variable
	IVKReduced1  frontend.Variable
	IVKQuotient1 frontend.Variable
	Blinding     frontend.Variable
	Amount       frontend.Variable
	AssetID      frontend.Variable
	Position     frontend.Variable
	Path         [circuits.StateCommitmentDepth][3]frontend.Variable
}

func defineNoteReshapeIdentityOwnershipAlias(
	api frontend.API,
	probe *noteReshapeIdentityOwnershipAliasProbe,
	enforceNonIdentity bool,
) error {
	ak := gnarkte.Point{X: probe.AK.X, Y: probe.AK.Y}
	divGen := gnarkte.Point{X: probe.DivGen.X, Y: probe.DivGen.Y}
	if enforceNonIdentity {
		circuits.AssertDecafNonIdentity(api, divGen)
	}

	divGenFq, err := decafgnark.CompressToField(api, divGen)
	if err != nil {
		return err
	}
	transmission0, err := circuits.DiversifiedTransmissionKey(
		api,
		probe.NK0,
		ak,
		divGen,
		probe.IVKReduced0,
		probe.IVKQuotient0,
	)
	if err != nil {
		return err
	}
	transmission1, err := circuits.DiversifiedTransmissionKey(
		api,
		probe.NK1,
		ak,
		divGen,
		probe.IVKReduced1,
		probe.IVKQuotient1,
	)
	if err != nil {
		return err
	}
	transmission0Fq, err := decafgnark.CompressToField(api, transmission0)
	if err != nil {
		return err
	}
	transmission1Fq, err := decafgnark.CompressToField(api, transmission1)
	if err != nil {
		return err
	}
	commitment0, err := circuits.NoteCommitmentWithCompressedDivGen(
		api,
		probe.Blinding,
		probe.Amount,
		probe.AssetID,
		divGenFq,
		transmission0Fq,
		0,
	)
	if err != nil {
		return err
	}
	commitment1, err := circuits.NoteCommitmentWithCompressedDivGen(
		api,
		probe.Blinding,
		probe.Amount,
		probe.AssetID,
		divGenFq,
		transmission1Fq,
		0,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(commitment0, probe.NoteCommitment)
	api.AssertIsEqual(commitment1, probe.NoteCommitment)

	path := make([][3]frontend.Variable, len(probe.Path))
	copy(path, probe.Path[:])
	root, err := primitives.VerifyStateCommitmentPath(
		api,
		probe.NoteCommitment,
		probe.Position,
		path,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(root, probe.Anchor)

	nullifier0, err := circuits.Nullifier(
		api,
		probe.NK0,
		probe.NoteCommitment,
		probe.Position,
	)
	if err != nil {
		return err
	}
	nullifier1, err := circuits.Nullifier(
		api,
		probe.NK1,
		probe.NoteCommitment,
		probe.Position,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(nullifier0, probe.Nullifier0)
	api.AssertIsEqual(nullifier1, probe.Nullifier1)
	api.AssertIsDifferent(probe.NK0, probe.NK1)
	api.AssertIsDifferent(probe.Nullifier0, probe.Nullifier1)
	return nil
}

func (p *noteReshapeIdentityOwnershipAliasProbe) Define(api frontend.API) error {
	return defineNoteReshapeIdentityOwnershipAlias(api, p, false)
}

type hardenedNoteReshapeIdentityOwnershipAliasProbe struct {
	noteReshapeIdentityOwnershipAliasProbe
}

func (p *hardenedNoteReshapeIdentityOwnershipAliasProbe) Define(
	api frontend.API,
) error {
	return defineNoteReshapeIdentityOwnershipAlias(
		api,
		&p.noteReshapeIdentityOwnershipAliasProbe,
		true,
	)
}

func noteReshapeIdentityOwnershipAliasAssignment(
	t *testing.T,
) noteReshapeIdentityOwnershipAliasProbe {
	t.Helper()
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}
	ak := gnarkte.Point{
		X: primitives.MustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
		Y: primitives.MustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
	}
	akFq, err := decafgnark.CompressToFieldNative(ak)
	if err != nil {
		t.Fatalf("compress authorization key: %v", err)
	}
	nk0 := big.NewInt(11)
	nk1 := big.NewInt(12)
	ivkReduced0, ivkQuotient0 := noteReshapeIdentityIVK(t, nk0, akFq)
	ivkReduced1, ivkQuotient1 := noteReshapeIdentityIVK(t, nk1, akFq)

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

	var nativePath [circuits.StateCommitmentDepth][3]*big.Int
	var path [circuits.StateCommitmentDepth][3]frontend.Variable
	for i := range nativePath {
		for j := range nativePath[i] {
			value := big.NewInt(int64(1000 + i*10 + j))
			nativePath[i][j] = value
			path[i][j] = value
		}
	}
	const position = uint64(0x12345)
	nullifier0 := noteReshapeIdentityNullifier(t, nk0, commitment, position)
	nullifier1 := noteReshapeIdentityNullifier(t, nk1, commitment, position)
	return noteReshapeIdentityOwnershipAliasProbe{
		Anchor:         noteReshapeIdentityRoot(t, commitment, position, nativePath),
		NoteCommitment: commitment,
		Nullifier0:     nullifier0,
		Nullifier1:     nullifier1,
		AK: circuits.Point2D{
			X: ak.X,
			Y: ak.Y,
		},
		DivGen:       circuits.Point2D{X: 0, Y: 1},
		NK0:          nk0,
		NK1:          nk1,
		IVKReduced0:  ivkReduced0,
		IVKQuotient0: ivkQuotient0,
		IVKReduced1:  ivkReduced1,
		IVKQuotient1: ivkQuotient1,
		Blinding:     1234,
		Amount:       55,
		AssetID:      777,
		Position:     position,
		Path:         path,
	}
}

func noteReshapeIdentityIVK(
	t *testing.T,
	nk *big.Int,
	akFq *big.Int,
) (*big.Int, *big.Int) {
	t.Helper()
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}
	ivkModQ, err := primitives.Poseidon377Hash2Native(
		primitives.MustBigInt(vectors.Poseidon377.IVKDomain),
		[2]*big.Int{nk, akFq},
	)
	if err != nil {
		t.Fatalf("compute incoming viewing key: %v", err)
	}
	quotient := new(big.Int)
	reduced := new(big.Int)
	quotient.QuoRem(ivkModQ, decaf377.ScalarOrder(), reduced)
	return reduced, quotient
}

func noteReshapeIdentityNullifier(
	t *testing.T,
	nk *big.Int,
	commitment *big.Int,
	position uint64,
) *big.Int {
	t.Helper()
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load prototype vectors: %v", err)
	}
	nullifier, err := primitives.Poseidon377Hash3Native(
		primitives.MustBigInt(vectors.Poseidon377.NullifierDomain),
		[3]*big.Int{nk, commitment, new(big.Int).SetUint64(position)},
	)
	if err != nil {
		t.Fatalf("compute nullifier: %v", err)
	}
	return nullifier
}

func noteReshapeIdentityRoot(
	t *testing.T,
	commitment *big.Int,
	position uint64,
	path [circuits.StateCommitmentDepth][3]*big.Int,
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

func TestNoteReshapeUnguardedIdentityDiversifierAliasesOwnership(
	t *testing.T,
) {
	assignment := noteReshapeIdentityOwnershipAliasAssignment(t)
	if err := test.IsSolved(
		&noteReshapeIdentityOwnershipAliasProbe{},
		&assignment,
		ecc.BLS12_377.ScalarField(),
	); err != nil {
		t.Fatalf("unguarded identity relation should remain satisfiable: %v", err)
	}

	hardened := hardenedNoteReshapeIdentityOwnershipAliasProbe{
		noteReshapeIdentityOwnershipAliasProbe: assignment,
	}
	if err := test.IsSolved(
		&hardenedNoteReshapeIdentityOwnershipAliasProbe{},
		&hardened,
		ecc.BLS12_377.ScalarField(),
	); err == nil {
		t.Fatal("the hardened relation must reject an identity diversified generator")
	}
}

func TestNoteReshapeCircuitRejectsIdentityAuthorizationAndDiversifiedGenerator(
	t *testing.T,
) {
	minusOne := new(big.Int).Sub(ecc.BLS12_377.ScalarField(), big.NewInt(1))
	for _, family := range generated.NoteReshapeFamilies {
		t.Run(family.Label, func(t *testing.T) {
			ccs := compileNoteReshapeRegressionCircuit(
				t,
				family.Label,
				family.NIn,
				family.NOut,
			)
			canonical := loadNoteReshapeRegressionAssignment(t, family.Label)
			if err := noteReshapeAssignmentSolved(t, ccs, canonical); err != nil {
				t.Fatalf("canonical witness must satisfy nonidentity guards: %v", err)
			}

			// IVK and transmission mutations also disturb derivation and commitment rows.
			for _, mutation := range []struct {
				name   string
				mutate func(*circuits.NoteReshapeCircuit)
			}{
				{
					name: "authorization_key_positive_representative",
					mutate: func(c *circuits.NoteReshapeCircuit) {
						c.Auth.AK = circuits.Point2D{X: 0, Y: 1}
					},
				},
				{
					name: "authorization_key_negative_representative",
					mutate: func(c *circuits.NoteReshapeCircuit) {
						c.Auth.AK = circuits.Point2D{X: 0, Y: minusOne}
					},
				},
				{
					name: "shared_diversified_generator_positive_representative",
					mutate: func(c *circuits.NoteReshapeCircuit) {
						c.Shared.DivGen = circuits.Point2D{X: 0, Y: 1}
					},
				},
				{
					name: "shared_diversified_generator_negative_representative",
					mutate: func(c *circuits.NoteReshapeCircuit) {
						c.Shared.DivGen = circuits.Point2D{X: 0, Y: minusOne}
					},
				},
				{
					name: "incoming_viewing_key_zero",
					mutate: func(c *circuits.NoteReshapeCircuit) {
						c.Auth.IVKReduced = 0
					},
				},
			} {
				t.Run(mutation.name, func(t *testing.T) {
					assignment := loadNoteReshapeRegressionAssignment(t, family.Label)
					mutation.mutate(assignment)
					if err := noteReshapeAssignmentSolved(t, ccs, assignment); err == nil {
						t.Fatalf("%s must reject the identity-class mutation", family.Label)
					}
				})
			}
		})
	}
}
