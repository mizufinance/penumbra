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
const TransferStatementBaseFields = 77
const TransferStatementFieldsPerInput = 2
const TransferStatementFieldsPerOutput = 1
const NoteReshapeStatementBaseFields = 2
const NoteReshapeStatementFieldsPerInput = 2
const NoteReshapeStatementFieldsPerOutput = 1
const ShieldedIcs20WithdrawalStatementBaseFields = 10
const ShieldedIcs20WithdrawalStatementFieldsPerInput = 2

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

func NoteReshapeStatementFieldCount(nIn, nOut int) int {
	return NoteReshapeStatementBaseFields +
		NoteReshapeStatementFieldsPerInput*nIn +
		NoteReshapeStatementFieldsPerOutput*nOut
}

func transferStatementLabel() string { return "transfer" }

func consolidateStatementLabel(nIn int) string {
	return fmt.Sprintf("consolidate%dx1", nIn)
}

func splitStatementLabel(nOut int) string {
	return fmt.Sprintf("split1x%d", nOut)
}

func shieldedIcs20WithdrawalStatementLabel(nIn int) string {
	_ = nIn
	return "shielded_ics20_withdrawal"
}

func TransferStatementHashForShape(
	api frontend.API,
	nIn, nOut int,
	fields []frontend.Variable,
) (frontend.Variable, error) {
	return transferStatementHash(
		api,
		transferStatementLabel(),
		fields,
		transferStatementFieldCount(nIn, nOut),
	)
}

func ConsolidateStatementHashForShape(
	api frontend.API,
	nIn int,
	fields []frontend.Variable,
) (frontend.Variable, error) {
	return transferStatementHash(
		api,
		consolidateStatementLabel(nIn),
		fields,
		NoteReshapeStatementFieldCount(nIn, 1),
	)
}

func SplitStatementHashForShape(
	api frontend.API,
	nOut int,
	fields []frontend.Variable,
) (frontend.Variable, error) {
	return transferStatementHash(
		api,
		splitStatementLabel(nOut),
		fields,
		NoteReshapeStatementFieldCount(1, nOut),
	)
}

func ShieldedIcs20WithdrawalStatementFieldCount(nIn int) int {
	return ShieldedIcs20WithdrawalStatementBaseFields +
		ShieldedIcs20WithdrawalStatementFieldsPerInput*nIn
}

func ShieldedIcs20WithdrawalStatementHashForShape(
	api frontend.API,
	nIn int,
	fields []frontend.Variable,
) (frontend.Variable, error) {
	return transferStatementHash(
		api,
		shieldedIcs20WithdrawalStatementLabel(nIn),
		fields,
		ShieldedIcs20WithdrawalStatementFieldCount(nIn),
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
		transferStatementLabel(),
		transferStatementFieldCount(nIn, nOut),
	)
}

func ShieldedIcs20WithdrawalStatementHashNativeForShape(
	fields []*big.Int,
	nIn int,
) (*big.Int, error) {
	return transferStatementHashNative(
		fields,
		shieldedIcs20WithdrawalStatementLabel(nIn),
		ShieldedIcs20WithdrawalStatementFieldCount(nIn),
	)
}
