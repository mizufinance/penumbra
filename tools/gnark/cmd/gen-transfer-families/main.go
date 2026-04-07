package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"go/format"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"text/template"
)

type manifest struct {
	ManifestSHA256 string   `json:"manifest_sha256,omitempty"`
	Families       []family `json:"families"`
}

type family struct {
	ID                 uint32 `json:"id"`
	Label              string `json:"label"`
	ArtifactName       string `json:"artifact_name"`
	NIn                int    `json:"n_in"`
	NOut               int    `json:"n_out"`
	BundledLibBasename string `json:"bundled_lib_basename"`

	RustConst            string `json:"-"`
	EnvArtifactDir       string `json:"-"`
	VerificationKeyConst string `json:"-"`
	ProvingKeyBytesConst string `json:"-"`
	MetadataBytesConst   string `json:"-"`
}

func main() {
	root, err := repoRoot()
	if err != nil {
		fail(err)
	}

	manifestPath := filepath.Join(root, "tools/gnark/transfer_families.json")
	data, err := os.ReadFile(manifestPath)
	if err != nil {
		fail(err)
	}

	var m manifest
	if err := json.Unmarshal(data, &m); err != nil {
		fail(err)
	}
	if err := prepareManifest(&m, data); err != nil {
		fail(err)
	}

	outputs := []struct {
		path     string
		gofmt    bool
		template string
	}{
		{
			path:     filepath.Join(root, "tools/gnark/internal/generated/transfer_families_generated.go"),
			gofmt:    true,
			template: goTemplate,
		},
		{
			path:     filepath.Join(root, "crates/core/component/shielded-pool/src/transfer/generated.rs"),
			template: rustShieldedPoolTemplate,
		},
		{
			path:     filepath.Join(root, "crates/crypto/proof-params/src/gen/gnark/transfer_families_manifest.json"),
			template: proofParamsManifestTemplate,
		},
		{
			path:     filepath.Join(root, "crates/crypto/proof-params/src/gen/gnark/transfer_families_build.rs"),
			template: proofParamsBuildTemplate,
		},
		{
			path:     filepath.Join(root, "crates/crypto/proof-params/src/gen/gnark/transfer_registry.rs"),
			template: proofParamsRegistryTemplate,
		},
		{
			path:     filepath.Join(root, "crates/crypto/proof-aggregation/src/transfer_family_dispatch.rs"),
			template: proofAggregationDispatchTemplate,
		},
	}

	for _, output := range outputs {
		if err := writeTemplate(output.path, output.template, m, output.gofmt); err != nil {
			fail(err)
		}
	}
}

func repoRoot() (string, error) {
	wd, err := os.Getwd()
	if err != nil {
		return "", err
	}
	for dir := wd; ; dir = filepath.Dir(dir) {
		candidate := filepath.Join(dir, "tools/gnark/transfer_families.json")
		if _, err := os.Stat(candidate); err == nil {
			return dir, nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
	}
	return "", fmt.Errorf("could not locate repo root from %s", wd)
}

func writeTemplate(path, source string, data any, gofmt bool) error {
	out, err := renderTemplate(path, source, data, gofmt)
	if err != nil {
		return err
	}

	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("mkdir %s: %w", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, out, 0o644); err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}
	return nil
}

func renderTemplate(path, source string, data any, gofmt bool) ([]byte, error) {
	tpl, err := template.New(filepath.Base(path)).Funcs(template.FuncMap{
		"sub1": func(v int) int { return v - 1 },
	}).Parse(source)
	if err != nil {
		return nil, fmt.Errorf("parse %s template: %w", path, err)
	}

	var buf bytes.Buffer
	if err := tpl.Execute(&buf, data); err != nil {
		return nil, fmt.Errorf("render %s: %w", path, err)
	}

	out := buf.Bytes()
	if gofmt {
		formatted, err := format.Source(out)
		if err != nil {
			return nil, fmt.Errorf("gofmt %s: %w", path, err)
		}
		out = formatted
	}
	return out, nil
}

func fail(err error) {
	fmt.Fprintln(os.Stderr, err)
	os.Exit(1)
}

