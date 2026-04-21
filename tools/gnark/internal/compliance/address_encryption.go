package compliance

import "github.com/consensys/gnark/frontend"

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
