package circuits_test

import (
	"os"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/test"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/abi"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/circuits"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type circuitFamily struct {
	name             string
	circuit          func() frontend.Circuit
	assignment       func(t *testing.T) frontend.Circuit
	mutateStatement  func(frontend.Circuit)
	mutateCompliance func(frontend.Circuit)
}

func testCircuitFamilies() []circuitFamily {
	return []circuitFamily{
		{
			name:    "spend",
			circuit: func() frontend.Circuit { return &circuits.SpendCircuit{} },
			assignment: func(t *testing.T) frontend.Circuit {
				t.Helper()
				assignment, err := abi.NewSpendCircuitAssignmentFromWitnessV1(primitives.LoadSpendWitnessV1())
				if err != nil {
					t.Fatalf("build spend assignment from witness binary: %v", err)
				}
				return assignment
			},
			mutateStatement: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.SpendCircuit)
				a.ClaimedStatementHash = circuits.MutateFieldByOne(a.ClaimedStatementHash)
			},
			mutateCompliance: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.SpendCircuit)
				a.Dleq.C = circuits.MutateFieldByOne(a.Dleq.C)
			},
		},
		{
			name:    "output",
			circuit: func() frontend.Circuit { return &circuits.OutputCircuit{} },
			assignment: func(t *testing.T) frontend.Circuit {
				t.Helper()
				assignment, err := abi.NewOutputCircuitAssignmentFromWitnessV1(primitives.LoadOutputWitnessV1())
				if err != nil {
					t.Fatalf("build output assignment from witness binary: %v", err)
				}
				return assignment
			},
			mutateStatement: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.OutputCircuit)
				a.ClaimedStatementHash = circuits.MutateFieldByOne(a.ClaimedStatementHash)
			},
			mutateCompliance: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.OutputCircuit)
				a.Dleq1.C = circuits.MutateFieldByOne(a.Dleq1.C)
			},
		},
		{
			name:    "transfer1x1",
			circuit: func() frontend.Circuit { return circuits.NewTransferCircuit(1, 1) },
			assignment: func(t *testing.T) frontend.Circuit {
				t.Helper()
				fixtureBytes, err := os.ReadFile("../../testdata/transfer1x1_witness_v1.bin")
				if err != nil {
					if os.IsNotExist(err) {
						t.Skipf("transfer1x1 witness fixture not found: %v", err)
					}
					t.Fatalf("read transfer1x1 witness fixture: %v", err)
				}
				assignment, _, err := abi.NewTransferCircuitAssignmentFromWitnessV1(fixtureBytes)
				if err != nil {
					t.Fatalf("decode transfer1x1 witness fixture: %v", err)
				}
				return assignment
			},
			mutateStatement: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.TransferCircuit)
				a.ClaimedStatementHash = circuits.MutateFieldByOne(a.ClaimedStatementHash)
			},
			mutateCompliance: func(assignment frontend.Circuit) {
				a := assignment.(*circuits.TransferCircuit)
				a.Spends[0].Dleq.C = circuits.MutateFieldByOne(a.Spends[0].Dleq.C)
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