func prepareManifest(m *manifest, raw []byte) error {
	m.ManifestSHA256 = sha256Hex(raw)
	seenIDs := make(map[uint32]struct{}, len(m.Families))
	seenLabels := make(map[string]struct{}, len(m.Families))
	seenShapes := make(map[string]struct{}, len(m.Families))
	for i := range m.Families {
		family := &m.Families[i]
		if family.ID == 0 {
			return fmt.Errorf("transfer family %q has invalid id 0", family.Label)
		}
		if family.Label == "" {
			return fmt.Errorf("transfer family %d has empty label", family.ID)
		}
		if family.ArtifactName == "" {
			return fmt.Errorf("transfer family %q has empty artifact_name", family.Label)
		}
		if family.NIn <= 0 || family.NOut <= 0 {
			return fmt.Errorf(
				"transfer family %q has invalid shape (%d, %d)",
				family.Label,
				family.NIn,
				family.NOut,
			)
		}
		if _, ok := seenIDs[family.ID]; ok {
			return fmt.Errorf("duplicate transfer family id %d", family.ID)
		}
		seenIDs[family.ID] = struct{}{}
		if _, ok := seenLabels[family.Label]; ok {
			return fmt.Errorf("duplicate transfer family label %q", family.Label)
		}
		seenLabels[family.Label] = struct{}{}
		shapeKey := fmt.Sprintf("%d:%d", family.NIn, family.NOut)
		if _, ok := seenShapes[shapeKey]; ok {
			return fmt.Errorf("duplicate transfer family shape (%d, %d)", family.NIn, family.NOut)
		}
		seenShapes[shapeKey] = struct{}{}

		constStem := constStem(family.Label)
		family.RustConst = rustConstName(family.NIn, family.NOut)
		family.EnvArtifactDir = fmt.Sprintf("PENUMBRA_GNARK_%s_ARTIFACT_DIR", constStem)
		family.VerificationKeyConst = fmt.Sprintf("%s_PROOF_VERIFICATION_KEY", constStem)
		family.ProvingKeyBytesConst = fmt.Sprintf("%s_PROOF_PROVING_KEY_BYTES", constStem)
		family.MetadataBytesConst = fmt.Sprintf("%s_CIRCUIT_METADATA", constStem)
	}
	return nil
}

func sha256Hex(raw []byte) string {
	sum := sha256.Sum256(raw)
	return hex.EncodeToString(sum[:])
}

func constStem(label string) string {
	var builder strings.Builder
	for _, r := range label {
		switch {
		case r >= 'a' && r <= 'z':
			builder.WriteRune(r - ('a' - 'A'))
		case r >= 'A' && r <= 'Z':
			builder.WriteRune(r)
		case r >= '0' && r <= '9':
			builder.WriteRune(r)
		default:
			builder.WriteByte('_')
		}
	}
	return builder.String()
}

func rustConstName(nIn, nOut int) string {
	return numberComponent(nIn) + "By" + numberComponent(nOut)
}

func numberComponent(n int) string {
	words := map[int]string{
		0:  "Zero",
		1:  "One",
		2:  "Two",
		3:  "Three",
		4:  "Four",
		5:  "Five",
		6:  "Six",
		7:  "Seven",
		8:  "Eight",
		9:  "Nine",
		10: "Ten",
	}
	if word, ok := words[n]; ok {
		return word
	}
	return "N" + strconv.Itoa(n)
}

const goTemplate = `// Code generated by gen-transfer-families; DO NOT EDIT.
// Manifest SHA256: {{ .ManifestSHA256 }}
package generated

type TransferFamilySpec struct {
	ID                 uint32
	Label              string
	ArtifactName       string
	NIn                int
	NOut               int
	BundledLibBasename string
}

var TransferFamilies = []TransferFamilySpec{
{{- range .Families }}
	{
		ID: {{ .ID }},
		Label: "{{ .Label }}",
		ArtifactName: "{{ .ArtifactName }}",
		NIn: {{ .NIn }},
		NOut: {{ .NOut }},
		BundledLibBasename: "{{ .BundledLibBasename }}",
	},
{{- end }}
}

func TransferFamilyByID(id uint32) (TransferFamilySpec, bool) {
	for _, family := range TransferFamilies {
		if family.ID == id {
			return family, true
		}
	}
	return TransferFamilySpec{}, false
}

func TransferFamilyByLabel(label string) (TransferFamilySpec, bool) {
	for _, family := range TransferFamilies {
		if family.Label == label {
			return family, true
		}
	}
	return TransferFamilySpec{}, false
}
`

