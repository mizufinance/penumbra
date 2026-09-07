package compliance

import (
	decafgnark "github.com/mizufinance/decaf377-go/gnark"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

// ClassifyTransferCore recovers a candidate seed from an authorized shared
// point. It returns a non-zero amount only when the public key confirmation
// matches; a wrong candidate is constrained to the exact non-match branch.
func ClassifyTransferCore(
	api frontend.API,
	sharedSecret gnarkte.Point,
	epkFq frontend.Variable,
	c2 frontend.Variable,
	keyConfirmation frontend.Variable,
	tierSalt frontend.Variable,
	ciphertext frontend.Variable,
) (frontend.Variable, frontend.Variable, error) {
	sharedSecretFq, err := decafgnark.CompressToField(api, sharedSecret)
	if err != nil {
		return nil, nil, err
	}
	seed := api.Sub(c2, sharedSecretFq)
	confirmation, err := primitives.Poseidon377Hash3(
		api,
		TransferKeyConfirmationDomain,
		[3]frontend.Variable{seed, epkFq, tierSalt},
	)
	if err != nil {
		return nil, nil, err
	}
	match := api.IsZero(api.Sub(confirmation, keyConfirmation))
	api.AssertIsBoolean(match)

	keystream, err := complianceStreamBlock(api, seed, 0)
	if err != nil {
		return nil, nil, err
	}
	plaintext := api.Sub(ciphertext, keystream)
	matchedAmount := api.Mul(match, plaintext)
	api.ToBinary(matchedAmount, 128)
	return match, matchedAmount, nil
}
