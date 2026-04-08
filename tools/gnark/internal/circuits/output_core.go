package circuits

import (
	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

func AssertNegativeBalanceCommitment(
	api frontend.API,
	claimedBalanceCommitment gnarkte.Point,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	balanceBlinding frontend.Variable,
) error {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return err
	}
	hashedAssetID, err := Poseidon377Hash1(api, MustBigInt(vectors.Poseidon377.ValueGeneratorDomain), noteAssetID)
	if err != nil {
		return err
	}
	valueGenerator, err := Decaf377EncodeToCurve(api, hashedAssetID)
	if err != nil {
		return err
	}
	valueBlindingGenerator := gnarkte.Point{
		X: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorX),
		Y: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorY),
	}
	// Output balance commitment is -amount*H + blinding*G (negative value, positive blinding).
	valuePoint := compliance.ScalarMulLE(api, curve, valueGenerator, noteAmount, 128)
	negValuePoint := curve.Neg(valuePoint)
	blindingPoint := compliance.ScalarMulLE(
		api,
		curve,
		valueBlindingGenerator,
		balanceBlinding,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	commitment := curve.Add(negValuePoint, blindingPoint)
	AssertDecafEquivalent(api, commitment, claimedBalanceCommitment)
	return nil
}

func CounterpartyLeafHash(
	api frontend.API,
	counterpartyDiversifiedGenerator gnarkte.Point,
	counterpartyTransmissionKey gnarkte.Point,
	counterpartyAssetID frontend.Variable,
	counterpartyD frontend.Variable,
	txBlindingNonce frontend.Variable,
) (frontend.Variable, error) {
	counterpartyLeafCommitment, err := ComplianceLeafCommitment(
		api,
		counterpartyDiversifiedGenerator,
		counterpartyTransmissionKey,
		counterpartyAssetID,
		counterpartyD,
	)
	if err != nil {
		return nil, err
	}
	return BlindSenderLeaf(api, counterpartyLeafCommitment, txBlindingNonce)
}

func OutputStatementHashFromParts(
	api frontend.API,
	claimedNoteCommitment frontend.Variable,
	balanceCommitmentFq frontend.Variable,
	epk1Fq frontend.Variable,
	epk2Fq frontend.Variable,
	epk3Fq frontend.Variable,
	assetAnchor frontend.Variable,
	complianceAnchor frontend.Variable,
	c2Core frontend.Variable,
	c2Ext frontend.Variable,
	c2Sext frontend.Variable,
	complianceCiphertext [compliance.OutputCiphertextFQCount]frontend.Variable,
	targetTimestamp frontend.Variable,
	dleqC1 frontend.Variable,
	dleqS1 frontend.Variable,
	dleqC2 frontend.Variable,
	dleqS2 frontend.Variable,
	dleqC3 frontend.Variable,
	dleqS3 frontend.Variable,
	counterpartyLeafHash frontend.Variable,
) (frontend.Variable, error) {
	fields := make([]frontend.Variable, 0, OutputStatementFieldCount)
	fields = append(fields,
		claimedNoteCommitment,
		balanceCommitmentFq,
		assetAnchor,
		complianceAnchor,
		epk1Fq,
		epk2Fq,
		epk3Fq,
		c2Core,
		c2Ext,
		c2Sext,
	)
	for i := range complianceCiphertext {
		fields = append(fields, complianceCiphertext[i])
	}
	fields = append(fields,
		targetTimestamp,
		dleqC1,
		dleqS1,
		dleqC2,
		dleqS2,
		dleqC3,
		dleqS3,
		counterpartyLeafHash,
	)
	return OutputStatementHash(api, fields)
}
