package primitives

import (
	"errors"
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
	"golang.org/x/crypto/blake2b"
)

const SpendStatementFieldCount = 17
const OutputStatementFieldCount = 29
const TransferStatementBaseFields = 5
const TransferStatementFieldsPerInput = 11
const TransferStatementFieldsPerOutput = 24

func hashStatementFields(
	api frontend.API,
	domainSeparator frontend.Variable,
	pad0 frontend.Variable,
	pad1 frontend.Variable,
	fields []frontend.Variable,
	expectedFieldCount int,
) (frontend.Variable, error) {
	if len(fields) != expectedFieldCount {
		return nil, errors.New("invalid statement field count")
	}

	first := [7]frontend.Variable{pad0, pad1, pad0, pad1, pad0, pad1, pad0}
	for i := 0; i < len(first) && i < len(fields); i++ {
		first[i] = fields[i]
	}

	h, err := Poseidon377Hash7(api, domainSeparator, first)
	if err != nil {
		return nil, err
	}
	idx := len(first)

	for idx+6 <= len(fields) {
		h, err = Poseidon377Hash7(api, domainSeparator, [7]frontend.Variable{
			h,
			fields[idx],
			fields[idx+1],
			fields[idx+2],
			fields[idx+3],
			fields[idx+4],
			fields[idx+5],
		})
		if err != nil {
			return nil, err
		}
		idx += 6
	}

	if idx < len(fields) {
		tail := [6]frontend.Variable{pad0, pad1, pad0, pad1, pad0, pad1}
		for i, value := range fields[idx:] {
			tail[i] = value
		}
		return Poseidon377Hash7(api, domainSeparator, [7]frontend.Variable{
			h,
			tail[0],
			tail[1],
			tail[2],
			tail[3],
			tail[4],
			tail[5],
		})
	}

	return h, nil
}

func SpendStatementHash(api frontend.API, fields []frontend.Variable) (frontend.Variable, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return hashStatementFields(
		api,
		MustBigInt(vectors.Poseidon377.SpendDomain),
		MustBigInt(vectors.Poseidon377.SpendPad0),
		MustBigInt(vectors.Poseidon377.SpendPad1),
		fields,
		SpendStatementFieldCount,
	)
}

func outputStatementHashConstant(label string) *big.Int {
	sum := blake2b.Sum512([]byte(label))
	return LittleEndianBytesToBigInt(sum[:])
}

func transferStatementHashConstant(label, suffix string) *big.Int {
	return outputStatementHashConstant("penumbra.shielded_pool." + label + ".public_input_hash." + suffix)
}

func transferStatementHash(
	api frontend.API,
	label string,
	fields []frontend.Variable,
	expectedFieldCount int,
) (frontend.Variable, error) {
	return hashStatementFields(
		api,
		transferStatementHashConstant(label, "v1"),
		transferStatementHashConstant(label, "pad0"),
		transferStatementHashConstant(label, "pad1"),
		fields,
		expectedFieldCount,
	)
}

func transferStatementFieldCount(nIn, nOut int) int {
	return TransferStatementBaseFields +
		TransferStatementFieldsPerInput*nIn +
		TransferStatementFieldsPerOutput*nOut
}

func transferStatementLabel(nIn, nOut int) string {
	return fmt.Sprintf("transfer%dx%d", nIn, nOut)
}

func TransferStatementHashForShape(
	api frontend.API,
	nIn, nOut int,
	fields []frontend.Variable,
) (frontend.Variable, error) {
	return transferStatementHash(
		api,
		transferStatementLabel(nIn, nOut),
		fields,
		transferStatementFieldCount(nIn, nOut),
	)
}

func OutputStatementHash(api frontend.API, fields []frontend.Variable) (frontend.Variable, error) {
	return hashStatementFields(
		api,
		outputStatementHashConstant("penumbra.shielded_pool.output.public_input_hash.v1"),
		outputStatementHashConstant("penumbra.shielded_pool.output.public_input_hash.pad0"),
		outputStatementHashConstant("penumbra.shielded_pool.output.public_input_hash.pad1"),
		fields,
		OutputStatementFieldCount,
	)
}

