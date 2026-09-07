package circuits

import (
	"encoding/hex"
	"fmt"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"golang.org/x/crypto/blake2b"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

func ComplianceNullifierKeyCommitment(
	api frontend.API,
	rnk frontend.Variable,
) (frontend.Variable, error) {
	domain := blake2b.Sum512([]byte("shieldd.compliance.nullifier_key"))
	return Poseidon377Hash1(
		api,
		LittleEndianBytesToBigInt(domain[:]),
		rnk,
	)
}

func CompressedLEHexToBigInt(value string) (*big.Int, error) {
	bytes, err := hex.DecodeString(value)
	if err != nil {
		return nil, fmt.Errorf("decode hex %q: %w", value, err)
	}
	if len(bytes) != 32 {
		return nil, fmt.Errorf("expected 32 bytes, got %d", len(bytes))
	}
	reversed := make([]byte, len(bytes))
	for i := range bytes {
		reversed[len(bytes)-1-i] = bytes[i]
	}
	return new(big.Int).SetBytes(reversed), nil
}

func NoteCommitment(
	api frontend.API,
	noteBlinding frontend.Variable,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	diversifiedGenerator gnarkte.Point,
	transmissionKeyS frontend.Variable,
	recoveryCommitment frontend.Variable,
) (frontend.Variable, error) {
	diversifiedGeneratorFq, err := decafgnark.CompressToField(api, diversifiedGenerator)
	if err != nil {
		return nil, err
	}

	return NoteCommitmentWithCompressedDivGen(
		api,
		noteBlinding,
		noteAmount,
		noteAssetID,
		diversifiedGeneratorFq,
		transmissionKeyS,
		recoveryCommitment,
	)
}

func NoteCommitmentWithCompressedDivGen(
	api frontend.API,
	noteBlinding frontend.Variable,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	diversifiedGeneratorFq frontend.Variable,
	transmissionKeyS frontend.Variable,
	recoveryCommitment frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash6(
		api,
		MustBigInt(vectors.Poseidon377.NoteCommitDomain),
		[6]frontend.Variable{
			noteBlinding,
			noteAmount,
			noteAssetID,
			diversifiedGeneratorFq,
			transmissionKeyS,
			recoveryCommitment,
		},
	)
}

func ComplianceLeafCommitment(
	api frontend.API,
	diversifiedGenerator gnarkte.Point,
	transmissionKey gnarkte.Point,
	assetID frontend.Variable,
	capk gnarkte.Point,
	rnkDhPk gnarkte.Point,
	rnkCommitment frontend.Variable,
	status frontend.Variable,
) (frontend.Variable, error) {
	diversifiedGeneratorFq, err := decafgnark.CompressToField(api, diversifiedGenerator)
	if err != nil {
		return nil, err
	}
	transmissionKeyFq, err := decafgnark.CompressToField(api, transmissionKey)
	if err != nil {
		return nil, err
	}

	return ComplianceLeafCommitmentFromCompressed(
		api,
		diversifiedGeneratorFq,
		transmissionKeyFq,
		assetID,
		capk,
		rnkDhPk,
		rnkCommitment,
		status,
	)
}

func ComplianceLeafCommitmentFromCompressed(
	api frontend.API,
	diversifiedGeneratorFq frontend.Variable,
	transmissionKeyFq frontend.Variable,
	assetID frontend.Variable,
	capk gnarkte.Point,
	rnkDhPk gnarkte.Point,
	rnkCommitment frontend.Variable,
	status frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return nil, err
	}
	curve.AssertIsOnCurve(capk)
	AssertDecafNonIdentity(api, capk)
	curve.AssertIsOnCurve(rnkDhPk)
	AssertDecafNonIdentity(api, rnkDhPk)
	capkFq, err := decafgnark.CompressToField(api, capk)
	if err != nil {
		return nil, err
	}

	rnkDhPkFq, err := decafgnark.CompressToField(api, rnkDhPk)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash7(
		api,
		MustBigInt(vectors.Poseidon377.ComplianceLeafDomain),
		[7]frontend.Variable{
			diversifiedGeneratorFq,
			transmissionKeyFq,
			assetID,
			capkFq,
			rnkDhPkFq,
			rnkCommitment,
			status,
		},
	)
}

// AssertActiveComplianceLifecycle constrains the packed lifecycle value.
// Layout is status[3] || freeze_generation[64] || frozen_since_height[64],
// little-endian. An active leaf keeps its authenticated freeze generation but
// must have no current frozen-since height.
func AssertActiveComplianceLifecycle(
	api frontend.API,
	lifecycle frontend.Variable,
	enabled frontend.Variable,
) {
	bits := api.ToBinary(lifecycle, 131)
	status := api.FromBinary(bits[:3]...)
	frozenSinceHeight := api.FromBinary(bits[67:131]...)
	AssertEqualIf(api, status, 1, enabled)
	AssertEqualIf(api, frozenSinceHeight, 0, enabled)
}

func BlindSenderLeaf(
	api frontend.API,
	leafHash frontend.Variable,
	txBlindingNonce frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash3(
		api,
		MustBigInt(vectors.Poseidon377.SenderLeafDomain),
		[3]frontend.Variable{leafHash, txBlindingNonce, 0},
	)
}

func ValueGeneratorNative(assetID *big.Int) (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	hashedAssetID, err := Poseidon377Hash1Native(MustBigInt(vectors.Poseidon377.ValueGeneratorDomain), assetID)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return decafgnark.EncodeToCurveNative(hashedAssetID)
}

func ValueBlindingGeneratorNative() (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	return gnarkte.Point{
		X: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorX),
		Y: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorY),
	}, nil
}

func BalanceCommitmentFromFixtureNative(fixture SpendFixture) (gnarkte.Point, error) {
	valueGenerator, err := ValueGeneratorNative(MustBigInt(fixture.Private.NoteAssetID))
	if err != nil {
		return gnarkte.Point{}, err
	}
	valueBlindingGenerator, err := ValueBlindingGeneratorNative()
	if err != nil {
		return gnarkte.Point{}, err
	}

	valuePoint, err := decafgnark.ScalarMulNative(valueGenerator, MustBigInt(fixture.Private.NoteAmount), 128)
	if err != nil {
		return gnarkte.Point{}, err
	}
	blindingPoint, err := decafgnark.ScalarMulNative(valueBlindingGenerator, MustBigInt(fixture.Private.VBlinding), 256)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return decafgnark.PointAddNative(valuePoint, blindingPoint)
}

func BalanceCommitment(
	api frontend.API,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	vBlinding frontend.Variable,
) (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}

	hashedAssetID, err := Poseidon377Hash1(api, MustBigInt(vectors.Poseidon377.ValueGeneratorDomain), noteAssetID)
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	valueGenerator, err := decafgnark.EncodeToCurve(api, hashedAssetID)
	if err != nil {
		return gnarkte.Point{}, err
	}
	valueBlindingGenerator := gnarkte.Point{
		X: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorX),
		Y: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorY),
	}

	// The 128-bit width here is the canonical note-amount range bound
	// (ZK-ASSUME-AMOUNT-RANGE): ScalarMulLE's api.ToBinary(noteAmount, 128)
	// makes any amount >= 2^128 unsatisfiable, preventing field-overflow value
	// inflation. Load-bearing for balance soundness; pinned by
	// TestAmountRangeBoundIs128Bits.
	valuePoint := ScalarMulLE(api, curve, valueGenerator, noteAmount, 128)
	blindingPoint := ScalarMulLE(
		api,
		curve,
		valueBlindingGenerator,
		vBlinding,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	return curve.Add(valuePoint, blindingPoint), nil
}
