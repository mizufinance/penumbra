#!/usr/bin/env python3
"""Generate or check canonical gnark family-registry projections."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
GNARK = ROOT / "tools" / "gnark"
TRANSFER_MANIFEST = GNARK / "transfer_families.json"
WITHDRAWAL_MANIFEST = GNARK / "shielded_ics20_withdrawal_families.json"
NOTE_RESHAPE_MANIFEST = GNARK / "note_reshape_families.json"


def fail(message: str) -> None:
    raise SystemExit(f"gnark family registry check failed: {message}")


def strict_json(path: Path) -> dict[str, object]:
    def no_duplicates(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                fail(f"{path}: duplicate JSON key {key!r}")
            result[key] = value
        return result

    try:
        value = json.loads(
            path.read_text(encoding="utf-8"),
            object_pairs_hook=no_duplicates,
            parse_constant=lambda token: fail(
                f"{path}: non-finite JSON number {token!r}"
            ),
        )
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail(f"cannot load {path}: {error}")
    if not isinstance(value, dict):
        fail(f"{path}: root must be an object")
    return value


def families(
    path: Path,
    schema: str,
    family_fields: set[str],
    *,
    exactly_one: bool = False,
) -> list[dict[str, object]]:
    manifest = strict_json(path)
    if set(manifest) != {"schema", "families"}:
        fail(f"{path}: expected exactly schema and families")
    if manifest["schema"] != schema:
        fail(f"{path}: unsupported schema {manifest['schema']!r}")
    values = manifest["families"]
    if not isinstance(values, list) or not values:
        fail(f"{path}: families must be a nonempty array")
    if exactly_one and len(values) != 1:
        fail(f"{path}: the fixed Transfer architecture requires exactly one family")

    labels: set[str] = set()
    ids: set[int] = set()
    checked: list[dict[str, object]] = []
    for index, value in enumerate(values, start=1):
        if not isinstance(value, dict) or set(value) != family_fields:
            fail(f"{path}: family {index} has an unexpected field set")
        label = value.get("label")
        artifact = value.get("artifact_name")
        basename = value.get("bundled_lib_basename")
        if (
            not isinstance(label, str)
            or re.fullmatch(r"[a-z0-9_]+", label) is None
            or label in labels
        ):
            fail(f"{path}: family {index} has an invalid or duplicate label")
        labels.add(label)
        for field, text in (
            ("artifact_name", artifact),
            ("bundled_lib_basename", basename),
        ):
            if not isinstance(text, str) or re.fullmatch(r"[a-z0-9_]+", text) is None:
                fail(f"{path}: family {label} has invalid {field}")
        for field in ("n_in", "n_out"):
            count = value.get(field)
            if type(count) is not int or count <= 0:
                fail(f"{path}: family {label} has invalid {field}")
        if "id" in family_fields:
            family_id = value.get("id")
            rust_name = value.get("rust_name")
            if (
                type(family_id) is not int
                or family_id <= 0
                or family_id in ids
            ):
                fail(f"{path}: family {label} has invalid or duplicate id")
            ids.add(family_id)
            if (
                not isinstance(rust_name, str)
                or re.fullmatch(r"[A-Z][A-Za-z0-9]*", rust_name) is None
            ):
                fail(f"{path}: family {label} has invalid rust_name")
        checked.append(value)
    return checked


def rust_symbol(label: str) -> str:
    return re.sub(r"[^A-Za-z0-9]", "_", label).upper()


def render_transfer_go(values: list[dict[str, object]]) -> str:
    entries = "\n".join(
        """\t{
\t\tLabel:              "%(label)s",
\t\tArtifactName:       "%(artifact_name)s",
\t\tNIn:                %(n_in)d,
\t\tNOut:               %(n_out)d,
\t\tBundledLibBasename: "%(bundled_lib_basename)s",
\t},""" % value
        for value in values
    )
    return f"""// Code generated from transfer_families.json. DO NOT EDIT.
package generated

type TransferFamilySpec struct {{
\tLabel              string
\tArtifactName       string
\tNIn                int
\tNOut               int
\tBundledLibBasename string
}}

var TransferFamilies = []TransferFamilySpec{{
{entries}
}}

func TransferFamilyByLabel(label string) (TransferFamilySpec, bool) {{
\tfor _, family := range TransferFamilies {{
\t\tif family.Label == label {{
\t\t\treturn family, true
\t\t}}
\t}}
\treturn TransferFamilySpec{{}}, false
}}
"""


def render_transfer_core(values: list[dict[str, object]]) -> str:
    family = values[0]
    return """// Code generated from transfer_families.json. DO NOT EDIT.
pub const PADDED_TRANSFER_INPUTS: usize = %(n_in)d;
pub const PADDED_TRANSFER_OUTPUTS: usize = %(n_out)d;
pub const TRANSFER_STATEMENT_FIELD_COUNT: usize =
    crate::public_input_hash::transfer_statement_field_count(
        PADDED_TRANSFER_INPUTS,
        PADDED_TRANSFER_OUTPUTS,
    );
