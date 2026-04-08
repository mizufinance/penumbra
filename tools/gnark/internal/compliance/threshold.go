package compliance

import (
	"math/big"

	"github.com/consensys/gnark/frontend"
)

const ThresholdAmountBits = 128

func flagBitFq() *big.Int {
	return new(big.Int).Lsh(big.NewInt(1), 253)
}

func fieldLessThan(api frontend.API, a, b frontend.Variable) frontend.Variable {
	aBits := api.ToBinary(a, ThresholdAmountBits)
	bBits := api.ToBinary(b, ThresholdAmountBits)

	prefixEqual := frontend.Variable(1)
	isLess := frontend.Variable(0)
	for i := ThresholdAmountBits - 1; i >= 0; i-- {
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
