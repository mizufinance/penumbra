package prototype

import (
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
)

func flagBitFq() *big.Int {
	return new(big.Int).Lsh(big.NewInt(1), 253)
}

func fieldLessThan(api frontend.API, a, b frontend.Variable) frontend.Variable {
	aBits := api.ToBinary(a, decaf377FieldBits)
	bBits := api.ToBinary(b, decaf377FieldBits)

	prefixEqual := frontend.Variable(1)
	isLess := frontend.Variable(0)
	for i := decaf377FieldBits - 1; i >= 0; i-- {
		ai := aBits[i]
		bi := bBits[i]
		lessAtI := api.Mul(prefixEqual, api.Sub(1, ai), bi)
		isLess = api.Sub(api.Add(isLess, lessAtI), api.Mul(isLess, lessAtI))
		eqBit := api.Add(1, api.Mul(2, ai, bi), api.Mul(-1, ai), api.Mul(-1, bi))
		prefixEqual = api.Mul(prefixEqual, eqBit)
	}
	return isLess
}

func VerifyThresholdFlagSimple(api frontend.API, amount, threshold, isFlagged frontend.Variable) {
	api.AssertIsBoolean(isFlagged)
	amountLTThreshold := fieldLessThan(api, amount, threshold)
	amountGTEThreshold := api.Sub(1, amountLTThreshold)
	api.AssertIsEqual(isFlagged, amountGTEThreshold)
}

func DeriveACKFromLeafDNative(ringPK gnarkte.Point, d *big.Int) (gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	return scalarMulNative(ringPK, d, mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen())
}

func DeriveACKFromLeafD(api frontend.API, ringPK gnarkte.Point, d frontend.Variable) (gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return scalarMulLE(api, curve, ringPK, d, mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()), nil
}

func DeriveSharedSecretsSpendNative(
	esk *big.Int,
	ackCore gnarkte.Point,
	dkPub gnarkte.Point,
	isFlagged bool,
) (gnarkte.Point, gnarkte.Point, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	nBits := mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()
	ssCoreUser, err := scalarMulNative(ackCore, esk, nBits)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	ssIssuer, err := scalarMulNative(dkPub, esk, nBits)
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
) (gnarkte.Point, gnarkte.Point, error) {
	api.AssertIsBoolean(isFlagged)
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	generator, err := decafGeneratorPoint()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	nBits := mustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()

	computedEPK := scalarMulLE(api, curve, generator, esk, nBits)
	AssertDecafEquivalent(api, computedEPK, publishedEPK)

	ssCoreUser := scalarMulLE(api, curve, ackCore, esk, nBits)
	ssIssuer := scalarMulLE(api, curve, dkPub, esk, nBits)
	ssCore := gnarkte.Point{
		X: api.Select(isFlagged, ssIssuer.X, ssCoreUser.X),
		Y: api.Select(isFlagged, ssIssuer.Y, ssCoreUser.Y),
	}
	return ssIssuer, ssCore, nil
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
		out[i] = littleEndianBytesToBigInt(chunk)
	}
	return out
}

func SpendCorePlaintextFQsNative(
	noteAmount *big.Int,
	selfDiversifiedGenerator gnarkte.Point,
	selfTransmissionKey gnarkte.Point,
) ([]*big.Int, error) {
	selfDiversifiedGeneratorFq, err := Decaf377CompressToFieldNative(selfDiversifiedGenerator)
	if err != nil {
		return nil, err
	}
	selfTransmissionKeyFq, err := Decaf377CompressToFieldNative(selfTransmissionKey)
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
	selfDiversifiedGeneratorFq, err := Decaf377CompressToField(api, selfDiversifiedGenerator)
	if err != nil {
		return nil, err
	}
	selfTransmissionKeyFq, err := Decaf377CompressToField(api, selfTransmissionKey)
	if err != nil {
		return nil, err
	}

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
	return out, nil
}