pub const TRANSFER_PROOF_LABEL: &str = "%(label)s";
pub const TRANSFER_ARTIFACT_NAME: &str = "%(artifact_name)s";

pub fn transfer_input_count() -> usize {
    PADDED_TRANSFER_INPUTS
}

pub fn transfer_output_count() -> usize {
    PADDED_TRANSFER_OUTPUTS
}

pub fn transfer_auth_sig_count() -> usize {
    PADDED_TRANSFER_INPUTS
}

#[cfg(test)]
mod tests {
    use super::{
        transfer_auth_sig_count, transfer_input_count, transfer_output_count,
        TRANSFER_ARTIFACT_NAME, TRANSFER_PROOF_LABEL, TRANSFER_STATEMENT_FIELD_COUNT,
    };

    #[test]
    fn transfer_shape_is_fixed() {
        assert_eq!(transfer_input_count(), %(n_in)d);
        assert_eq!(transfer_output_count(), %(n_out)d);
        assert_eq!(transfer_auth_sig_count(), %(n_in)d);
        assert_eq!(
            TRANSFER_STATEMENT_FIELD_COUNT,
            crate::public_input_hash::transfer_statement_field_count(%(n_in)d, %(n_out)d)
        );
        assert_eq!(TRANSFER_PROOF_LABEL, "%(label)s");
        assert_eq!(TRANSFER_ARTIFACT_NAME, "%(artifact_name)s");
    }
}
""" % family


def render_transfer_build(values: list[dict[str, object]]) -> str:
    entries = "\n".join(
        """    GeneratedTransferFamily {
        label: "%(label)s",
        artifact_name: "%(artifact_name)s",
        n_in: %(n_in)d,
        n_out: %(n_out)d,
        bundled_lib_basename: "%(bundled_lib_basename)s",
    },""" % value
        for value in values
    )
    return f"""// Code generated from transfer_families.json. DO NOT EDIT.
pub struct GeneratedTransferFamily {{
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub n_in: usize,
    pub n_out: usize,
    pub bundled_lib_basename: &'static str,
}}

pub const GENERATED_TRANSFER_FAMILIES: &[GeneratedTransferFamily] = &[
{entries}
];
"""


def render_transfer_projection(
    values: list[dict[str, object]], source_hash: str
) -> str:
    projection = {
        "schema": "shieldd.transfer_family_projection",
        "source_sha256": source_hash,
        "families": values,
    }
    return json.dumps(projection, indent=2) + "\n"


def render_transfer_proof_registry(values: list[dict[str, object]]) -> str:
    family = values[0]
    label = str(family["label"])
    artifact = str(family["artifact_name"])
    symbol = rust_symbol(label)
    return f"""// Code generated from transfer_families.json. DO NOT EDIT.
#[derive(Clone, Copy, Debug)]
struct GeneratedTransferProofFamily {{
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    verifying_key_json_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}}

static {symbol}_PROOF_VERIFYING_KEY_JSON_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/{artifact}/verifying_key.json"
));

static {symbol}_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {{
    load_verifying_key_json_bytes({symbol}_PROOF_VERIFYING_KEY_JSON_BYTES)
    .expect("bundled {label} VerifyingKey is valid")
    .into()
}});

static {symbol}_PROOF_PROVING_KEY_BYTES: &[u8] = {{
    #[cfg(feature = "bundled-proving-keys")]
    {{
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/{artifact}/proving_key.bin"
        ))
    }}
    #[cfg(not(feature = "bundled-proving-keys"))]
    {{
        &[]
    }}
}};

static {symbol}_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/{artifact}/circuit_metadata.json"
));

static GENERATED_TRANSFER_PROOF_FAMILY: GeneratedTransferProofFamily =
    GeneratedTransferProofFamily {{
        verification_key: &{symbol}_PROOF_VERIFICATION_KEY,
        proving_key_bytes: {symbol}_PROOF_PROVING_KEY_BYTES,
        verifying_key_json_bytes: {symbol}_PROOF_VERIFYING_KEY_JSON_BYTES,
        metadata_bytes: {symbol}_CIRCUIT_METADATA,
    }};

pub fn transfer_proof_verification_key() -> &'static PreparedVerifyingKey<Bls12_377> {{
    &**GENERATED_TRANSFER_PROOF_FAMILY.verification_key
}}

pub fn transfer_proving_key_bytes() -> &'static [u8] {{
    GENERATED_TRANSFER_PROOF_FAMILY.proving_key_bytes
}}

pub fn transfer_verifying_key_json_bytes() -> &'static [u8] {{
    GENERATED_TRANSFER_PROOF_FAMILY.verifying_key_json_bytes
}}

