package compliance

import (
	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

const OutputCiphertextFQCount = 11

func AddressPlaintextFQs(
	api frontend.API,
	diversifiedGenerator gnarkte.Point,
	transmissionKey gnarkte.Point,
) ([]frontend.Variable, error) {
	diversifiedGeneratorFq, err := primitives.Decaf377CompressToField(api, diversifiedGenerator)
	if err != nil {
		return nil, err
	}
	transmissionKeyFq, err := primitives.Decaf377CompressToField(api, transmissionKey)
	if err != nil {
		return nil, err
	}

	return AddressPlaintextFQsFromCompressed(api, diversifiedGeneratorFq, transmissionKeyFq), nil
}

func AddressPlaintextFQsFromCompressed(
	api frontend.API,
	diversifiedGeneratorFq frontend.Variable,
	transmissionKeyFq frontend.Variable,
) []frontend.Variable {

	var bits []frontend.Variable
	divBits := api.ToBinary(diversifiedGeneratorFq, 32*8)
	bits = append(bits, divBits...)
	transBits := api.ToBinary(transmissionKeyFq, 32*8)
	bits = append(bits, transBits...)

	out := make([]frontend.Variable, 0, 3)
	for start := 0; start < len(bits); start += 31 * 8 {
		end := start + 31*8
		if end > len(bits) {
			end = len(bits)
		}
		out = append(out, api.FromBinary(bits[start:end]...))
	}
	return out
}

func DeriveSharedSecretsOutput(
	api frontend.API,
	r1 frontend.Variable,
	r2 frontend.Variable,
	r3 frontend.Variable,
	ackReceiver gnarkte.Point,
	ackSender gnarkte.Point,
	dkPub gnarkte.Point,
	isFlagged frontend.Variable,
	epk1 gnarkte.Point,
	epk2 gnarkte.Point,
	epk3 gnarkte.Point,
) (gnarkte.Point, gnarkte.Point, gnarkte.Point, gnarkte.Point, gnarkte.Point, gnarkte.Point, gnarkte.Point, error) {
	api.AssertIsBoolean(isFlagged)

	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, err
	}
	generator, err := decafGeneratorPoint()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, err
	}
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, err
	}
	orderBitLen := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()

	computedEpk1 := ScalarMulLE(api, curve, generator, r1, orderBitLen)
	computedEpk2 := ScalarMulLE(api, curve, generator, r2, orderBitLen)
	computedEpk3 := ScalarMulLE(api, curve, generator, r3, orderBitLen)
	primitives.AssertDecafEquivalent(api, computedEpk1, epk1)
	primitives.AssertDecafEquivalent(api, computedEpk2, epk2)
	primitives.AssertDecafEquivalent(api, computedEpk3, epk3)

	ssCoreUser := ScalarMulLE(api, curve, ackReceiver, r1, orderBitLen)
	ssExtUser := ScalarMulLE(api, curve, ackReceiver, r2, orderBitLen)
	ssSextUser := ScalarMulLE(api, curve, ackSender, r3, orderBitLen)
	ssIssuer1 := ScalarMulLE(api, curve, dkPub, r1, orderBitLen)
	ssIssuer2 := ScalarMulLE(api, curve, dkPub, r2, orderBitLen)
	ssIssuer3 := ScalarMulLE(api, curve, dkPub, r3, orderBitLen)

	selectPoint := func(flag frontend.Variable, flagged, unflagged gnarkte.Point) gnarkte.Point {
		return gnarkte.Point{
			X: api.Select(flag, flagged.X, unflagged.X),
			Y: api.Select(flag, flagged.Y, unflagged.Y),
		}
	}

	return ssIssuer1,
		ssCoreUser,
		ssExtUser,
		ssSextUser,
		selectPoint(isFlagged, ssIssuer1, ssCoreUser),
		selectPoint(isFlagged, ssIssuer2, ssExtUser),
		selectPoint(isFlagged, ssIssuer3, ssSextUser),
		nil
}

