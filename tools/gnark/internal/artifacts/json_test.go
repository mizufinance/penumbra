package artifacts

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/consensys/gnark-crypto/ecc/bls12-377/fp"
)

func testVerifyingKeyJSON() VerifyingKeyJSON {
	g1 := G1PointJSON{X: "1", Y: "2"}
	g2 := G2PointJSON{
		X: Fq2JSON{A0: "3", A1: "4"},
		Y: Fq2JSON{A0: "5", A1: "6"},
	}
	return VerifyingKeyJSON{
		AlphaG1:    g1,
		BetaG2:     g2,
		GammaG2:    g2,
		DeltaG2:    g2,
		GammaABCG1: []G1PointJSON{g1},
	}
}

func TestDecodeCanonicalVerifyingKeyJSONRejectsAlternateEncodings(t *testing.T) {
	value := testVerifyingKeyJSON()
	canonical, err := EncodeCanonicalJSON(value)
	if err != nil {
		t.Fatalf("encode canonical JSON: %v", err)
	}
	if _, err := DecodeCanonicalVerifyingKeyJSON(canonical); err != nil {
		t.Fatalf("decode canonical JSON: %v", err)
	}

	duplicate := bytes.Replace(
		canonical,
		[]byte("{\n"),
		[]byte("{\n  \"alpha_g1\": null,\n"),
		1,
	)
	if _, err := DecodeCanonicalVerifyingKeyJSON(duplicate); err == nil {
		t.Fatal("duplicate JSON field must fail")
	}

	unknown := bytes.Replace(
		canonical,
		[]byte("{\n"),
		[]byte("{\n  \"unknown\": 0,\n"),
		1,
	)
	if _, err := DecodeCanonicalVerifyingKeyJSON(unknown); err == nil {
		t.Fatal("unknown JSON field must fail")
	}

	compact, err := json.Marshal(value)
	if err != nil {
		t.Fatalf("encode compact JSON: %v", err)
	}
	if _, err := DecodeCanonicalVerifyingKeyJSON(compact); err == nil {
		t.Fatal("noncanonical JSON formatting must fail")
	}

	value.AlphaG1.X = fp.Modulus().String()
	modularAlias, err := EncodeCanonicalJSON(value)
	if err != nil {
		t.Fatalf("encode modular alias: %v", err)
	}
	if _, err := DecodeCanonicalVerifyingKeyJSON(modularAlias); err == nil {
		t.Fatal("coordinate equal to the base-field modulus must fail")
	}

	value.AlphaG1.X = "-1"
	negative, err := EncodeCanonicalJSON(value)
	if err != nil {
		t.Fatalf("encode negative coordinate: %v", err)
	}
	if _, err := DecodeCanonicalVerifyingKeyJSON(negative); err == nil {
		t.Fatal("negative coordinate must fail")
	}

	value.AlphaG1.X = "01"
	leadingZero, err := EncodeCanonicalJSON(value)
	if err != nil {
		t.Fatalf("encode leading-zero coordinate: %v", err)
	}
	if _, err := DecodeCanonicalVerifyingKeyJSON(leadingZero); err == nil {
		t.Fatal("leading-zero coordinate must fail")
	}
}

func TestDecodeCanonicalCircuitMetadataJSONRejectsOldOrAlternateEncodings(t *testing.T) {
	value := testCircuitMetadata()
	canonical, err := EncodeCanonicalJSON(value)
	if err != nil {
		t.Fatalf("encode canonical JSON: %v", err)
	}
	if _, err := DecodeCanonicalCircuitMetadataJSON(canonical); err != nil {
		t.Fatalf("decode canonical metadata: %v", err)
	}

	oldSchema := bytes.Replace(
		canonical,
		[]byte(CircuitMetadataSchema),
		[]byte("shieldd.gnark.wrong_metadata"),
		1,
	)
	if _, err := DecodeCanonicalCircuitMetadataJSON(oldSchema); err == nil {
		t.Fatal("old metadata schema must fail")
	}

	unknown := bytes.Replace(
		canonical,
		[]byte("{\n"),
		[]byte("{\n  \"verifying_key_sha256_hex\": \"legacy\",\n"),
		1,
	)
	if _, err := DecodeCanonicalCircuitMetadataJSON(unknown); err == nil {
		t.Fatal("ambiguous legacy VK hash field must fail")
	}

	optionalID := bytes.Replace(
		canonical,
		[]byte("{\n"),
		[]byte("{\n  \"verifying_key_id\": \"redundant\",\n"),
		1,
	)
	if _, err := DecodeCanonicalCircuitMetadataJSON(optionalID); err == nil {
		t.Fatal("metadata fields outside the exact schema must fail")
	}

	missingPin := bytes.Replace(
		canonical,
		[]byte(`  "proving_key_sha256_hex": "`+value.ProvingKeySHA256Hex+"\",\n"),
		nil,
		1,
	)
	if _, err := DecodeCanonicalCircuitMetadataJSON(missingPin); err == nil {
		t.Fatal("missing proving-key pin must fail")
	}

	missingProvenancePin := bytes.Replace(
		canonical,
		[]byte(`  "setup_provenance_sha256_hex": "`+value.SetupProvenanceSHA256Hex+"\",\n"),
		nil,
		1,
	)
	if _, err := DecodeCanonicalCircuitMetadataJSON(missingProvenancePin); err == nil {
		t.Fatal("missing setup-provenance pin must fail")
	}

	compact, err := json.Marshal(value)
	if err != nil {
		t.Fatalf("encode compact metadata: %v", err)
	}
	if _, err := DecodeCanonicalCircuitMetadataJSON(compact); err == nil {
		t.Fatal("noncanonical metadata formatting must fail")
	}
}