pub fn transfer_circuit_metadata() -> &'static [u8] {{
    GENERATED_TRANSFER_PROOF_FAMILY.metadata_bytes
}}
"""


def note_reshape_input_padding_go(value: object) -> str:
    return {
        "fixed": "InputPaddingFixed",
        "synthetic_private": "InputPaddingSyntheticPrivate",
    }[str(value)]


def note_reshape_output_padding_go(value: object) -> str:
    return {
        "fixed": "OutputPaddingFixed",
        "zero_note": "OutputPaddingZeroNote",
    }[str(value)]


def note_reshape_input_padding_rust(value: object) -> str:
    return {
        "fixed": "InputPaddingPolicy::Fixed",
        "synthetic_private": "InputPaddingPolicy::SyntheticPrivate",
    }[str(value)]


def note_reshape_output_padding_rust(value: object) -> str:
    return {
        "fixed": "OutputPaddingPolicy::Fixed",
        "zero_note": "OutputPaddingPolicy::ZeroNote",
    }[str(value)]


def render_note_reshape_go(values: list[dict[str, object]]) -> str:
    entries = "\n".join(
        "\t{ID: %(id)d, Label: \"%(label)s\", ArtifactName: \"%(artifact_name)s\", "
        "InputPadding: %(input_padding)s, OutputPadding: %(output_padding)s, "
        "NIn: %(n_in)d, NOut: %(n_out)d, MinRealInputs: %(min_real_inputs)d, "
        "MaxRealInputs: %(max_real_inputs)d, MinRealOutputs: %(min_real_outputs)d, "
        "MaxRealOutputs: %(max_real_outputs)d},"
        % {
            **value,
            "input_padding": note_reshape_input_padding_go(value["input_padding"]),
            "output_padding": note_reshape_output_padding_go(value["output_padding"]),
        }
        for value in values
    )
    return f"""// Code generated from note_reshape_families.json. DO NOT EDIT.
package generated

type NoteReshapeFamilySpec struct {{
\tID             uint32
\tLabel          string
\tArtifactName   string
\tInputPadding   InputPaddingPolicy
\tOutputPadding  OutputPaddingPolicy
\tNIn            int
\tNOut           int
\tMinRealInputs  int
\tMaxRealInputs  int
\tMinRealOutputs int
\tMaxRealOutputs int
}}

type InputPaddingPolicy uint8

const (
\tInputPaddingFixed InputPaddingPolicy = iota
\tInputPaddingSyntheticPrivate
)

type OutputPaddingPolicy uint8

const (
\tOutputPaddingFixed OutputPaddingPolicy = iota
\tOutputPaddingZeroNote
)

var NoteReshapeFamilies = []NoteReshapeFamilySpec{{
{entries}
}}

func NoteReshapeFamilyByID(id uint32) (NoteReshapeFamilySpec, bool) {{
\tfor _, family := range NoteReshapeFamilies {{
\t\tif family.ID == id {{
\t\t\treturn family, true
\t\t}}
\t}}
\treturn NoteReshapeFamilySpec{{}}, false
}}

func NoteReshapeFamilyByLabel(label string) (NoteReshapeFamilySpec, bool) {{
\tfor _, family := range NoteReshapeFamilies {{
\t\tif family.Label == label {{
\t\t\treturn family, true
\t\t}}
\t}}
\treturn NoteReshapeFamilySpec{{}}, false
}}

func SmallestNoteReshapeFamily(realInputs, realOutputs int) (NoteReshapeFamilySpec, bool) {{
\tvar selected NoteReshapeFamilySpec
\tfound := false
\tfor _, family := range NoteReshapeFamilies {{
\t\tif realInputs < family.MinRealInputs || realInputs > family.MaxRealInputs ||
\t\t\trealOutputs < family.MinRealOutputs || realOutputs > family.MaxRealOutputs {{
\t\t\tcontinue
\t\t}}
\t\tif !found || family.NIn < selected.NIn || (family.NIn == selected.NIn && family.NOut < selected.NOut) {{
\t\t\tselected = family
\t\t\tfound = true
\t\t}}
\t}}
\treturn selected, found
}}
"""


def render_note_reshape_core(values: list[dict[str, object]]) -> str:
    constants = "\n".join(
        f"    pub const {value['rust_name']}: Self = Self({value['id']});"
        for value in values
    )
    all_values = ", ".join(f"Self::{value['rust_name']}" for value in values)
    specs = "\n".join(
        f"""    NoteReshapeFamilySpec {{
        id: NoteReshapeFamilyId::{value['rust_name']},
        label: \"{value['label']}\",
        artifact_name: \"{value['artifact_name']}\",
        input_padding: {note_reshape_input_padding_rust(value['input_padding'])},
        output_padding: {note_reshape_output_padding_rust(value['output_padding'])},
        n_in: {value['n_in']},
        n_out: {value['n_out']},
        min_real_inputs: {value['min_real_inputs']},
        max_real_inputs: {value['max_real_inputs']},
        min_real_outputs: {value['min_real_outputs']},
        max_real_outputs: {value['max_real_outputs']},
    }},"""
        for value in values
    )
    return f"""// Code generated from note_reshape_families.json. DO NOT EDIT.
