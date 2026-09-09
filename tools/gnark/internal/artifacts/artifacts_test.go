package artifacts

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"

	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

func testCircuitMetadata() CircuitMetadataJSON {
	return CircuitMetadataJSON{
		Schema:                      CircuitMetadataSchema,
		Curve:                       "bls12-377",
		Circuit:                     "output",
		ProvingKeySize:              3,
		VerifyingKeySize:            4,
		NbConstraints:               1,
		NbPublic:                    1,
		NbSecret:                    1,
		SR1CSSHA256Hex:              strings.Repeat("0", 64),
		SetupProvenanceSHA256Hex:    strings.Repeat("4", 64),
		ProvingKeySHA256Hex:         strings.Repeat("1", 64),
		VerifyingKeyBinarySHA256Hex: strings.Repeat("2", 64),
		VerifyingKeyJSONSHA256Hex:   strings.Repeat("3", 64),
	}
}

func TestLoadCircuitMetadataBytesRoundTrip(t *testing.T) {
	encoded, err := EncodeCanonicalJSON(testCircuitMetadata())
	if err != nil {
		t.Fatalf("EncodeCanonicalJSON: %v", err)
	}
	metadata, err := LoadCircuitMetadataBytes(encoded, "test metadata")
	if err != nil {
		t.Fatalf("LoadCircuitMetadataBytes: %v", err)
	}
	if *metadata != testCircuitMetadata() {
		t.Fatalf("metadata roundtrip mismatch: got %+v want %+v", *metadata, testCircuitMetadata())
	}
}

func TestLoadCircuitMetadataBindsSetupProvenanceBytes(t *testing.T) {
	dir := t.TempDir()
	provenancePath := filepath.Join(dir, "setup_provenance.json")
	if err := os.WriteFile(provenancePath, []byte("{}\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	metadata := testCircuitMetadata()
	var err error
	metadata.SetupProvenanceSHA256Hex, err = SHA256HexFile(provenancePath)
	if err != nil {
		t.Fatal(err)
	}
	if err := WriteJSON(filepath.Join(dir, "circuit_metadata.json"), metadata); err != nil {
		t.Fatal(err)
	}
	if _, err := LoadCircuitMetadata(dir); err != nil {
		t.Fatalf("bound setup provenance rejected: %v", err)
	}
	if err := os.WriteFile(provenancePath, []byte("{\"changed\":true}\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := LoadCircuitMetadata(dir); err == nil {
		t.Fatal("stale setup provenance must be rejected")
	}
}

func TestValidateCircuitMetadataForCircuitRejectsNilCCS(t *testing.T) {
	metadata := testCircuitMetadata()
	if err := ValidateCircuitMetadataForCircuit(&metadata, "output", nil); err == nil {
		t.Fatalf("expected nil ccs to fail validation")
	}
}

type squareEqualsSelfCircuit struct {
	X frontend.Variable
}

func (c *squareEqualsSelfCircuit) Define(api frontend.API) error {
	api.AssertIsEqual(api.Mul(c.X, c.X), c.X)
	return nil
}

type squareEqualsZeroCircuit struct {
	X frontend.Variable
}

func (c *squareEqualsZeroCircuit) Define(api frontend.API) error {
	api.AssertIsEqual(api.Mul(c.X, c.X), 0)
	return nil
}

func compileMetadataTestCircuit(
	t *testing.T,
	circuit frontend.Circuit,
) constraint.ConstraintSystem {
	t.Helper()
	ccs, err := frontend.Compile(
		primitives.ScalarField(),
		r1cs.NewBuilder,
		circuit,
	)
	if err != nil {
		t.Fatalf("compile metadata test circuit: %v", err)
	}
	return ccs
}

func TestConstraintSystemHashRejectsSameShapeWrongRelation(t *testing.T) {
	expected := compileMetadataTestCircuit(t, &squareEqualsSelfCircuit{})
	wrong := compileMetadataTestCircuit(t, &squareEqualsZeroCircuit{})
	if expected.GetNbConstraints() != wrong.GetNbConstraints() ||
		expected.GetNbPublicVariables() != wrong.GetNbPublicVariables() ||
		expected.GetNbSecretVariables() != wrong.GetNbSecretVariables() {
		t.Fatal("test circuits must have the same metadata counts")
	}

	metadata := testCircuitMetadata()
	metadata.NbConstraints = expected.GetNbConstraints()
	metadata.NbPublic = expected.GetNbPublicVariables()
	metadata.NbSecret = expected.GetNbSecretVariables()
	var err error
	metadata.SR1CSSHA256Hex, err = ConstraintSystemSHA256Hex(expected)
	if err != nil {
		t.Fatalf("hash expected constraint system: %v", err)
	}
	if err := ValidateCircuitMetadataForCircuit(
		&metadata,
		"output",
		expected,
	); err != nil {
		t.Fatalf("validate exact constraint system: %v", err)
	}
	if err := ValidateCircuitMetadataForCircuit(
		&metadata,
		"output",
		wrong,
	); err == nil {
		t.Fatal("same-shape wrong constraint system must fail metadata validation")
	}

	path := filepath.Join(t.TempDir(), "expected.sr1cs")
	if err := WriteConstraintSystem(path, expected); err != nil {
		t.Fatalf("write expected constraint system: %v", err)
	}
	got, err := SHA256HexFile(path)
	if err != nil {
		t.Fatalf("hash written constraint system: %v", err)
	}
	if got != metadata.SR1CSSHA256Hex {
		t.Fatalf("streamed hash %s != written artifact hash %s", metadata.SR1CSSHA256Hex, got)
	}
}
