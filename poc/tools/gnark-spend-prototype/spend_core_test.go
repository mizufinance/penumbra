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

func TestNoteCommitmentFromFixtureNativeMatchesRust(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	commitment, err := NoteCommitmentFromFixtureNative(fixture)
	if err != nil {
		t.Fatalf("compute note commitment: %v", err)
	}

	if got, want := commitment.String(), fixture.Private.StateCommitmentProof.Commitment; got != want {
		t.Fatalf("note commitment mismatch: got %s want %s", got, want)
	}
}

func TestNullifierFromFixtureNativeMatchesRust(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	nullifier, err := NullifierFromFixtureNative(fixture)
	if err != nil {
		t.Fatalf("compute nullifier: %v", err)
	}

	if got, want := nullifier.String(), fixture.Public.Nullifier; got != want {
		t.Fatalf("nullifier mismatch: got %s want %s", got, want)
	}
}

type spendCoreIntegrityCircuit struct {
	NoteBlinding     frontend.Variable
	NoteAmount       frontend.Variable
	NoteAssetID      frontend.Variable
	DiversifiedGenX  frontend.Variable
	DiversifiedGenY  frontend.Variable
	TransmissionKeyS frontend.Variable
	ClueKey          frontend.Variable
	NK               frontend.Variable
	StateCommitment  frontend.Variable
	Position         frontend.Variable

	ExpectedCommitment frontend.Variable `gnark:",public"`
	ExpectedNullifier  frontend.Variable `gnark:",public"`
}

func (c *spendCoreIntegrityCircuit) Define(api frontend.API) error {
	commitment, err := NoteCommitment(
		api,
		c.NoteBlinding,
		c.NoteAmount,
		c.NoteAssetID,
		gnarkte.Point{X: c.DiversifiedGenX, Y: c.DiversifiedGenY},
		c.TransmissionKeyS,
		c.ClueKey,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(commitment, c.ExpectedCommitment)

	nullifier, err := Nullifier(api, c.NK, c.StateCommitment, c.Position)
	if err != nil {
		return err
	}
	api.AssertIsEqual(nullifier, c.ExpectedNullifier)
	return nil
}

func TestSpendCoreIntegrityCircuitMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	transmissionKeyS, err := compressedLEHexToBigInt(fixture.Private.TransmissionKeyHex)
	if err != nil {
		t.Fatalf("parse transmission key: %v", err)
	}

	assignment := &spendCoreIntegrityCircuit{
		NoteBlinding:       fixture.Private.NoteBlinding,
		NoteAmount:         fixture.Private.NoteAmount,
		NoteAssetID:        fixture.Private.NoteAssetID,
		DiversifiedGenX:    fixture.Private.DiversifiedGeneratorAffine.X,
		DiversifiedGenY:    fixture.Private.DiversifiedGeneratorAffine.Y,
		TransmissionKeyS:   transmissionKeyS.String(),
		ClueKey:            fixture.Private.ClueKey,
		NK:                 fixture.Private.NK,
		StateCommitment:    fixture.Private.StateCommitmentProof.Commitment,
		Position:           fixture.Private.StateCommitmentProof.Position,
		ExpectedCommitment: fixture.Private.StateCommitmentProof.Commitment,
		ExpectedNullifier:  fixture.Public.Nullifier,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&spendCoreIntegrityCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestSpendCoreIntegrityCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &spendCoreIntegrityCircuit{})
	if err != nil {
		t.Fatalf("compile spend core integrity circuit: %v", err)
	}
}

type balanceCommitmentCircuit struct {
	NoteAmount  frontend.Variable
	NoteAssetID frontend.Variable
	VBlinding   frontend.Variable

	ExpectedX frontend.Variable `gnark:",public"`
	ExpectedY frontend.Variable `gnark:",public"`
}

func (c *balanceCommitmentCircuit) Define(api frontend.API) error {
	commitment, err := BalanceCommitment(api, c.NoteAmount, c.NoteAssetID, c.VBlinding)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, commitment, gnarkte.Point{X: c.ExpectedX, Y: c.ExpectedY})
	return nil
}

func TestBalanceCommitmentFromFixtureNativeMatchesRust(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	commitment, err := BalanceCommitmentFromFixtureNative(fixture)
	if err != nil {
		t.Fatalf("compute balance commitment: %v", err)
	}

	if got, want := commitment.X.(*big.Int).String(), fixture.Public.BalanceCommitmentAffine.X; got != want {
		t.Fatalf("balance commitment x mismatch: got %s want %s", got, want)
	}
	if got, want := commitment.Y.(*big.Int).String(), fixture.Public.BalanceCommitmentAffine.Y; got != want {
		t.Fatalf("balance commitment y mismatch: got %s want %s", got, want)
	}
}

func TestBalanceCommitmentCircuitMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	assignment := &balanceCommitmentCircuit{
		NoteAmount:  fixture.Private.NoteAmount,
		NoteAssetID: fixture.Private.NoteAssetID,
		VBlinding:   fixture.Private.VBlinding,
		ExpectedX:   fixture.Public.BalanceCommitmentAffine.X,
		ExpectedY:   fixture.Public.BalanceCommitmentAffine.Y,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&balanceCommitmentCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestBalanceCommitmentCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &balanceCommitmentCircuit{})
	if err != nil {
		t.Fatalf("compile balance commitment circuit: %v", err)
	}
}

type randomizedVerificationKeyCircuit struct {
	AKX                 frontend.Variable
	AKY                 frontend.Variable
	SpendAuthRandomizer frontend.Variable

	ExpectedX frontend.Variable `gnark:",public"`
	ExpectedY frontend.Variable `gnark:",public"`
}

func (c *randomizedVerificationKeyCircuit) Define(api frontend.API) error {
	rk, err := RandomizedVerificationKey(
		api,
		gnarkte.Point{X: c.AKX, Y: c.AKY},
		c.SpendAuthRandomizer,
	)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, rk, gnarkte.Point{X: c.ExpectedX, Y: c.ExpectedY})
	return nil
}

func TestRandomizedVerificationKeyNativeMatchesRust(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	rk, err := RandomizedVerificationKeyNative(fixture)
	if err != nil {
		t.Fatalf("compute randomized verification key: %v", err)
	}

	if got, want := rk.X.(*big.Int).String(), fixture.Public.RKAffine.X; got != want {
		t.Fatalf("randomized verification key x mismatch: got %s want %s", got, want)
	}
	if got, want := rk.Y.(*big.Int).String(), fixture.Public.RKAffine.Y; got != want {
		t.Fatalf("randomized verification key y mismatch: got %s want %s", got, want)
	}
}

func TestRandomizedVerificationKeyCircuitMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	assignment := &randomizedVerificationKeyCircuit{
		AKX:                 fixture.Private.AKAffine.X,
		AKY:                 fixture.Private.AKAffine.Y,
		SpendAuthRandomizer: fixture.Private.SpendAuthRandomizer,
		ExpectedX:           fixture.Public.RKAffine.X,
		ExpectedY:           fixture.Public.RKAffine.Y,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&randomizedVerificationKeyCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestRandomizedVerificationKeyCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &randomizedVerificationKeyCircuit{})
	if err != nil {
		t.Fatalf("compile randomized verification key circuit: %v", err)
	}
}

type diversifiedTransmissionKeyCircuit struct {
	NK              frontend.Variable
	AKX             frontend.Variable
	AKY             frontend.Variable
	DiversifiedGenX frontend.Variable
	DiversifiedGenY frontend.Variable
	IVKReduced      frontend.Variable
	IVKQuotientA    frontend.Variable

	ExpectedX frontend.Variable `gnark:",public"`
	ExpectedY frontend.Variable `gnark:",public"`
}

func (c *diversifiedTransmissionKeyCircuit) Define(api frontend.API) error {
	pk, err := DiversifiedTransmissionKey(
		api,
		c.NK,
		gnarkte.Point{X: c.AKX, Y: c.AKY},
		gnarkte.Point{X: c.DiversifiedGenX, Y: c.DiversifiedGenY},
		c.IVKReduced,
		c.IVKQuotientA,
	)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, pk, gnarkte.Point{X: c.ExpectedX, Y: c.ExpectedY})
	return nil
}

func TestDiversifiedTransmissionKeyNativeMatchesRust(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	pk, err := DiversifiedTransmissionKeyNative(fixture)
	if err != nil {
		t.Fatalf("compute diversified transmission key: %v", err)
	}

	if got, want := pk.X.(*big.Int).String(), fixture.Private.TransmissionKeyAffine.X; got != want {
		t.Fatalf("transmission key x mismatch: got %s want %s", got, want)
	}
	if got, want := pk.Y.(*big.Int).String(), fixture.Private.TransmissionKeyAffine.Y; got != want {
		t.Fatalf("transmission key y mismatch: got %s want %s", got, want)
	}
}

func TestDiversifiedTransmissionKeyCircuitMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}
	ivkReduced, quotientA, err := IncomingViewingKeyReductionNative(fixture)
	if err != nil {
		t.Fatalf("derive incoming viewing key reduction: %v", err)
	}

	assignment := &diversifiedTransmissionKeyCircuit{
		NK:              fixture.Private.NK,
		AKX:             fixture.Private.AKAffine.X,
		AKY:             fixture.Private.AKAffine.Y,
		DiversifiedGenX: fixture.Private.DiversifiedGeneratorAffine.X,
		DiversifiedGenY: fixture.Private.DiversifiedGeneratorAffine.Y,
		IVKReduced:      ivkReduced.String(),
		IVKQuotientA:    quotientA,
		ExpectedX:       fixture.Private.TransmissionKeyAffine.X,
		ExpectedY:       fixture.Private.TransmissionKeyAffine.Y,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&diversifiedTransmissionKeyCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestDiversifiedTransmissionKeyCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &diversifiedTransmissionKeyCircuit{})
	if err != nil {
		t.Fatalf("compile diversified transmission key circuit: %v", err)
	}
}