use anyhow::Error;

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Deserialize, serde::Serialize,
)]
#[serde(try_from = \"u32\", into = \"u32\")]
pub struct NoteReshapeFamilyId(u32);

#[allow(non_upper_case_globals)]
impl NoteReshapeFamilyId {{
{constants}

    pub const ALL: [Self; {len(values)}] = [{all_values}];

    pub const fn get(self) -> u32 {{
        self.0
    }}

    pub fn label(self) -> &'static str {{
        self.spec().label
    }}
    pub fn input_count(self) -> usize {{
        self.spec().n_in
    }}
    pub fn output_count(self) -> usize {{
        self.spec().n_out
    }}
    pub fn min_real_inputs(self) -> usize {{
        self.spec().min_real_inputs
    }}
    pub fn max_real_inputs(self) -> usize {{
        self.spec().max_real_inputs
    }}
    pub fn min_real_outputs(self) -> usize {{
        self.spec().min_real_outputs
    }}
    pub fn max_real_outputs(self) -> usize {{
        self.spec().max_real_outputs
    }}
    pub fn auth_sig_count(self) -> usize {{
        self.input_count()
    }}
    pub fn is_many_to_one(self) -> bool {{
        self.output_count() == 1
    }}

    pub fn spec(self) -> &'static NoteReshapeFamilySpec {{
        NOTE_RESHAPE_FAMILY_SPECS
            .iter()
            .find(|spec| spec.id == self)
            .expect(\"unknown note reshape family id\")
    }}

    pub fn supports_real_counts(self, real_inputs: usize, real_outputs: usize) -> bool {{
        (self.min_real_inputs()..=self.max_real_inputs()).contains(&real_inputs)
            && (self.min_real_outputs()..=self.max_real_outputs()).contains(&real_outputs)
    }}

    pub fn smallest_covering(real_inputs: usize, real_outputs: usize) -> Option<Self> {{
        Self::ALL
            .into_iter()
            .filter(|family| family.supports_real_counts(real_inputs, real_outputs))
            .min_by_key(|family| (family.input_count(), family.output_count()))
    }}

    pub fn validate_real_counts(
        self,
        real_inputs: usize,
        real_outputs: usize,
    ) -> anyhow::Result<()> {{
        anyhow::ensure!(
            self.supports_real_counts(real_inputs, real_outputs),
            \"{{}} does not support {{}} real inputs and {{}} real outputs\",
            self.label(),
            real_inputs,
            real_outputs
        );
        Ok(())
    }}
}}

impl TryFrom<u32> for NoteReshapeFamilyId {{
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {{
        let family = Self(value);
        if NOTE_RESHAPE_FAMILY_SPECS
            .iter()
            .any(|spec| spec.id == family)
        {{
            Ok(family)
        }} else {{
            Err(anyhow::anyhow!(\"unknown note reshape family id {{value}}\"))
        }}
    }}
}}

impl From<NoteReshapeFamilyId> for u32 {{
    fn from(value: NoteReshapeFamilyId) -> Self {{
        value.0
    }}
}}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputPaddingPolicy {{
    Fixed,
    SyntheticPrivate,
}}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputPaddingPolicy {{
    Fixed,
    ZeroNote,
}}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoteReshapeFamilySpec {{
    pub id: NoteReshapeFamilyId,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub input_padding: InputPaddingPolicy,
    pub output_padding: OutputPaddingPolicy,
    pub n_in: usize,
    pub n_out: usize,
    pub min_real_inputs: usize,
    pub max_real_inputs: usize,
    pub min_real_outputs: usize,
    pub max_real_outputs: usize,
}}

pub const NOTE_RESHAPE_FAMILY_SPECS: [NoteReshapeFamilySpec; {len(values)}] = [
{specs}
];

#[cfg(test)]
mod tests {{
    use super::{{InputPaddingPolicy, NoteReshapeFamilyId, OutputPaddingPolicy}};

    #[test]
    fn registry_declares_the_two_padded_directions() {{
        assert_eq!(NoteReshapeFamilyId::ALL.len(), 2);
        assert_eq!(
            NoteReshapeFamilyId::EightByOne.spec().input_padding,
            InputPaddingPolicy::SyntheticPrivate
        );
        assert_eq!(
            NoteReshapeFamilyId::EightByOne.spec().output_padding,
            OutputPaddingPolicy::Fixed
        );
        assert_eq!(
            NoteReshapeFamilyId::OneByEight.spec().input_padding,
            InputPaddingPolicy::Fixed
        );
        assert_eq!(
            NoteReshapeFamilyId::OneByEight.spec().output_padding,
            OutputPaddingPolicy::ZeroNote
        );
    }}

