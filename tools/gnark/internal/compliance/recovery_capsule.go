package compliance

import (
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"golang.org/x/crypto/blake2b"
)

type RecoveryCapsuleFields struct {
	Commitment            frontend.Variable
	EPK                   gnarkte.Point
	C2                    frontend.Variable
	Salt                  frontend.Variable
	KeyConfirmation       frontend.Variable
	EncryptedAmount       frontend.Variable
	EncryptedNoteBlinding frontend.Variable
	Seed                  frontend.Variable
	Randomizer            frontend.Variable
}

func recoveryCapsuleDomain(label string) *big.Int {
	sum := blake2b.Sum512([]byte(label))
	return primitives.LittleEndianBytesToBigInt(sum[:])
}

var (
	recoveryCapsuleCommitmentDomain   = recoveryCapsuleDomain("shieldd.recovery_capsule.commitment")
	recoveryCapsuleConfirmationDomain = recoveryCapsuleDomain("shieldd.recovery_capsule.confirmation")
)

func VerifyRecoveryCapsule(
	api frontend.API,
	capk gnarkte.Point,
	amount frontend.Variable,
	noteBlinding frontend.Variable,
	capsule RecoveryCapsuleFields,
) error {
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return err
	}
	generator, err := decafGeneratorPoint()
	if err != nil {
		return err
	}
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return err
	}
	randomizerBits := api.ToBinary(
		capsule.Randomizer,
		primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	computedEPK := ScalarMulLEBits(api, curve, generator, randomizerBits)
	decafgnark.AssertEquivalent(api, computedEPK, capsule.EPK)
	api.AssertIsDifferent(capsule.EPK.X, 0)

	shared := ScalarMulWindow2LEBits(api, curve, capk, randomizerBits)
	sharedFq, err := decafgnark.CompressToField(api, shared)
	if err != nil {
		return err
	}
	api.AssertIsEqual(capsule.C2, api.Add(capsule.Seed, sharedFq))

	return verifyRecoveryCapsulePlaintext(api, amount, noteBlinding, capsule)
}

// VerifyRecoveryCapsulePlaintext proves the public capsule commitment and
// plaintext after native release verification has derived the capsule seed.
func VerifyRecoveryCapsulePlaintext(
	api frontend.API,
	amount frontend.Variable,
	noteBlinding frontend.Variable,
	capsule RecoveryCapsuleFields,
) error {
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return err
	}
	curve.AssertIsOnCurve(capsule.EPK)
	api.AssertIsDifferent(capsule.EPK.X, 0)
	return verifyRecoveryCapsulePlaintext(api, amount, noteBlinding, capsule)
}

func verifyRecoveryCapsulePlaintext(
	api frontend.API,
	amount frontend.Variable,
	noteBlinding frontend.Variable,
	capsule RecoveryCapsuleFields,
) error {
	epkFq, err := decafgnark.CompressToField(api, capsule.EPK)
	if err != nil {
		return err
	}
	confirmation, err := primitives.Poseidon377Hash3(
		api,
		recoveryCapsuleConfirmationDomain,
		[3]frontend.Variable{capsule.Seed, epkFq, capsule.Salt},
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(confirmation, capsule.KeyConfirmation)

	amountStream, err := complianceStreamBlock(api, capsule.Seed, 0)
	if err != nil {
		return err
	}
	blindingStream, err := complianceStreamBlock(api, capsule.Seed, 1)
	if err != nil {
		return err
	}
	api.AssertIsEqual(capsule.EncryptedAmount, api.Add(amount, amountStream))
	api.AssertIsEqual(
		capsule.EncryptedNoteBlinding,
		api.Add(noteBlinding, blindingStream),
	)

	commitment, err := primitives.Poseidon377Hash6(
		api,
		recoveryCapsuleCommitmentDomain,
		[6]frontend.Variable{
			epkFq,
			capsule.C2,
			capsule.Salt,
			capsule.KeyConfirmation,
			capsule.EncryptedAmount,
			capsule.EncryptedNoteBlinding,
		},
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(commitment, capsule.Commitment)
	return nil
}
