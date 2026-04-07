package main

import (
	"encoding/json"
	"strings"
	"testing"
)

const syntheticManifestJSON = `{
  "families": [
    {
      "id": 1,
      "label": "transfer1x1",
      "artifact_name": "transfer1x1",
      "n_in": 1,
      "n_out": 1,
      "bundled_lib_basename": "libpenumbra_gnark_transfer"
    },
    {
      "id": 9,
      "label": "transfer3x3",
      "artifact_name": "transfer3x3",
      "n_in": 3,
      "n_out": 3,
      "bundled_lib_basename": "libpenumbra_gnark_transfer"
    }
  ]
}`

func loadSyntheticManifest(t *testing.T) manifest {
	t.Helper()

	var m manifest
	raw := []byte(syntheticManifestJSON)
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatalf("unmarshal manifest: %v", err)
	}
	if err := prepareManifest(&m, raw); err != nil {
		t.Fatalf("prepare manifest: %v", err)
	}
	return m
}

func TestPrepareManifestDerivesTransferFamilySymbols(t *testing.T) {
	m := loadSyntheticManifest(t)
	family := m.Families[1]

	if family.RustConst != "ThreeByThree" {
		t.Fatalf("unexpected Rust const: got %q", family.RustConst)
	}
	if family.EnvArtifactDir != "PENUMBRA_GNARK_TRANSFER3X3_ARTIFACT_DIR" {
		t.Fatalf("unexpected env artifact dir: got %q", family.EnvArtifactDir)
	}
	if family.VerificationKeyConst != "TRANSFER3X3_PROOF_VERIFICATION_KEY" {
		t.Fatalf("unexpected verification key const: got %q", family.VerificationKeyConst)
	}
	if family.ProvingKeyBytesConst != "TRANSFER3X3_PROOF_PROVING_KEY_BYTES" {
		t.Fatalf("unexpected proving key bytes const: got %q", family.ProvingKeyBytesConst)
	}
	if family.MetadataBytesConst != "TRANSFER3X3_CIRCUIT_METADATA" {
		t.Fatalf("unexpected metadata const: got %q", family.MetadataBytesConst)
	}
	if m.ManifestSHA256 == "" {
		t.Fatal("manifest SHA256 should be populated")
	}
}

func TestRenderTemplatesSupportAdditionalTransferFamilies(t *testing.T) {
	m := loadSyntheticManifest(t)

	testCases := []struct {
		name     string
		template string
		path     string
		gofmt    bool
		snippets []string
	}{
		{
			name:     "go registry",
			template: goTemplate,
			path:     "transfer_families_generated.go",
			gofmt:    true,
			snippets: []string{
				"Manifest SHA256: " + m.ManifestSHA256,
				`"transfer3x3"`,
				`NIn:                3`,
				`NOut:               3`,
			},
		},
		{
			name:     "shielded pool registry",
			template: rustShieldedPoolTemplate,
			path:     "generated.rs",
			snippets: []string{
				"pub const ThreeByThree: Self = Self(9);",
				`label: "transfer3x3"`,
				"transfer_statement_field_count(3, 3)",
			},
		},
		{
			name:     "proof params registry",
			template: proofParamsRegistryTemplate,
			path:     "transfer_registry.rs",
			snippets: []string{
				"TRANSFER3X3_PROOF_VERIFICATION_KEY",
				`include_bytes!("transfer3x3/verifying_key.json")`,
				"transfer_proof_verification_key",
			},
		},
		{
			name:     "aggregation dispatch",
			template: proofAggregationDispatchTemplate,
			path:     "transfer_family_dispatch.rs",
			snippets: []string{
				"TransferFamilyId::ThreeByThree",
				"crate::backend::verify_with_digest_profiled",
				"crate::backend::aggregate_with_digest_profiled",
			},
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			rendered, err := renderTemplate(tc.path, tc.template, m, tc.gofmt)
			if err != nil {
				t.Fatalf("render template: %v", err)
			}
			output := string(rendered)
			for _, snippet := range tc.snippets {
				if !strings.Contains(output, snippet) {
					t.Fatalf("rendered output missing %q\n%s", snippet, output)
				}
			}
		})
	}
}