    #[test]
    fn canonical_selection_is_directional_and_unique() {{
        for real_inputs in 2..=8 {{
            assert_eq!(
                NoteReshapeFamilyId::smallest_covering(real_inputs, 1),
                Some(NoteReshapeFamilyId::EightByOne)
            );
        }}
        for real_outputs in 2..=8 {{
            assert_eq!(
                NoteReshapeFamilyId::smallest_covering(1, real_outputs),
                Some(NoteReshapeFamilyId::OneByEight)
            );
        }}
        assert_eq!(NoteReshapeFamilyId::smallest_covering(1, 1), None);
        assert_eq!(NoteReshapeFamilyId::smallest_covering(9, 1), None);
    }}

    #[test]
    fn serde_rejects_unsupported_family_ids() {{
        for id in [0, 1, 4, 99] {{
            let error = serde_json::from_str::<NoteReshapeFamilyId>(&id.to_string())
                .expect_err(\"unknown family ids must fail closed\");
            assert!(error.to_string().contains(\"unknown note reshape family id\"));
        }}
    }}
}}
"""


def render_note_reshape_build(values: list[dict[str, object]]) -> str:
    entries = "\n".join(
        f"""    GeneratedNoteReshapeFamily {{
        id: {value['id']},
        label: \"{value['label']}\",
        artifact_name: \"{value['artifact_name']}\",
        input_padding: {note_reshape_input_padding_rust(value['input_padding'])},
        output_padding: {note_reshape_output_padding_rust(value['output_padding'])},
        n_in: {value['n_in']},
        n_out: {value['n_out']},
        min_real_inputs: {value['min_real_inputs']},
        max_real_inputs: {value['max_real_inputs']},
        min_real_outputs: {value['min_real_outputs']},
        max_real_outputs: {value['max_real_outputs']},
    }},"""
        for value in values
    )
    return f"""// Code generated from note_reshape_families.json. DO NOT EDIT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputPaddingPolicy {{
    Fixed,
    SyntheticPrivate,
}}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputPaddingPolicy {{
    Fixed,
    ZeroNote,
}}

pub struct GeneratedNoteReshapeFamily {{
    pub id: u32,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub input_padding: InputPaddingPolicy,
    pub output_padding: OutputPaddingPolicy,
    pub n_in: usize,
    pub n_out: usize,
    pub min_real_inputs: usize,
    pub max_real_inputs: usize,
    pub min_real_outputs: usize,
    pub max_real_outputs: usize,
}}

pub const GENERATED_NOTE_RESHAPE_FAMILIES: &[GeneratedNoteReshapeFamily] = &[
{entries}
];
"""


def render_note_reshape_proof_registry(
    values: list[dict[str, object]],
) -> str:
    statics: list[str] = []
    entries: list[str] = []
    for value in values:
        family_id = int(value["id"])
        label = str(value["label"])
        artifact = str(value["artifact_name"])
        symbol = f"NOTE_RESHAPE{int(value['n_in'])}X{int(value['n_out'])}"
        statics.append(
            f"""static {symbol}_PROOF_VERIFYING_KEY_JSON_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/{artifact}/verifying_key.json"
));

static {symbol}_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {{
    load_verifying_key_json_bytes({symbol}_PROOF_VERIFYING_KEY_JSON_BYTES)
        .expect("bundled {label} VerifyingKey is valid")
        .into()
}});

static {symbol}_PROOF_PROVING_KEY_BYTES: &[u8] = {{
    #[cfg(feature = "bundled-proving-keys")]
    {{
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/{artifact}/proving_key.bin"
        ))
    }}
    #[cfg(not(feature = "bundled-proving-keys"))]
    {{
        &[]
    }}
}};

static {symbol}_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/{artifact}/circuit_metadata.json"
));"""
        )
        entries.append(
            f"""    GeneratedNoteReshapeProofFamily {{
        id: {family_id},
        verification_key: &{symbol}_PROOF_VERIFICATION_KEY,
        proving_key_bytes: {symbol}_PROOF_PROVING_KEY_BYTES,
        verifying_key_json_bytes: {symbol}_PROOF_VERIFYING_KEY_JSON_BYTES,
        metadata_bytes: {symbol}_CIRCUIT_METADATA,
    }},"""
        )
    return """// Code generated from note_reshape_families.json. DO NOT EDIT.
#[derive(Clone, Copy, Debug)]
struct GeneratedNoteReshapeProofFamily {
    id: u32,
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    verifying_key_json_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}

%s

static GENERATED_NOTE_RESHAPE_PROOF_FAMILIES: &[GeneratedNoteReshapeProofFamily] = &[
%s
];

fn note_reshape_proof_family(family_id: u32) -> &'static GeneratedNoteReshapeProofFamily {
    GENERATED_NOTE_RESHAPE_PROOF_FAMILIES
        .iter()
        .find(|family| family.id == family_id)
        .unwrap_or_else(|| panic!("unknown note_reshape family id {family_id}"))
}

pub fn note_reshape_proof_verification_key(
    family_id: u32,
) -> &'static PreparedVerifyingKey<Bls12_377> {
    &**note_reshape_proof_family(family_id).verification_key
}