func SpendStatementHashNative(fields []*big.Int) (*big.Int, error) {
	if len(fields) != SpendStatementFieldCount {
		return nil, errors.New("invalid spend statement field count")
	}

	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	domain := MustBigInt(vectors.Poseidon377.SpendDomain)
	pad0 := MustBigInt(vectors.Poseidon377.SpendPad0)
	pad1 := MustBigInt(vectors.Poseidon377.SpendPad1)
	first := [7]*big.Int{pad0, pad1, pad0, pad1, pad0, pad1, pad0}
	for i := 0; i < len(first) && i < len(fields); i++ {
		first[i] = fields[i]
	}

	h, err := Poseidon377Hash7Native(domain, first)
	if err != nil {
		return nil, err
	}
	idx := len(first)

	for idx+6 <= len(fields) {
		h, err = Poseidon377Hash7Native(domain, [7]*big.Int{
			h,
			fields[idx],
			fields[idx+1],
			fields[idx+2],
			fields[idx+3],
			fields[idx+4],
			fields[idx+5],
		})
		if err != nil {
			return nil, err
		}
		idx += 6
	}

	if idx < len(fields) {
		tail := [6]*big.Int{pad0, pad1, pad0, pad1, pad0, pad1}
		for i, value := range fields[idx:] {
			tail[i] = value
		}
		return Poseidon377Hash7Native(domain, [7]*big.Int{
			h,
			tail[0],
			tail[1],
			tail[2],
			tail[3],
			tail[4],
			tail[5],
		})
	}

	return h, nil
}

func OutputStatementHashNative(fields []*big.Int) (*big.Int, error) {
	if len(fields) != OutputStatementFieldCount {
		return nil, errors.New("invalid output statement field count")
	}

	domain := outputStatementHashConstant("penumbra.shielded_pool.output.public_input_hash.v1")
	pad0 := outputStatementHashConstant("penumbra.shielded_pool.output.public_input_hash.pad0")
	pad1 := outputStatementHashConstant("penumbra.shielded_pool.output.public_input_hash.pad1")
	first := [7]*big.Int{pad0, pad1, pad0, pad1, pad0, pad1, pad0}
	for i := 0; i < len(first) && i < len(fields); i++ {
		first[i] = fields[i]
	}

	h, err := Poseidon377Hash7Native(domain, first)
	if err != nil {
		return nil, err
	}
	idx := len(first)

	for idx+6 <= len(fields) {
		h, err = Poseidon377Hash7Native(domain, [7]*big.Int{
			h,
			fields[idx],
			fields[idx+1],
			fields[idx+2],
			fields[idx+3],
			fields[idx+4],
			fields[idx+5],
		})
		if err != nil {
			return nil, err
		}
		idx += 6
	}

	if idx < len(fields) {
		tail := [6]*big.Int{pad0, pad1, pad0, pad1, pad0, pad1}
		for i, value := range fields[idx:] {
			tail[i] = value
		}
		return Poseidon377Hash7Native(domain, [7]*big.Int{
			h,
			tail[0],
			tail[1],
			tail[2],
			tail[3],
			tail[4],
			tail[5],
		})
	}

	return h, nil
}

func transferStatementHashNative(
	fields []*big.Int,
	label string,
	expectedFieldCount int,
) (*big.Int, error) {
	if len(fields) != expectedFieldCount {
		return nil, errors.New("invalid " + label + " statement field count")
	}

	domain := transferStatementHashConstant(label, "v1")
	pad0 := transferStatementHashConstant(label, "pad0")
	pad1 := transferStatementHashConstant(label, "pad1")
	first := [7]*big.Int{pad0, pad1, pad0, pad1, pad0, pad1, pad0}
	for i := 0; i < len(first) && i < len(fields); i++ {
		first[i] = fields[i]
	}

	h, err := Poseidon377Hash7Native(domain, first)
	if err != nil {
		return nil, err
	}
	idx := len(first)

	for idx+6 <= len(fields) {
		h, err = Poseidon377Hash7Native(domain, [7]*big.Int{
			h,
			fields[idx],
			fields[idx+1],
			fields[idx+2],
			fields[idx+3],
			fields[idx+4],
			fields[idx+5],
		})
		if err != nil {
			return nil, err
		}
		idx += 6
	}

	if idx < len(fields) {
		tail := [6]*big.Int{pad0, pad1, pad0, pad1, pad0, pad1}
		for i, value := range fields[idx:] {
			tail[i] = value
		}
		return Poseidon377Hash7Native(domain, [7]*big.Int{
			h,
			tail[0],
			tail[1],
			tail[2],
			tail[3],
			tail[4],
			tail[5],
		})
	}

	return h, nil
}

func TransferStatementHashNativeForShape(
	fields []*big.Int,
	nIn, nOut int,
) (*big.Int, error) {
	return transferStatementHashNative(
		fields,
		transferStatementLabel(nIn, nOut),
		transferStatementFieldCount(nIn, nOut),
	)
}