func VerifyPoseidonEncryptionOutput(
	api frontend.API,
	isRegulated frontend.Variable,
	isFlagged frontend.Variable,
	ssDetection gnarkte.Point,
	ssCore gnarkte.Point,
	ssExt gnarkte.Point,
	ssSext gnarkte.Point,
	c2Core frontend.Variable,
	c2Ext frontend.Variable,
	c2Sext frontend.Variable,
	epk1Fq frontend.Variable,
	salt frontend.Variable,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	selfDiversifiedGeneratorFq frontend.Variable,
	selfTransmissionKeyFq frontend.Variable,
	counterpartyDiversifiedGeneratorFq frontend.Variable,
	counterpartyTransmissionKeyFq frontend.Variable,
	complianceCiphertext [OutputCiphertextFQCount]frontend.Variable,
) error {
	api.AssertIsBoolean(isRegulated)
	api.AssertIsBoolean(isFlagged)

	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return err
	}
	ssDetectionFq, err := primitives.Decaf377CompressToField(api, ssDetection)
	if err != nil {
		return err
	}
	seedDetection, err := primitives.Poseidon377Hash2(
		api,
		primitives.MustBigInt(vectors.Poseidon377.IssuerDetectionDomain),
		[2]frontend.Variable{ssDetectionFq, epk1Fq},
	)
	if err != nil {
		return err
	}

	flagContribution := api.Mul(isFlagged, flagBitFq())
	detectionPlaintext := api.Add(noteAssetID, flagContribution)
	keystream0, err := primitives.Poseidon377Hash2(api, seedDetection, [2]frontend.Variable{0, seedDetection})
	if err != nil {
		return err
	}
	keystream1, err := primitives.Poseidon377Hash2(api, seedDetection, [2]frontend.Variable{1, seedDetection})
	if err != nil {
		return err
	}
	AssertEqualIf(api, api.Add(detectionPlaintext, keystream0), complianceCiphertext[0], isRegulated)
	AssertEqualIf(api, api.Add(salt, keystream1), complianceCiphertext[1], isRegulated)

	ssCoreFq, err := primitives.Decaf377CompressToField(api, ssCore)
	if err != nil {
		return err
	}
	seedCore := api.Sub(c2Core, ssCoreFq)
	corePlaintexts := SpendCorePlaintextFQsFromCompressed(api, noteAmount, selfDiversifiedGeneratorFq, selfTransmissionKeyFq)
	for i, plain := range corePlaintexts {
		keystream, err := primitives.Poseidon377Hash2(api, seedCore, [2]frontend.Variable{i, seedCore})
		if err != nil {
			return err
		}
		AssertEqualIf(api, api.Add(plain, keystream), complianceCiphertext[2+i], isRegulated)
	}

	ssExtFq, err := primitives.Decaf377CompressToField(api, ssExt)
	if err != nil {
		return err
	}
	seedExt := api.Sub(c2Ext, ssExtFq)
	extPlaintexts := AddressPlaintextFQsFromCompressed(api, counterpartyDiversifiedGeneratorFq, counterpartyTransmissionKeyFq)
	for i, plain := range extPlaintexts {
		keystream, err := primitives.Poseidon377Hash2(api, seedExt, [2]frontend.Variable{i, seedExt})
		if err != nil {
			return err
		}
		AssertEqualIf(api, api.Add(plain, keystream), complianceCiphertext[5+i], isRegulated)
	}

	ssSextFq, err := primitives.Decaf377CompressToField(api, ssSext)
	if err != nil {
		return err
	}
	seedSext := api.Sub(c2Sext, ssSextFq)
	for i, plain := range corePlaintexts {
		keystream, err := primitives.Poseidon377Hash2(api, seedSext, [2]frontend.Variable{i, seedSext})
		if err != nil {
			return err
		}
		AssertEqualIf(api, api.Add(plain, keystream), complianceCiphertext[8+i], isRegulated)
	}

	return nil
}
