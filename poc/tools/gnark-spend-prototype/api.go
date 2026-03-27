package prototype

import "math/big"

import "github.com/consensys/gnark-crypto/ecc"

func ScalarField() *big.Int {
	return ecc.BLS12_377.ScalarField()
}

func LoadSpendFixtureForCLI() (spendFixture, error) {
	return loadSpendFixture()
}

func LoadSpendWitnessV1ForCLI() []byte {
	return loadSpendWitnessV1()
}

type SpendWitnessSummary struct {
	ClaimedStatementHash string
	StatementFields      []string
}

func DecodeSpendWitnessSummaryV1(payload []byte) (SpendWitnessSummary, error) {
	witness, err := decodeSpendWitnessV1(payload)
	if err != nil {
		return SpendWitnessSummary{}, err
	}
	summary := SpendWitnessSummary{
		ClaimedStatementHash: littleEndianBytesToBigInt(witness.ClaimedStatementHash[:]).String(),
		StatementFields:      make([]string, len(witness.StatementFields)),
	}
	for i := range witness.StatementFields {
		summary.StatementFields[i] = littleEndianBytesToBigInt(witness.StatementFields[i][:]).String()
	}
	return summary, nil
}