const rustShieldedPoolTemplate = `// Code generated by gen-transfer-families; DO NOT EDIT.
// Manifest SHA256: {{ .ManifestSHA256 }}
use anyhow::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct TransferFamilyId(pub u32);

#[allow(non_upper_case_globals)]
impl TransferFamilyId {
{{- range .Families }}
    pub const {{ .RustConst }}: Self = Self({{ .ID }});
{{- end }}

    pub const ALL: [Self; {{ len .Families }}] = [
{{- range .Families }}
        Self::{{ .RustConst }},
{{- end }}
    ];

    pub const fn get(self) -> u32 {
        self.0
    }

    pub fn label(self) -> &'static str {
        self.spec().label
    }

    pub fn input_count(self) -> usize {
        self.spec().n_in
    }

    pub fn output_count(self) -> usize {
        self.spec().n_out
    }

    pub fn auth_sig_count(self) -> usize {
        self.input_count()
    }

    pub const fn proving_implemented(self) -> bool {
        let _ = self;
        true
    }

    pub const fn planner_enabled(self) -> bool {
        let _ = self;
        true
    }

    pub fn from_shape(inputs: usize, outputs: usize) -> Option<Self> {
        Self::ALL.into_iter().find(|family| {
            let spec = family.spec();
            spec.n_in == inputs && spec.n_out == outputs
        })
    }

    pub fn spec(self) -> &'static TransferFamilySpec {
        TRANSFER_FAMILY_SPECS
            .iter()
            .find(|spec| spec.id == self)
            .expect("unknown transfer family id")
    }
}

impl TryFrom<u32> for TransferFamilyId {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let family = Self(value);
        if TRANSFER_FAMILY_SPECS.iter().any(|spec| spec.id == family) {
            Ok(family)
        } else {
            Err(anyhow::anyhow!("unknown transfer family id {value}"))
        }
    }
}

impl From<TransferFamilyId> for u32 {
    fn from(value: TransferFamilyId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferFamilySpec {
    pub id: TransferFamilyId,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub n_in: usize,
    pub n_out: usize,
    pub statement_field_count: usize,
}

pub const TRANSFER_FAMILY_SPECS: [TransferFamilySpec; {{ len .Families }}] = [
{{- range .Families }}
    TransferFamilySpec {
        id: TransferFamilyId::{{ .RustConst }},
        label: "{{ .Label }}",
        artifact_name: "{{ .ArtifactName }}",
        n_in: {{ .NIn }},
        n_out: {{ .NOut }},
        statement_field_count: crate::public_input_hash::transfer_statement_field_count({{ .NIn }}, {{ .NOut }}),
    },
{{- end }}
];
`

const proofParamsManifestTemplate = `{
  "manifest_sha256": "{{ .ManifestSHA256 }}",
  "families": [
{{- range $index, $family := .Families }}
    {
      "id": {{ $family.ID }},
      "label": "{{ $family.Label }}",
      "artifact_name": "{{ $family.ArtifactName }}",
      "bundled_lib_basename": "{{ $family.BundledLibBasename }}",
      "n_in": {{ $family.NIn }},
      "n_out": {{ $family.NOut }}
    }{{ if lt $index (sub1 (len $.Families)) }},{{ end }}
{{- end }}
  ]
}
`

const proofParamsBuildTemplate = `// Code generated by gen-transfer-families; DO NOT EDIT.
// Manifest SHA256: {{ .ManifestSHA256 }}
pub struct GeneratedTransferFamily {
    pub id: u32,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub n_in: usize,
    pub n_out: usize,
    pub bundled_lib_basename: &'static str,
}

pub const GENERATED_TRANSFER_FAMILIES: &[GeneratedTransferFamily] = &[
{{- range .Families }}
    GeneratedTransferFamily {
        id: {{ .ID }},
        label: "{{ .Label }}",
        artifact_name: "{{ .ArtifactName }}",
        n_in: {{ .NIn }},
        n_out: {{ .NOut }},
        bundled_lib_basename: "{{ .BundledLibBasename }}",
    },
{{- end }}
];
`

const proofParamsRegistryTemplate = `// Code generated by gen-transfer-families; DO NOT EDIT.
// Manifest SHA256: {{ .ManifestSHA256 }}
#[derive(Clone, Copy, Debug)]
struct GeneratedTransferProofFamily {
    id: u32,
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}

{{ range .Families -}}
/// Verification key for the {{ .Label }} proof.
static {{ .VerificationKeyConst }}: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {
    if let Some(dir) = std::env::var_os("{{ .EnvArtifactDir }}") {
        return load_verifying_key_json_artifact(Path::new(&dir), "{{ .ArtifactName }}")
            .expect("can deserialize {{ .Label }} VerifyingKey")
            .into();
    }
    load_verifying_key_json_bytes(include_bytes!("{{ .ArtifactName }}/verifying_key.json"))
        .expect("bundled {{ .Label }} VerifyingKey is valid")
        .into()
});

/// {{ .Label }} proving key bytes. Non-empty only with the ` + "`bundled-proving-keys`" + ` feature.
static {{ .ProvingKeyBytesConst }}: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!("{{ .ArtifactName }}/proving_key.bin")
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

/// Bundled gnark {{ .Label }} circuit metadata JSON.
static {{ .MetadataBytesConst }}: &[u8] = include_bytes!("{{ .ArtifactName }}/circuit_metadata.json");

{{ end -}}
static GENERATED_TRANSFER_PROOF_FAMILIES: &[GeneratedTransferProofFamily] = &[
{{- range .Families }}
    GeneratedTransferProofFamily {
        id: {{ .ID }},
        verification_key: &{{ .VerificationKeyConst }},
        proving_key_bytes: {{ .ProvingKeyBytesConst }},
        metadata_bytes: {{ .MetadataBytesConst }},
    },
{{- end }}
];

fn transfer_proof_family(family_id: u32) -> &'static GeneratedTransferProofFamily {
    GENERATED_TRANSFER_PROOF_FAMILIES
        .iter()
        .find(|family| family.id == family_id)
        .unwrap_or_else(|| panic!("unknown transfer family id {family_id}"))
}

pub fn transfer_proof_verification_key(family_id: u32) -> &'static PreparedVerifyingKey<Bls12_377> {
    &**transfer_proof_family(family_id).verification_key
}

pub fn transfer_proving_key_bytes(family_id: u32) -> &'static [u8] {
    transfer_proof_family(family_id).proving_key_bytes
}

pub fn transfer_circuit_metadata(family_id: u32) -> &'static [u8] {
    transfer_proof_family(family_id).metadata_bytes
}
`

