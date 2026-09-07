package compliance

import "github.com/consensys/gnark/frontend"

const AmountBits = 128

func FieldLessThan(api frontend.API, a, b frontend.Variable) frontend.Variable {
	aBits := api.ToBinary(a, AmountBits)
	bBits := api.ToBinary(b, AmountBits)

	prefixEqual := frontend.Variable(1)
	isLess := frontend.Variable(0)
	for i := AmountBits - 1; i >= 0; i-- {
		ai := aBits[i]
		bi := bBits[i]
		lessAtI := api.Mul(prefixEqual, api.Sub(1, ai), bi)
		isLess = api.Sub(api.Add(isLess, lessAtI), api.Mul(isLess, lessAtI))
		eqBit := api.Add(1, api.Mul(2, ai, bi), api.Mul(-1, ai), api.Mul(-1, bi))
		prefixEqual = api.Mul(prefixEqual, eqBit)
	}
	return isLess
}