pub fn note_reshape_proving_key_bytes(family_id: u32) -> &'static [u8] {
    note_reshape_proof_family(family_id).proving_key_bytes
}

pub fn note_reshape_verifying_key_json_bytes(family_id: u32) -> &'static [u8] {
    note_reshape_proof_family(family_id).verifying_key_json_bytes
}

pub fn note_reshape_circuit_metadata(family_id: u32) -> &'static [u8] {
    note_reshape_proof_family(family_id).metadata_bytes
}
""" % ("\n\n".join(statics), "\n".join(entries))


def render_withdrawal_go(values: list[dict[str, object]]) -> str:
    entries = "\n".join(
        """\t{
\t\tID:                 %(id)d,
\t\tLabel:              "%(label)s",
\t\tArtifactName:       "%(artifact_name)s",
\t\tNIn:                %(n_in)d,
\t\tNOut:               %(n_out)d,
\t\tBundledLibBasename: "%(bundled_lib_basename)s",
\t},""" % value
        for value in values
    )
    return f"""// Code generated from shielded_ics20_withdrawal_families.json. DO NOT EDIT.
package generated

type ShieldedIcs20WithdrawalFamilySpec struct {{
\tID                 uint32
\tLabel              string
\tArtifactName       string
\tNIn                int
\tNOut               int
\tBundledLibBasename string
}}

var ShieldedIcs20WithdrawalFamilies = []ShieldedIcs20WithdrawalFamilySpec{{
{entries}
}}

func ShieldedIcs20WithdrawalFamilyByID(id uint32) (ShieldedIcs20WithdrawalFamilySpec, bool) {{
\tfor _, family := range ShieldedIcs20WithdrawalFamilies {{
\t\tif family.ID == id {{
\t\t\treturn family, true
\t\t}}
\t}}
\treturn ShieldedIcs20WithdrawalFamilySpec{{}}, false
}}

func ShieldedIcs20WithdrawalFamilyByLabel(label string) (ShieldedIcs20WithdrawalFamilySpec, bool) {{
\tfor _, family := range ShieldedIcs20WithdrawalFamilies {{
\t\tif family.Label == label {{
\t\t\treturn family, true
\t\t}}
\t}}
\treturn ShieldedIcs20WithdrawalFamilySpec{{}}, false
}}
"""


def render_withdrawal_core(values: list[dict[str, object]]) -> str:
    constants = "\n".join(
        f"    pub const {value['rust_name']}: Self = Self({value['id']});"
        for value in values
    )
    if len(values) == 1:
        all_declaration = f"[Self::{values[0]['rust_name']}]"
    else:
        all_values = ",\n".join(
            f"        Self::{value['rust_name']}" for value in values
        )
        all_declaration = f"[\n{all_values},\n    ]"
    specs = "\n".join(
        """ShieldedIcs20WithdrawalFamilySpec {
        id: ShieldedIcs20WithdrawalFamilyId::%(rust_name)s,
        label: "%(label)s",
        artifact_name: "%(artifact_name)s",
        bundled_lib_basename: "%(bundled_lib_basename)s",
        n_in: %(n_in)d,
        n_out: %(n_out)d,
    }""" % value
        for value in values
    )
    if len(values) == 1:
        specs_declaration = f"[{specs}]"
    else:
        specs_declaration = "[\n    " + specs.replace(
            "\n", "\n    "
        ).replace("}\n    Shielded", "},\n    Shielded") + ",\n]"
    return f"""// Code generated from shielded_ics20_withdrawal_families.json. DO NOT EDIT.
use anyhow::Error;

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Deserialize, serde::Serialize,
)]
#[serde(try_from = "u32", into = "u32")]
pub struct ShieldedIcs20WithdrawalFamilyId(u32);

#[allow(non_upper_case_globals)]
impl ShieldedIcs20WithdrawalFamilyId {{
{constants}

    pub const ALL: [Self; {len(values)}] = {all_declaration};

    pub const fn get(self) -> u32 {{
        self.0
    }}

    pub fn label(self) -> &'static str {{
        self.spec().label
    }}

    pub fn input_count(self) -> usize {{
        self.spec().n_in
    }}

    pub fn output_count(self) -> usize {{
        self.spec().n_out
    }}

    pub fn auth_sig_count(self) -> usize {{
        self.spec().n_in
    }}

    pub fn spec(self) -> &'static ShieldedIcs20WithdrawalFamilySpec {{
        SHIELDED_ICS20_WITHDRAWAL_FAMILY_SPECS
            .iter()
            .find(|spec| spec.id == self)
            .expect("unknown shielded ICS-20 withdrawal family id")
    }}
}}

impl TryFrom<u32> for ShieldedIcs20WithdrawalFamilyId {{
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {{
        let family = Self(value);
        if SHIELDED_ICS20_WITHDRAWAL_FAMILY_SPECS
            .iter()
            .any(|spec| spec.id == family)
        {{
            Ok(family)
        }} else {{
            Err(anyhow::anyhow!(
                "unknown shielded ICS-20 withdrawal family id {{value}}"
            ))
        }}
    }}
}}

