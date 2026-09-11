package compliance

import (
	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

var TransferMasterWrappingDomain = transferSaltConstant("shieldd.transfer.master_wrapping.v1")

// Bits and seed come from the existing EPK and payload constraints.
func VerifyTransferMasterWrapping(api frontend.API, bits []frontend.Variable,
	ring, issuerShared gnarkte.Point, flagged, seed, epk frontend.Variable,
	position int, wrapping frontend.Variable) error {
	api.AssertIsBoolean(flagged)
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return err
	}
	masterShared := ScalarMulWindow2LEBits(api, curve, ring, bits)
	selected := SelectPoint(api, flagged, issuerShared, masterShared)
	encoded, err := decafgnark.CompressToField(api, selected)
	if err != nil {
		return err
	}
	mask, err := primitives.Poseidon377Hash3(api, TransferMasterWrappingDomain,
		[3]frontend.Variable{position, encoded, epk})
	if err != nil {
		return err
	}
	api.AssertIsEqual(wrapping, api.Add(seed, mask))
	return nil
}
