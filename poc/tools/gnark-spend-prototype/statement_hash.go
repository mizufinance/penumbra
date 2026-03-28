package prototype

import (
	"errors"
	"math/big"

	"github.com/consensys/gnark/frontend"
)

const spendStatementFieldCount = 17

func hashStatementFields(
	api frontend.API,
	domainSeparator frontend.Variable,
	pad0 frontend.Variable,
	pad1 frontend.Variable,
	fields []frontend.Variable,
) (frontend.Variable, error) {
	if len(fields) != spendStatementFieldCount {
		return nil, errors.New("invalid spend statement field count")
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
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	return hashStatementFields(
		api,
		mustBigInt(vectors.Poseidon377.SpendDomain),
		mustBigInt(vectors.Poseidon377.SpendPad0),
		mustBigInt(vectors.Poseidon377.SpendPad1),
		fields,
	)
}

func SpendStatementHashNative(fields []*big.Int) (*big.Int, error) {
	if len(fields) != spendStatementFieldCount {
		return nil, errors.New("invalid spend statement field count")
	}

	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	domain := mustBigInt(vectors.Poseidon377.SpendDomain)
	pad0 := mustBigInt(vectors.Poseidon377.SpendPad0)
	pad1 := mustBigInt(vectors.Poseidon377.SpendPad1)
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