impl From<ShieldedIcs20WithdrawalFamilyId> for u32 {{
    fn from(value: ShieldedIcs20WithdrawalFamilyId) -> Self {{
        value.0
    }}
}}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShieldedIcs20WithdrawalFamilySpec {{
    pub id: ShieldedIcs20WithdrawalFamilyId,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub bundled_lib_basename: &'static str,
    pub n_in: usize,
    pub n_out: usize,
}}

pub const SHIELDED_ICS20_WITHDRAWAL_FAMILY_SPECS: [ShieldedIcs20WithdrawalFamilySpec; {len(values)}] =
    {specs_declaration};
"""


def render_withdrawal_build(values: list[dict[str, object]]) -> str:
    entries = "\n".join(
        """    GeneratedShieldedIcs20WithdrawalFamily {
        id: %(id)d,
        label: "%(label)s",
        artifact_name: "%(artifact_name)s",
        bundled_lib_basename: "%(bundled_lib_basename)s",
        n_in: %(n_in)d,
        n_out: %(n_out)d,
    },""" % value
        for value in values
    )
    return f"""// Code generated from shielded_ics20_withdrawal_families.json. DO NOT EDIT.
pub struct GeneratedShieldedIcs20WithdrawalFamily {{
    pub id: u32,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub bundled_lib_basename: &'static str,
    pub n_in: usize,
    pub n_out: usize,
}}

pub const GENERATED_SHIELDED_ICS20_WITHDRAWAL_FAMILIES:
    &[GeneratedShieldedIcs20WithdrawalFamily] = &[
{entries}
];
"""


def render_withdrawal_proof_registry(values: list[dict[str, object]]) -> str:
    statics: list[str] = []
    entries: list[str] = []
    for value in values:
        family_id = int(value["id"])
        label = str(value["label"])
        artifact = str(value["artifact_name"])
        symbol = rust_symbol(label)
        statics.append(
            f"""static {symbol}_PROOF_VERIFYING_KEY_JSON_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/{artifact}/verifying_key.json"
));

static {symbol}_PROOF_VERIFICATION_KEY: Lazy<
    PreparedVerifyingKey<Bls12_377>,
> = Lazy::new(|| {{
    load_verifying_key_json_bytes({symbol}_PROOF_VERIFYING_KEY_JSON_BYTES)
    .expect("bundled {label} VerifyingKey is valid")
    .into()
}});

static {symbol}_PROOF_PROVING_KEY_BYTES: &[u8] = {{
    #[cfg(feature = "bundled-proving-keys")]
    {{
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/{artifact}/proving_key.bin"
        ))
    }}
    #[cfg(not(feature = "bundled-proving-keys"))]
    {{
        &[]
    }}
}};

static {symbol}_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/{artifact}/circuit_metadata.json"
));"""
        )
        entries.append(
            f"""    GeneratedShieldedIcs20WithdrawalProofFamily {{
        id: {family_id},
        verification_key: &{symbol}_PROOF_VERIFICATION_KEY,
        proving_key_bytes: {symbol}_PROOF_PROVING_KEY_BYTES,
        verifying_key_json_bytes: {symbol}_PROOF_VERIFYING_KEY_JSON_BYTES,
        metadata_bytes: {symbol}_CIRCUIT_METADATA,
    }},"""
        )
    return """// Code generated from shielded_ics20_withdrawal_families.json. DO NOT EDIT.
#[derive(Clone, Copy, Debug)]
struct GeneratedShieldedIcs20WithdrawalProofFamily {
    id: u32,
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    verifying_key_json_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}

%s

static GENERATED_SHIELDED_ICS20_WITHDRAWAL_PROOF_FAMILIES:
    &[GeneratedShieldedIcs20WithdrawalProofFamily] = &[
%s
];

fn shielded_ics20_withdrawal_proof_family(
    family_id: u32,
) -> &'static GeneratedShieldedIcs20WithdrawalProofFamily {
    GENERATED_SHIELDED_ICS20_WITHDRAWAL_PROOF_FAMILIES
        .iter()
        .find(|family| family.id == family_id)
        .unwrap_or_else(|| panic!("unknown shielded ICS-20 withdrawal family id {family_id}"))
}

pub fn shielded_ics20_withdrawal_proof_verification_key(
    family_id: u32,
) -> &'static PreparedVerifyingKey<Bls12_377> {
    &**shielded_ics20_withdrawal_proof_family(family_id).verification_key
}

pub fn shielded_ics20_withdrawal_proving_key_bytes(family_id: u32) -> &'static [u8] {
    shielded_ics20_withdrawal_proof_family(family_id).proving_key_bytes
}

pub fn shielded_ics20_withdrawal_verifying_key_json_bytes(family_id: u32) -> &'static [u8] {
    shielded_ics20_withdrawal_proof_family(family_id).verifying_key_json_bytes
}