const proofAggregationDispatchTemplate = `// Code generated by gen-transfer-families; DO NOT EDIT.
// Manifest SHA256: {{ .ManifestSHA256 }}
use anyhow::Result;
use ark_groth16::PreparedVerifyingKey;
use decaf377::{Bls12_377, Fq};
use penumbra_sdk_proof_params::batch::BatchItem;
use penumbra_sdk_shielded_pool::TransferFamilyId;

use crate::{
    backend::{AggregateBuildBackendProfile, AggregateVerificationProfile},
    srs::DevSrs,
    transcript::TransferTranscriptDigest,
};

/// Verifies a transfer-family aggregate proof and returns an
/// AggregateVerificationProfile for instrumentation.
///
/// "Unchecked" here does not bypass SnarkPack verification. It means the caller must
/// already have selected the correct transfer family, transcript domain, verifying key,
/// and padded public inputs before calling this helper.
///
/// Safe usage: internal validation and profiling paths after transfer-family routing and
/// aggregate input preparation have already succeeded.
///
/// Unsafe usage: calling this helper with mismatched family metadata, transcript domain,
/// or public inputs. That will still run cryptographic verification, but the result and
/// profiling data will not be meaningful for the intended family.
pub(crate) fn verify_transfer_family_aggregate_profiled_unchecked_generated(
    family_id: TransferFamilyId,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<AggregateVerificationProfile> {
    match family_id {
{{- range .Families }}
        TransferFamilyId::{{ .RustConst }} => crate::backend::verify_with_digest_profiled::<
            TransferTranscriptDigest<{ TransferFamilyId::{{ .RustConst }}.get() }>,
        >(pvk, aggregate_proof_bytes, padded_public_inputs, srs),
{{- end }}
        other => anyhow::bail!("unknown transfer family id {}", other.get()),
    }
}

pub(crate) fn aggregate_transfer_family_generated(
    family_id: TransferFamilyId,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<Vec<u8>> {
    match family_id {
{{- range .Families }}
        TransferFamilyId::{{ .RustConst }} => crate::backend::aggregate_with_digest::<
            TransferTranscriptDigest<{ TransferFamilyId::{{ .RustConst }}.get() }>,
        >(items, srs),
{{- end }}
        other => anyhow::bail!("unknown transfer family id {}", other.get()),
    }
}

pub(crate) fn verify_transfer_family_aggregate_generated(
    family_id: TransferFamilyId,
    pvk: &PreparedVerifyingKey<Bls12_377>,
    aggregate_proof_bytes: &[u8],
    padded_public_inputs: &[Vec<Fq>],
    srs: &DevSrs,
) -> Result<bool> {
    match family_id {
{{- range .Families }}
        TransferFamilyId::{{ .RustConst }} => crate::backend::verify_with_digest::<
            TransferTranscriptDigest<{ TransferFamilyId::{{ .RustConst }}.get() }>,
        >(pvk, aggregate_proof_bytes, padded_public_inputs, srs),
{{- end }}
        other => anyhow::bail!("unknown transfer family id {}", other.get()),
    }
}

pub(crate) fn aggregate_transfer_family_profiled_generated(
    family_id: TransferFamilyId,
    items: &[BatchItem],
    srs: &DevSrs,
) -> Result<(Vec<u8>, AggregateBuildBackendProfile)> {
    match family_id {
{{- range .Families }}
        TransferFamilyId::{{ .RustConst }} => crate::backend::aggregate_with_digest_profiled::<
            TransferTranscriptDigest<{ TransferFamilyId::{{ .RustConst }}.get() }>,
        >(items, srs),
{{- end }}
        other => anyhow::bail!("unknown transfer family id {}", other.get()),
    }
}
`
