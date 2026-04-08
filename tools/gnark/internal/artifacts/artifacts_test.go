package artifacts

import "testing"

func TestLoadCircuitMetadataBytesRoundTrip(t *testing.T) {
	metadata, err := LoadCircuitMetadataBytes([]byte(`{
		"curve":"bls12-377",
		"circuit":"output",
		"nb_constraints":1,
		"nb_public_variables":1,
		"nb_secret_variables":1
	}`), "test metadata")
	if err != nil {
		t.Fatalf("LoadCircuitMetadataBytes: %v", err)
	}
	if metadata.Circuit != "output" {
		t.Fatalf("unexpected circuit %q", metadata.Circuit)
	}
}

func TestValidateCircuitMetadataForCircuitRejectsNilCCS(t *testing.T) {
	metadata := &CircuitMetadataJSON{
		Curve:         "bls12-377",
		Circuit:       "output",
		NbConstraints: 1,
		NbPublic:      1,
		NbSecret:      1,
	}
	if err := ValidateCircuitMetadataForCircuit(metadata, "output", nil); err == nil {
		t.Fatalf("expected nil ccs to fail validation")
	}
}