type complianceLeafCommitmentCircuit struct {
	DiversifiedGenX  frontend.Variable
	DiversifiedGenY  frontend.Variable
	TransmissionKeyX frontend.Variable
	TransmissionKeyY frontend.Variable
	AssetID          frontend.Variable
	D                frontend.Variable

	Expected frontend.Variable `gnark:",public"`
}

func (c *complianceLeafCommitmentCircuit) Define(api frontend.API) error {
	commitment, err := ComplianceLeafCommitment(
		api,
		gnarkte.Point{X: c.DiversifiedGenX, Y: c.DiversifiedGenY},
		gnarkte.Point{X: c.TransmissionKeyX, Y: c.TransmissionKeyY},
		c.AssetID,
		c.D,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(commitment, c.Expected)
	return nil
}

func TestComplianceLeafCommitmentNativeMatchesRust(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	commitment, err := ComplianceLeafCommitmentFromFixtureNative(fixture)
	if err != nil {
		t.Fatalf("compute compliance leaf commitment: %v", err)
	}

	if got, want := commitment.String(), fixture.Private.UserLeafCommitment; got != want {
		t.Fatalf("compliance leaf commitment mismatch: got %s want %s", got, want)
	}

	assignment := &complianceLeafCommitmentCircuit{
		DiversifiedGenX:  fixture.Private.UserDiversifiedGeneratorAffine.X,
		DiversifiedGenY:  fixture.Private.UserDiversifiedGeneratorAffine.Y,
		TransmissionKeyX: fixture.Private.UserTransmissionKeyAffine.X,
		TransmissionKeyY: fixture.Private.UserTransmissionKeyAffine.Y,
		AssetID:          fixture.Private.NoteAssetID,
		D:                fixture.Private.UserDDecimal,
		Expected:         commitment.String(),
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&complianceLeafCommitmentCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestComplianceLeafCommitmentCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &complianceLeafCommitmentCircuit{})
	if err != nil {
		t.Fatalf("compile compliance leaf commitment circuit: %v", err)
	}
}

type senderLeafBindingCircuit struct {
	DiversifiedGenX  frontend.Variable
	DiversifiedGenY  frontend.Variable
	TransmissionKeyX frontend.Variable
	TransmissionKeyY frontend.Variable
	AssetID          frontend.Variable
	D                frontend.Variable
	TxBlindingNonce  frontend.Variable

	Expected frontend.Variable `gnark:",public"`
}

func (c *senderLeafBindingCircuit) Define(api frontend.API) error {
	leafHash, err := ComplianceLeafCommitment(
		api,
		gnarkte.Point{X: c.DiversifiedGenX, Y: c.DiversifiedGenY},
		gnarkte.Point{X: c.TransmissionKeyX, Y: c.TransmissionKeyY},
		c.AssetID,
		c.D,
	)
	if err != nil {
		return err
	}
	blinded, err := BlindSenderLeaf(api, leafHash, c.TxBlindingNonce)
	if err != nil {
		return err
	}
	api.AssertIsEqual(blinded, c.Expected)
	return nil
}

func TestBlindSenderLeafNativeMatchesRust(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	blinded, err := BlindSenderLeafFromFixtureNative(fixture)
	if err != nil {
		t.Fatalf("compute blinded sender leaf: %v", err)
	}

	if got, want := blinded.String(), fixture.Public.SenderLeafHash; got != want {
		t.Fatalf("sender leaf hash mismatch: got %s want %s", got, want)
	}
}

func TestBlindSenderLeafCircuitMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	assignment := &senderLeafBindingCircuit{
		DiversifiedGenX:  fixture.Private.UserDiversifiedGeneratorAffine.X,
		DiversifiedGenY:  fixture.Private.UserDiversifiedGeneratorAffine.Y,
		TransmissionKeyX: fixture.Private.UserTransmissionKeyAffine.X,
		TransmissionKeyY: fixture.Private.UserTransmissionKeyAffine.Y,
		AssetID:          fixture.Private.NoteAssetID,
		D:                fixture.Private.UserDDecimal,
		TxBlindingNonce:  fixture.Private.TxBlindingNonce,
		Expected:         fixture.Public.SenderLeafHash,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&senderLeafBindingCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestSenderLeafBindingCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &senderLeafBindingCircuit{})
	if err != nil {
		t.Fatalf("compile sender leaf binding circuit: %v", err)
	}
}
