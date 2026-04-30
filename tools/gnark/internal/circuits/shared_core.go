package circuits

import (
	"encoding/hex"
	"fmt"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/mizufinance/penumbra/tools/gnark/internal/compliance"
	. "github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

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

func NoteCommitmentFromFixtureNative(fixture SpendFixture) (*big.Int, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	diversifiedGenerator := PointAffineToNative(fixture.Private.DiversifiedGeneratorAffine)
	diversifiedGeneratorFq, err := decafgnark.CompressToFieldNative(diversifiedGenerator)
	if err != nil {
		return nil, err
	}
	transmissionKeyS, err := CompressedLEHexToBigInt(fixture.Private.TransmissionKeyHex)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash6Native(
		MustBigInt(vectors.Poseidon377.NoteCommitDomain),
		[6]*big.Int{
			MustBigInt(fixture.Private.NoteBlinding),
			MustBigInt(fixture.Private.NoteAmount),
			MustBigInt(fixture.Private.NoteAssetID),
			diversifiedGeneratorFq,
			transmissionKeyS,
			MustBigInt(fixture.Private.ClueKey),
		},
	)
}

func NoteCommitment(
	api frontend.API,
	noteBlinding frontend.Variable,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	diversifiedGenerator gnarkte.Point,
	transmissionKeyS frontend.Variable,
	clueKey frontend.Variable,
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
		clueKey,
	)
}

func NoteCommitmentWithCompressedDivGen(
	api frontend.API,
	noteBlinding frontend.Variable,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	diversifiedGeneratorFq frontend.Variable,
	transmissionKeyS frontend.Variable,
	clueKey frontend.Variable,
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
			clueKey,
		},
	)
}

func ComplianceLeafCommitmentFromFixtureNative(fixture SpendFixture) (*big.Int, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	diversifiedGeneratorFq, err := decafgnark.CompressToFieldNative(
		PointAffineToNative(fixture.Private.UserDiversifiedGeneratorAffine),
	)
	if err != nil {
		return nil, err
	}
	transmissionKeyFq, err := decafgnark.CompressToFieldNative(
		PointAffineToNative(fixture.Private.UserTransmissionKeyAffine),
	)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash4Native(
		MustBigInt(vectors.Poseidon377.ComplianceLeafDomain),
		[4]*big.Int{
			diversifiedGeneratorFq,
			transmissionKeyFq,
			MustBigInt(fixture.Private.NoteAssetID),
			MustBigInt(fixture.Private.UserDDecimal),
		},
	)
}

func ComplianceLeafCommitment(
	api frontend.API,
	diversifiedGenerator gnarkte.Point,
	transmissionKey gnarkte.Point,
	assetID frontend.Variable,
	d frontend.Variable,
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
		d,
	)
}

func ComplianceLeafCommitmentFromCompressed(
	api frontend.API,
	diversifiedGeneratorFq frontend.Variable,
	transmissionKeyFq frontend.Variable,
	assetID frontend.Variable,
	d frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash4(
		api,
		MustBigInt(vectors.Poseidon377.ComplianceLeafDomain),
		[4]frontend.Variable{
			diversifiedGeneratorFq,
			transmissionKeyFq,
			assetID,
			d,
		},
	)
}

func BlindSenderLeafFromFixtureNative(fixture SpendFixture) (*big.Int, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	leafCommitment, err := ComplianceLeafCommitmentFromFixtureNative(fixture)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash3Native(
		MustBigInt(vectors.Poseidon377.SenderLeafDomain),
		[3]*big.Int{
			leafCommitment,
			MustBigInt(fixture.Private.TxBlindingNonce),
			big.NewInt(0),
		},
	)
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
