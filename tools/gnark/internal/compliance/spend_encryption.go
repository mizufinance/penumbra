package compliance

import (
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

func DeriveSharedSecretsSpendNative(
	esk *big.Int,
	ackCore gnarkte.Point,
	dkPub gnarkte.Point,
	isFlagged bool,
) (gnarkte.Point, gnarkte.Point, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	nBits := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()
	ssCoreUser, err := primitives.ScalarMulNative(ackCore, esk, nBits)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	ssIssuer, err := primitives.ScalarMulNative(dkPub, esk, nBits)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	if isFlagged {
		return ssIssuer, ssIssuer, nil
	}
	return ssIssuer, ssCoreUser, nil
}

func DeriveSharedSecretsSpend(
	api frontend.API,
	esk frontend.Variable,
	ackCore gnarkte.Point,
	dkPub gnarkte.Point,
	isFlagged frontend.Variable,
	publishedEPK gnarkte.Point,
) (gnarkte.Point, gnarkte.Point, gnarkte.Point, error) {
	api.AssertIsBoolean(isFlagged)
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, err
	}
	generator, err := decafGeneratorPoint()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, err
	}
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, gnarkte.Point{}, err
	}
	nBits := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()

	computedEPK := ScalarMulLE(api, curve, generator, esk, nBits)
	primitives.AssertDecafEquivalent(api, computedEPK, publishedEPK)

	ssCoreUser := ScalarMulLE(api, curve, ackCore, esk, nBits)
	ssIssuer := ScalarMulLE(api, curve, dkPub, esk, nBits)
	ssCore := gnarkte.Point{
		X: api.Select(isFlagged, ssIssuer.X, ssCoreUser.X),
		Y: api.Select(isFlagged, ssIssuer.Y, ssCoreUser.Y),
	}
	return ssIssuer, ssCoreUser, ssCore, nil
}

func bigIntToFixedLE(value *big.Int, size int) []byte {
	out := make([]byte, size)
	be := value.Bytes()
	for i := 0; i < len(be) && i < size; i++ {
		out[i] = be[len(be)-1-i]
	}
	return out
}

func packBytesToFqChunksLE(input []byte, chunkSize, chunkCount int) []*big.Int {
	out := make([]*big.Int, chunkCount)
	for i := 0; i < chunkCount; i++ {
		start := i * chunkSize
		end := start + chunkSize
		if end > len(input) {
			end = len(input)
		}
		chunk := []byte{}
		if start < len(input) {
			chunk = input[start:end]
		}
		out[i] = primitives.LittleEndianBytesToBigInt(chunk)
	}
	return out
}

func SpendCorePlaintextFQsNative(
	noteAmount *big.Int,
	selfDiversifiedGenerator gnarkte.Point,
	selfTransmissionKey gnarkte.Point,
) ([]*big.Int, error) {
	selfDiversifiedGeneratorFq, err := primitives.Decaf377CompressToFieldNative(selfDiversifiedGenerator)
	if err != nil {
		return nil, err
	}
	selfTransmissionKeyFq, err := primitives.Decaf377CompressToFieldNative(selfTransmissionKey)
	if err != nil {
		return nil, err
	}

	var payload []byte
	payload = append(payload, bigIntToFixedLE(noteAmount, 16)...)
	payload = append(payload, bigIntToFixedLE(selfDiversifiedGeneratorFq, 32)...)
	payload = append(payload, bigIntToFixedLE(selfTransmissionKeyFq, 32)...)
	return packBytesToFqChunksLE(payload, 31, 3), nil
}

func SpendCorePlaintextFQs(
	api frontend.API,
	noteAmount frontend.Variable,
	selfDiversifiedGenerator gnarkte.Point,
	selfTransmissionKey gnarkte.Point,
) ([]frontend.Variable, error) {
	selfDiversifiedGeneratorFq, err := primitives.Decaf377CompressToField(api, selfDiversifiedGenerator)
	if err != nil {
		return nil, err
	}
	selfTransmissionKeyFq, err := primitives.Decaf377CompressToField(api, selfTransmissionKey)
	if err != nil {
		return nil, err
	}

	return SpendCorePlaintextFQsFromCompressed(api, noteAmount, selfDiversifiedGeneratorFq, selfTransmissionKeyFq), nil
}

