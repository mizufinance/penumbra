package circuits

import (
	"crypto/rand"
	"math/big"
	"testing"

	decaf377 "github.com/mizufinance/decaf377-go"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
)

// sharedDivGenCommitmentParityCircuit pins T1-f: the note commitment computed
// with a per-note div_gen compress must equal the note commitment computed
// from the shared div_gen's compress, for any noteDivGen that is decaf
// equivalent to sharedDivGen (the only relation the production circuit
// enforces on the per-note div_gen after T1-f).
type sharedDivGenCommitmentParityCircuit struct {
	Blinding         frontend.Variable
	Amount           frontend.Variable
	AssetID          frontend.Variable
	TransmissionKeyS frontend.Variable

	SharedDivGenX frontend.Variable `gnark:",public"`
	SharedDivGenY frontend.Variable `gnark:",public"`
	NoteDivGenX   frontend.Variable `gnark:",public"`
	NoteDivGenY   frontend.Variable `gnark:",public"`
}

func (c *sharedDivGenCommitmentParityCircuit) Define(api frontend.API) error {
	sharedDivGen := gnarkte.Point{X: c.SharedDivGenX, Y: c.SharedDivGenY}
	noteDivGen := gnarkte.Point{X: c.NoteDivGenX, Y: c.NoteDivGenY}

	// Production invariant: every note's div_gen is asserted decaf-equivalent
	// to the shared div_gen (note_reshape_circuit.go verifyNoteReshapeSpend
	// / verifyNoteReshapeOutput).
	decafgnark.AssertEquivalent(api, noteDivGen, sharedDivGen)

	// Pre-T1-f: per-note compress feeding the note commitment.
	perNoteFq, err := decafgnark.CompressToField(api, noteDivGen)
	if err != nil {
		return err
	}
	perNoteCommitment, err := NoteCommitmentWithCompressedDivGen(
		api, c.Blinding, c.Amount, c.AssetID, perNoteFq, c.TransmissionKeyS, 0,
	)
	if err != nil {
		return err
	}

	// Post-T1-f: shared compress computed once, reused for the note commitment.
	sharedFq, err := decafgnark.CompressToField(api, sharedDivGen)
	if err != nil {
		return err
	}
	sharedCommitment, err := NoteCommitmentWithCompressedDivGen(
		api, c.Blinding, c.Amount, c.AssetID, sharedFq, c.TransmissionKeyS, 0,
	)
	if err != nil {
		return err
	}

	api.AssertIsEqual(perNoteCommitment, sharedCommitment)
	return nil
}

func TestSharedDivGenCompressCommitmentParity(t *testing.T) {
	gen, err := decaf377.Generator()
	if err != nil {
		t.Fatal(err)
	}
	scalar, err := rand.Int(rand.Reader, decaf377.ScalarOrder())
	if err != nil {
		t.Fatal(err)
	}
	shared, err := decaf377.ScalarMul(gen, scalar)
	if err != nil {
		t.Fatal(err)
	}

	blinding := randFieldElement(t)
	amount := randFieldElement(t)
	assetID := randFieldElement(t)
	transmissionKeyS := randFieldElement(t)

	// Case 1: note.div_gen is the SAME representative as shared.div_gen.
	assignmentSame := &sharedDivGenCommitmentParityCircuit{
		Blinding: blinding, Amount: amount, AssetID: assetID,
		TransmissionKeyS: transmissionKeyS,
		SharedDivGenX:    shared.X, SharedDivGenY: shared.Y,
		NoteDivGenX: shared.X, NoteDivGenY: shared.Y,
	}
	if err := test.IsSolved(
		&sharedDivGenCommitmentParityCircuit{},
		assignmentSame,
		ecc.BLS12_377.ScalarField(),
	); err != nil {
		t.Fatalf("same-representative case: shared-compress commitment disagrees with per-note-compress commitment: %v", err)
	}

	// Case 2: note.div_gen is a GENUINELY DIFFERENT representative in the same
	// 2-torsion coset. On a twisted Edwards curve the order-2 point is
	// (0,-1); adding it to any point P=(x,y) yields P+T=(-x,-y) (the unified
	// addition law collapses to coordinate negation for this T), which is
	// equivalent to P under decaf's X_l*Y_r = X_r*Y_l relation
	// (x*(-y) = (-x)*y) but is a distinct affine pair whenever x,y != 0.
	fieldModulus := decaf377.FieldModulus()
	negX := new(big.Int).Neg(shared.X)
	negX.Mod(negX, fieldModulus)
	negY := new(big.Int).Neg(shared.Y)
	negY.Mod(negY, fieldModulus)

	assignmentDifferentRepresentative := &sharedDivGenCommitmentParityCircuit{
		Blinding: blinding, Amount: amount, AssetID: assetID,
		TransmissionKeyS: transmissionKeyS,
		SharedDivGenX:    shared.X, SharedDivGenY: shared.Y,
		NoteDivGenX: negX, NoteDivGenY: negY,
	}
	if err := test.IsSolved(
		&sharedDivGenCommitmentParityCircuit{},
		assignmentDifferentRepresentative,
		ecc.BLS12_377.ScalarField(),
	); err != nil {
		t.Fatalf("different-representative (P+T) case: shared-compress commitment disagrees with per-note-compress commitment: %v", err)
	}
}

func randFieldElement(t *testing.T) *big.Int {
	t.Helper()
	v, err := rand.Int(rand.Reader, decaf377.FieldModulus())
	if err != nil {
		t.Fatal(err)
	}
	return v
}
