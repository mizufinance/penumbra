package circuits_test

import (
	"math/big"
	"strings"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/test"
	"github.com/mizufinance/penumbra/tools/gnark/internal/abi"
	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

type circuitFamily struct {
	name             string
	circuit          func() frontend.Circuit
	assignment       func(t *testing.T) frontend.Circuit
	mutateStatement  func(frontend.Circuit)
	mutateCompliance func(frontend.Circuit)
}

func mutateFieldByOne(value frontend.Variable) frontend.Variable {
	switch v := value.(type) {
	case string:
		n, ok := new(big.Int).SetString(v, 10)
		if !ok {
			panic("invalid decimal frontend.Variable string")
		}
		return new(big.Int).Add(n, big.NewInt(1)).String()
	case *big.Int:
		return new(big.Int).Add(v, big.NewInt(1))
	case big.Int:
		return new(big.Int).Add(&v, big.NewInt(1))
	case int:
		return v + 1
	case int64:
		return v + 1
	case uint64:
		return v + 1
	default:
		panic("unsupported frontend.Variable type for mutation")
	}
}

func testCircuitFamilies() []circuitFamily {
	return []circuitFamily{
		{
			name:    "transfer",
			circuit: func() frontend.Circuit { return circuits.NewTransferCircuit() },
			assignment: func(t *testing.T) frontend.Circuit {
				t.Helper()
				fixtureBytes := primitives.LoadTransferWitnessV1("transfer")
				assignment, _, err := abi.NewTransferCircuitAssignmentFromWitnessV1(fixtureBytes)
				if err != nil {
					t.Fatalf("decode transfer witness fixture: %v", err)
				}
				return assignment
			},
			mutateStatement: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.TransferCircuit)
				a.ClaimedStatementHash = mutateFieldByOne(a.ClaimedStatementHash)
			},
			mutateCompliance: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.TransferCircuit)
				a.Compliance.SenderCore.Proof.Challenge = mutateFieldByOne(a.Compliance.SenderCore.Proof.Challenge)
			},
		},
	}
}

func TestCircuitFamiliesCompile(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, family.circuit())
			if err != nil {
				t.Fatalf("compile %s circuit: %v", family.name, err)
			}
		})
	}
}

func TestCircuitFamiliesAcceptValidAssignment(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithValidAssignment(family.assignment(t)),
			)
		})
	}
}

func TestCircuitFamiliesRejectWrongStatementHash(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			assignment := family.assignment(t)
			family.mutateStatement(assignment)

			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithInvalidAssignment(assignment),
			)
		})
	}
}

func TestCircuitFamiliesRejectMutatedComplianceField(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		t.Run(family.name, func(t *testing.T) {
			assignment := family.assignment(t)
			family.mutateCompliance(assignment)

			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithInvalidAssignment(assignment),
			)
		})
	}
}

func TestTransferFamiliesRejectWrongReceiverOrdering(t *testing.T) {
	for _, family := range testCircuitFamilies() {
		if !strings.HasPrefix(family.name, "transfer") {
			continue
		}
		t.Run(family.name, func(t *testing.T) {
			assignment := family.assignment(t).(*circuits.TransferCircuit)
			assignment.Outputs[0].IsReceiver = 0
			if len(assignment.Outputs) > 1 {
				assignment.Outputs[1].IsReceiver = 1
			}

			assert := test.NewAssert(t)
			assert.CheckCircuit(
				family.circuit(),
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithInvalidAssignment(assignment),
			)
		})
	}
}
