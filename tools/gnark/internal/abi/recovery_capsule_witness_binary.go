package abi

import (
	"bytes"

	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
)

type RecoveryCapsuleWitnessBinary struct {
	EPKAffine             PointAffineBinary
	C2                    [32]byte
	Salt                  [32]byte
	KeyConfirmation       [32]byte
	EncryptedAmount       [32]byte
	EncryptedNoteBlinding [32]byte
	Seed                  [32]byte
	Randomizer            [32]byte
}

func readRecoveryCapsule(reader *bytes.Reader) (RecoveryCapsuleWitnessBinary, error) {
	var capsule RecoveryCapsuleWitnessBinary
	var err error
	if capsule.EPKAffine, err = readPointAffine(reader); err != nil {
		return capsule, err
	}
	for _, target := range []*[32]byte{
		&capsule.C2,
		&capsule.Salt,
		&capsule.KeyConfirmation,
		&capsule.EncryptedAmount,
		&capsule.EncryptedNoteBlinding,
		&capsule.Seed,
	} {
		if *target, err = read32(reader); err != nil {
			return capsule, err
		}
	}
	if capsule.Randomizer, err = readFr32(reader); err != nil {
		return capsule, err
	}
	return capsule, nil
}

func recoveryCapsuleFields(
	commitment [32]byte,
	capsule RecoveryCapsuleWitnessBinary,
) compliance.RecoveryCapsuleFields {
	return compliance.RecoveryCapsuleFields{
		Commitment: fqString(commitment),
		EPK: gnarkte.Point{
			X: fqString(capsule.EPKAffine.X),
			Y: fqString(capsule.EPKAffine.Y),
		},
		C2:                    fqString(capsule.C2),
		Salt:                  fqString(capsule.Salt),
		KeyConfirmation:       fqString(capsule.KeyConfirmation),
		EncryptedAmount:       fqString(capsule.EncryptedAmount),
		EncryptedNoteBlinding: fqString(capsule.EncryptedNoteBlinding),
		Seed:                  fqString(capsule.Seed),
		Randomizer:            fqString(capsule.Randomizer),
	}
}