func modSub(left, right *big.Int) *big.Int {
	result := new(big.Int).Sub(left, right)
	result.Mod(result, ScalarField())
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
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	ssDetectionFq, err := Decaf377CompressToFieldNative(ssDetection)
	if err != nil {
		return nil, err
	}
	epkFq, err := Decaf377CompressToFieldNative(epk)
	if err != nil {
		return nil, err
	}
	seedDetection, err := Poseidon377Hash2Native(
		mustBigInt(vectors.Poseidon377.IssuerDetectionDomain),
		[2]*big.Int{ssDetectionFq, epkFq},
	)
	if err != nil {
		return nil, err
	}

	detectionPlaintext := new(big.Int).Set(noteAssetID)
	if isFlagged {
		detectionPlaintext.Add(detectionPlaintext, flagBitFq())
		detectionPlaintext.Mod(detectionPlaintext, ScalarField())
	}

	keystream0, err := Poseidon377Hash2Native(seedDetection, [2]*big.Int{big.NewInt(0), seedDetection})
	if err != nil {
		return nil, err
	}
	keystream1, err := Poseidon377Hash2Native(seedDetection, [2]*big.Int{big.NewInt(1), seedDetection})
	if err != nil {
		return nil, err
	}

	ssCoreFq, err := Decaf377CompressToFieldNative(ssCore)
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
	detection0.Mod(detection0, ScalarField())
	out = append(out, detection0)
	detection1 := new(big.Int).Add(new(big.Int).Set(salt), keystream1)
	detection1.Mod(detection1, ScalarField())
	out = append(out, detection1)
	for i, plain := range corePlaintexts {
		keystream, err := Poseidon377Hash2Native(seedCore, [2]*big.Int{big.NewInt(int64(i)), seedCore})
		if err != nil {
			return nil, err
		}
		cipher := new(big.Int).Add(new(big.Int).Set(plain), keystream)
		cipher.Mod(cipher, ScalarField())
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
	epk gnarkte.Point,
	salt frontend.Variable,
	noteAmount frontend.Variable,
	noteAssetID frontend.Variable,
	selfDiversifiedGenerator gnarkte.Point,
	selfTransmissionKey gnarkte.Point,
	complianceCiphertext [5]frontend.Variable,
) error {
	api.AssertIsBoolean(isRegulated)
	api.AssertIsBoolean(isFlagged)
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return err
	}

	ssDetectionFq, err := Decaf377CompressToField(api, ssDetection)
	if err != nil {
		return err
	}
	epkFq, err := Decaf377CompressToField(api, epk)
	if err != nil {
		return err
	}
	seedDetection, err := Poseidon377Hash2(
		api,
		mustBigInt(vectors.Poseidon377.IssuerDetectionDomain),
		[2]frontend.Variable{ssDetectionFq, epkFq},
	)
	if err != nil {
		return err
	}

	flagContribution := api.Mul(isFlagged, flagBitFq())
	detectionPlaintext := api.Add(noteAssetID, flagContribution)
	keystream0, err := Poseidon377Hash2(api, seedDetection, [2]frontend.Variable{0, seedDetection})
	if err != nil {
		return err
	}
	keystream1, err := Poseidon377Hash2(api, seedDetection, [2]frontend.Variable{1, seedDetection})
	if err != nil {
		return err
	}
	assertEqualIf(api, api.Add(detectionPlaintext, keystream0), complianceCiphertext[0], isRegulated)
	assertEqualIf(api, api.Add(salt, keystream1), complianceCiphertext[1], isRegulated)

	ssCoreFq, err := Decaf377CompressToField(api, ssCore)
	if err != nil {
		return err
	}
	seedCore := api.Sub(c2Core, ssCoreFq)
	corePlaintexts, err := SpendCorePlaintextFQs(api, noteAmount, selfDiversifiedGenerator, selfTransmissionKey)
	if err != nil {
		return err
	}
	for i, plain := range corePlaintexts {
		keystream, err := Poseidon377Hash2(api, seedCore, [2]frontend.Variable{i, seedCore})
		if err != nil {
			return err
		}
		assertEqualIf(api, api.Add(plain, keystream), complianceCiphertext[2+i], isRegulated)
	}
	return nil
}

func ComputeMetadataHash(api frontend.API, policyIDHash, resourceHash, permissionHash, tier, targetTimestamp, salt frontend.Variable) (frontend.Variable, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}
	return Poseidon377Hash6(
		api,
		mustBigInt(vectors.Poseidon377.DLEQMetadataDomain),
		[6]frontend.Variable{policyIDHash, resourceHash, permissionHash, tier, targetTimestamp, salt},
	)
}