pub fn shielded_ics20_withdrawal_circuit_metadata(family_id: u32) -> &'static [u8] {
    shielded_ics20_withdrawal_proof_family(family_id).metadata_bytes
}
""" % ("\n\n".join(statics), "\n".join(entries))


def note_reshape_families() -> list[dict[str, object]]:
    manifest = strict_json(NOTE_RESHAPE_MANIFEST)
    if manifest.get("schema") != "shieldd.note_reshape_families":
        fail("unexpected NoteReshape family manifest schema")
    values = manifest.get("families")
    if (
        not isinstance(values, list)
        or not values
        or any(not isinstance(value, dict) for value in values)
    ):
        fail("NoteReshape families must be a nonempty object array")
    ids = [value.get("id") for value in values]
    labels = [value.get("label") for value in values]
    if len(ids) != len(set(ids)) or len(labels) != len(set(labels)):
        fail("NoteReshape family ids and labels must be unique")
    return values


def generated_outputs() -> dict[Path, str]:
    transfer = families(
        TRANSFER_MANIFEST,
        "shieldd.transfer_families",
        {
            "label",
            "artifact_name",
            "bundled_lib_basename",
            "n_in",
            "n_out",
        },
        exactly_one=True,
    )
    withdrawal = families(
        WITHDRAWAL_MANIFEST,
        "shieldd.shielded_ics20_withdrawal_families",
        {
            "id",
            "rust_name",
            "label",
            "artifact_name",
            "bundled_lib_basename",
            "n_in",
            "n_out",
        },
    )
    if any(value["n_out"] != 1 for value in withdrawal):
        fail("withdrawal circuit architecture requires exactly one output")
    if len({value["bundled_lib_basename"] for value in withdrawal}) != 1:
        fail(
            "all withdrawal families must share the one per-kind bundled prover library"
        )
    note_reshape = note_reshape_families()
    transfer_hash = hashlib.sha256(TRANSFER_MANIFEST.read_bytes()).hexdigest()
    return {
        GNARK
        / "internal/generated/transfer_families_generated.go": render_transfer_go(
            transfer
        ),
        ROOT
        / "crates/core/component/shielded-pool/src/transfer/generated.rs": render_transfer_core(
            transfer
        ),
        ROOT
        / "crates/crypto/proof-params/src/gen/gnark/transfer_families_build.rs": render_transfer_build(
            transfer
        ),
        ROOT
        / "crates/crypto/proof-params/src/gen/gnark/transfer_families_manifest.json": render_transfer_projection(
            transfer, transfer_hash
        ),
        ROOT
        / "crates/crypto/proof-params/src/gen/gnark/transfer_registry.rs": render_transfer_proof_registry(
            transfer
        ),
        ROOT
        / "crates/crypto/proof-params/src/gen/gnark/note_reshape_registry.rs": render_note_reshape_proof_registry(
            note_reshape
        ),
        GNARK
        / "internal/generated/note_reshape_families_generated.go": render_note_reshape_go(
            note_reshape
        ),
        ROOT
        / "crates/core/component/shielded-pool/src/note_reshape/generated.rs": render_note_reshape_core(
            note_reshape
        ),
        ROOT
        / "crates/crypto/proof-params/src/gen/gnark/note_reshape_families_build.rs": render_note_reshape_build(
            note_reshape
        ),
        GNARK
        / "internal/generated/shielded_ics20_withdrawal_families_generated.go": render_withdrawal_go(
            withdrawal
        ),
        ROOT
        / "crates/core/component/shielded-pool/src/shielded_ics20_withdrawal/generated.rs": render_withdrawal_core(
            withdrawal
        ),
        ROOT
        / "crates/crypto/proof-params/src/gen/gnark/shielded_ics20_withdrawal_families_build.rs": render_withdrawal_build(
            withdrawal
        ),
        ROOT
        / "crates/crypto/proof-params/src/gen/gnark/shielded_ics20_withdrawal_registry.rs": render_withdrawal_proof_registry(
            withdrawal
        ),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--write",
        action="store_true",
        help="write deterministic circuit-family projections",
    )
    args = parser.parse_args()

    outputs = generated_outputs()
    changed: list[Path] = []
    for path, expected in outputs.items():
        actual = path.read_text(encoding="utf-8") if path.is_file() else None
        if actual == expected:
            continue
        if not args.write:
            changed.append(path)
            continue
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(expected, encoding="utf-8")

    if changed:
        fail(
            "generated registry projection drift: "
            + ", ".join(str(path.relative_to(ROOT)) for path in changed)
            + "; run tools/gnark/check_gnark_family_registries.py --write"
        )
    note_count = len(note_reshape_families())
    action = "wrote" if args.write else "checked"
    print(
        f"{action} canonical gnark registries: "
        f"{note_count} NoteReshape, 1 Transfer, "
        f"{len(strict_json(WITHDRAWAL_MANIFEST)['families'])} withdrawal"
    )


if __name__ == "__main__":
    main()