func SpendCorePlaintextFQsFromCompressed(
	api frontend.API,
	noteAmount frontend.Variable,
	selfDiversifiedGeneratorFq frontend.Variable,
	selfTransmissionKeyFq frontend.Variable,
) []frontend.Variable {

	var bits []frontend.Variable
	amountBits := api.ToBinary(noteAmount, 16*8)
	bits = append(bits, amountBits...)
	selfDiversifiedGeneratorBits := api.ToBinary(selfDiversifiedGeneratorFq, 32*8)
	bits = append(bits, selfDiversifiedGeneratorBits...)
	selfTransmissionKeyBits := api.ToBinary(selfTransmissionKeyFq, 32*8)
	bits = append(bits, selfTransmissionKeyBits...)

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

func modSub(left, right *big.Int) *big.Int {
	result := new(big.Int).Sub(left, right)
	result.Mod(result, primitives.ScalarField())
	return result
}

func VerifyPoseidonEncryptionSpendNative(
	noteAmount *big.Int,
	noteAssetID *big.Int,
	selfDiversifiedGenerator gnarkte.Point,
	selfTransmissionKey gnarkte.Point,
	ssDetection gnarkte.Point,
	ssCore gnarkte.Point,
	epk gnarkte.Point,
	c2Core *big.Int,
	isFlagged bool,
	salt *big.Int,
) ([]*big.Int, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	ssDetectionFq, err := primitives.Decaf377CompressToFieldNative(ssDetection)
	if err != nil {
		return nil, err
	}
	epkFq, err := primitives.Decaf377CompressToFieldNative(epk)
	if err != nil {
		return nil, err
	}
	seedDetection, err := primitives.Poseidon377Hash2Native(
		primitives.MustBigInt(vectors.Poseidon377.IssuerDetectionDomain),
		[2]*big.Int{ssDetectionFq, epkFq},
	)
	if err != nil {
		return nil, err
	}

	detectionPlaintext := new(big.Int).Set(noteAssetID)
	if isFlagged {
		detectionPlaintext.Add(detectionPlaintext, flagBitFq())
		detectionPlaintext.Mod(detectionPlaintext, primitives.ScalarField())
	}

	keystream0, err := primitives.Poseidon377Hash2Native(seedDetection, [2]*big.Int{big.NewInt(0), seedDetection})
	if err != nil {
		return nil, err
	}
	keystream1, err := primitives.Poseidon377Hash2Native(seedDetection, [2]*big.Int{big.NewInt(1), seedDetection})
	if err != nil {
		return nil, err
	}

	ssCoreFq, err := primitives.Decaf377CompressToFieldNative(ssCore)
	if err != nil {
		return nil, err
	}
	seedCore := modSub(c2Core, ssCoreFq)
	corePlaintexts, err := SpendCorePlaintextFQsNative(noteAmount, selfDiversifiedGenerator, selfTransmissionKey)
	if err != nil {
		return nil, err
	}

	out := make([]*big.Int, 0, 5)
	detection0 := new(big.Int).Add(detectionPlaintext, keystream0)
	detection0.Mod(detection0, primitives.ScalarField())
	out = append(out, detection0)
	detection1 := new(big.Int).Add(new(big.Int).Set(salt), keystream1)
	detection1.Mod(detection1, primitives.ScalarField())
	out = append(out, detection1)
	for i, plain := range corePlaintexts {
		keystream, err := primitives.Poseidon377Hash2Native(seedCore, [2]*big.Int{big.NewInt(int64(i)), seedCore})
		if err != nil {
			return nil, err
		}
		cipher := new(big.Int).Add(new(big.Int).Set(plain), keystream)
		cipher.Mod(cipher, primitives.ScalarField())
		out = append(out, cipher)
	}
	return out, nil
}

func VerifyPoseidonEncryptionSpend(
	api frontend.API,
	isRegulated frontend.Variable,
	isFlagged frontend.Variable,
	ssDetection gnarkte.Point,
	ssCore gnarkte.Point,
	c2Core frontend.Variable,
	epkFq frontend.Variable,
	salt frontend.Variable,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	selfDiversifiedGeneratorFq frontend.Variable,
	selfTransmissionKeyFq frontend.Variable,
	complianceCiphertext [5]frontend.Variable,
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
		[2]frontend.Variable{ssDetectionFq, epkFq},
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
	return nil
}
