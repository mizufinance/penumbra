package circuits

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark/constraint"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
)

func normalizedExpression(constant int64, coefficients map[int]int64) normalizedLinearExpression {
	result := normalizedLinearExpression{
		constant:     big.NewInt(constant),
		coefficients: make(map[int]*big.Int, len(coefficients)),
	}
	for wireID, coefficient := range coefficients {
		result.coefficients[wireID] = big.NewInt(coefficient)
	}
	return result
}

func TestR1CPolynomialInfluence(t *testing.T) {
	modulus := big.NewInt(101)
	for _, testCase := range []struct {
		name        string
		left        normalizedLinearExpression
		right       normalizedLinearExpression
		output      normalizedLinearExpression
		wantDepends bool
	}{
		{
			name:   "canceled linear appearance",
			left:   normalizedExpression(0, map[int]int64{1: 1}),
			right:  normalizedExpression(1, nil),
			output: normalizedExpression(0, map[int]int64{1: 1}),
		},
		{
			name:        "nonlinear term",
			left:        normalizedExpression(0, map[int]int64{1: 1}),
			right:       normalizedExpression(0, map[int]int64{1: 1}),
			output:      normalizedExpression(0, nil),
			wantDepends: true,
		},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			got := r1cPolynomialDependsOn(
				testCase.left,
				testCase.right,
				testCase.output,
				1,
				modulus,
			)
			if got != testCase.wantDepends {
				t.Fatalf("dependency = %v, want %v", got, testCase.wantDepends)
			}
		})
	}
}

func TestCircuitExportManifestsCoverEveryConstraint(t *testing.T) {
	type compileExport func() (constraint.ConstraintSystem, *ConstraintManifest, error)
	cases := map[string]compileExport{
		"transfer": CompileTransferForExport,
		"shielded_withdrawal": func() (constraint.ConstraintSystem, *ConstraintManifest, error) {
			return CompileShieldedWithdrawalForExport("shielded_withdrawal", 2)
		},
	}
	for _, family := range generated.NoteReshapeFamilies {
		family := family
		cases[family.Label] = func() (constraint.ConstraintSystem, *ConstraintManifest, error) {
			return CompileNoteReshapeForExport(family.Label, family.NIn, family.NOut)
		}
	}

	for label, compile := range cases {
		t.Run(label, func(t *testing.T) {
			ccs, manifest, err := compile()
			if err != nil {
				t.Fatal(err)
			}
			assertManifestCoverage(t, ccs, manifest)
		})
	}
}

func assertManifestCoverage(
	t *testing.T,
	ccs constraint.ConstraintSystem,
	manifest *ConstraintManifest,
) {
	t.Helper()
	if manifest.NbConstraints != ccs.GetNbConstraints() ||
		manifest.Breakdown.TotalConstraints != ccs.GetNbConstraints() {
		t.Fatal("manifest constraint count does not match compiled circuit")
	}
	if manifest.Breakdown.Segments != len(manifest.Segments) || len(manifest.Segments) == 0 {
		t.Fatal("manifest segment count is inconsistent")
	}
	nextRow := 0
	for index, segment := range manifest.Segments {
		if segment.Index != index+1 || segment.Start != nextRow ||
			segment.End-segment.Start != segment.ConstraintCount {
			t.Fatalf("noncanonical segment %d: %+v", index+1, segment)
		}
		nextRow = segment.End
	}
	if nextRow != ccs.GetNbConstraints() {
		t.Fatalf("segments cover %d of %d rows", nextRow, ccs.GetNbConstraints())
	}
	if len(manifest.WitnessWires) != manifest.NbPublic-1+manifest.NbSecret {
		t.Fatal("manifest witness count does not match compiled circuit")
	}
	for index, wire := range manifest.WitnessWires {
		if wire.WireID != index+1 || wire.Path == "" || wire.ConstraintRows == 0 {
			t.Fatalf("unbound or noncanonical witness wire: %+v", wire)
		}
	}
	if len(manifest.SemanticBindings) == 0 {
		t.Fatal("manifest has no semantic bindings")
	}
}

func TestWiringTranscriptIsDeterministic(t *testing.T) {
	first, err := ExportNoteReshape8x1WiringTranscript()
	if err != nil {
		t.Fatal(err)
	}
	second, err := ExportNoteReshape8x1WiringTranscript()
	if err != nil {
		t.Fatal(err)
	}
	if first != second {
		t.Fatal("wiring transcript is not deterministic")
	}
}
