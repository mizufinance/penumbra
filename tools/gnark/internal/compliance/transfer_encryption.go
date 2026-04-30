package compliance

import (
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"math/big"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
	"golang.org/x/crypto/blake2b"
)

const (
	TransferDetectionFQCount      = 2
	TransferCoreCiphertextFQCount = 1
	TransferExtCiphertextFQCount  = 3
)

var (
	TransferSaltDomain          = transferSaltConstant("penumbra.transfer.compliance.salt")
	TransferDetectionSaltLabel  = transferSaltConstant("detection")
	TransferSenderCoreSaltLabel = transferSaltConstant("sender_core")
	TransferSenderExtSaltLabel  = transferSaltConstant("sender_ext")
	TransferOutputCoreSaltLabel = transferSaltConstant("output_core")
	TransferOutputExtSaltLabel  = transferSaltConstant("output_ext")
)

func transferSaltConstant(label string) *big.Int {
	sum := blake2b.Sum512([]byte(label))
	return primitives.LittleEndianBytesToBigInt(sum[:])
}

func DeriveTransferSalt(
	api frontend.API,
	transferNonceRoot frontend.Variable,
	label *big.Int,
) (frontend.Variable, error) {
	return primitives.Poseidon377Hash2(
		api,
		TransferSaltDomain,
		[2]frontend.Variable{transferNonceRoot, label},
	)
}

func ThresholdFlag(api frontend.API, amount, threshold frontend.Variable) frontend.Variable {
	return api.Sub(1, fieldLessThan(api, amount, threshold))
}

func VerifyPoseidonEncryptionTransferDetection(
	api frontend.API,
	isRegulated frontend.Variable,
	isFlagged frontend.Variable,
	ssDetection gnarkte.Point,
	senderCoreEPKFq frontend.Variable,
	detectionSalt frontend.Variable,
	assetID frontend.Variable,
	ciphertext [TransferDetectionFQCount]frontend.Variable,
) error {
	api.AssertIsBoolean(isRegulated)
	api.AssertIsBoolean(isFlagged)

	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return err
	}
	ssDetectionFq, err := decafgnark.CompressToField(api, ssDetection)
	if err != nil {
		return err
	}
	seedDetection, err := primitives.Poseidon377Hash2(
		api,
		primitives.MustBigInt(vectors.Poseidon377.IssuerDetectionDomain),
		[2]frontend.Variable{ssDetectionFq, senderCoreEPKFq},
	)
	if err != nil {
		return err
	}

	detectionPlaintext := api.Add(assetID, api.Mul(isFlagged, flagBitFq()))
	keystream0, err := primitives.Poseidon377Hash2(api, seedDetection, [2]frontend.Variable{0, seedDetection})
	if err != nil {
		return err
	}
	keystream1, err := primitives.Poseidon377Hash2(api, seedDetection, [2]frontend.Variable{1, seedDetection})
	if err != nil {
		return err
	}

	AssertEqualIf(api, api.Add(detectionPlaintext, keystream0), ciphertext[0], isRegulated)
	AssertEqualIf(api, api.Add(detectionSalt, keystream1), ciphertext[1], isRegulated)
	return nil
}

func VerifyPoseidonEncryptionTransferAmount(
	api frontend.API,
	isRegulated frontend.Variable,
	sharedSecret gnarkte.Point,
	c2 frontend.Variable,
	amount frontend.Variable,
	ciphertext [TransferCoreCiphertextFQCount]frontend.Variable,
) error {
	api.AssertIsBoolean(isRegulated)

	sharedSecretFq, err := decafgnark.CompressToField(api, sharedSecret)
	if err != nil {
		return err
	}
	seed := api.Sub(c2, sharedSecretFq)
	keystream, err := primitives.Poseidon377Hash2(api, seed, [2]frontend.Variable{0, seed})
	if err != nil {
		return err
	}
	AssertEqualIf(api, api.Add(amount, keystream), ciphertext[0], isRegulated)
	return nil
}

func VerifyPoseidonEncryptionTransferAddress(
	api frontend.API,
	isRegulated frontend.Variable,
	sharedSecret gnarkte.Point,
	c2 frontend.Variable,
	diversifiedGeneratorFq frontend.Variable,
	transmissionKeyFq frontend.Variable,
	ciphertext [TransferExtCiphertextFQCount]frontend.Variable,
) error {
	api.AssertIsBoolean(isRegulated)

	sharedSecretFq, err := decafgnark.CompressToField(api, sharedSecret)
	if err != nil {
		return err
	}
	seed := api.Sub(c2, sharedSecretFq)
	plaintexts := AddressPlaintextFQsFromCompressed(api, diversifiedGeneratorFq, transmissionKeyFq)
	for i, plain := range plaintexts {
		keystream, err := primitives.Poseidon377Hash2(api, seed, [2]frontend.Variable{i, seed})
		if err != nil {
			return err
		}
		AssertEqualIf(api, api.Add(plain, keystream), ciphertext[i], isRegulated)
	}
	return nil
}
