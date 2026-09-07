package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/mizufinance/shieldd/tools/gnark/internal/artifacts"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
)

func TestRunExportWiringTranscript(t *testing.T) {
	cases := []struct {
		label string
		want  func() (string, error)
	}{
		{"note_reshape8x1", circuits.ExportNoteReshape8x1WiringTranscript},
		{"transfer", circuits.ExportTransferWiringTranscript},
	}

	for _, tc := range cases {
		t.Run(tc.label, func(t *testing.T) {
			outPath := filepath.Join(t.TempDir(), tc.label+".wiring")
			if err := runExportWiringTranscript([]string{
				"--circuit", tc.label,
				"--out", outPath,
			}); err != nil {
				t.Fatalf("export wiring transcript: %v", err)
			}

			got, err := os.ReadFile(outPath)
			if err != nil {
				t.Fatalf("read transcript: %v", err)
			}
			want, err := tc.want()
			if err != nil {
				t.Fatalf("export expected transcript: %v", err)
			}
			if string(got) != want {
				t.Fatalf("exported transcript mismatch")
			}
		})
	}
}

func TestRunExportWiringTranscriptRejectsUnsupportedCircuit(t *testing.T) {
	err := runExportWiringTranscript([]string{
		"--circuit", "split1x4",
		"--out", filepath.Join(t.TempDir(), "split1x4.wiring"),
	})
	if err == nil {
		t.Fatalf("expected unsupported circuit error")
	}
	if !strings.Contains(err.Error(), "unsupported wiring transcript circuit") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestValidateSetupSelfTestFlagsRequiresUniqueCasesAndWitnesses(t *testing.T) {
	if err := validateSetupSelfTestFlags(
		[]string{"regulated_unflagged", "unregulated_hidden"},
		[]string{"regulated.bin", "unregulated.bin"},
	); err != nil {
		t.Fatalf("valid setup coherence matrix rejected: %v", err)
	}
	for _, tc := range []struct {
		name      string
		cases     []string
		witnesses []string
		want      string
	}{
		{name: "empty", want: "equal nonzero counts"},
		{
			name:      "unequal",
			cases:     []string{"regulated"},
			witnesses: []string{"first.bin", "second.bin"},
			want:      "equal nonzero counts",
		},
		{
			name:      "invalid case",
			cases:     []string{"Regulated"},
			witnesses: []string{"regulated.bin"},
			want:      "invalid proof case",
		},
		{
			name:      "duplicate case",
			cases:     []string{"regulated", "regulated"},
			witnesses: []string{"first.bin", "second.bin"},
			want:      "duplicate proof case",
		},
		{
			name:      "duplicate witness",
			cases:     []string{"regulated", "unregulated"},
			witnesses: []string{"same.bin", "same.bin"},
			want:      "duplicate witness path",
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			err := validateSetupSelfTestFlags(tc.cases, tc.witnesses)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("error = %v, want substring %q", err, tc.want)
			}
		})
	}
}

func TestFilesEqualUsesExactBytes(t *testing.T) {
	dir := t.TempDir()
	first := filepath.Join(dir, "first")
	second := filepath.Join(dir, "second")
	if err := os.WriteFile(first, []byte("same"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(second, []byte("same"), 0o600); err != nil {
		t.Fatal(err)
	}
	equal, err := filesEqual(first, second)
	if err != nil || !equal {
		t.Fatalf("equal files: equal=%v err=%v", equal, err)
	}
	if err := os.WriteFile(second, []byte("different"), 0o600); err != nil {
		t.Fatal(err)
	}
	equal, err = filesEqual(first, second)
	if err != nil || equal {
		t.Fatalf("different files: equal=%v err=%v", equal, err)
	}
}

func TestLoadKeysRejectNonCanonicalEncodings(t *testing.T) {
	sourceDir := filepath.Join("..", "..", "artifacts", "note_reshape1x8")
	for _, tc := range []struct {
		name string
		file string
		load func(string) error
		raw  func([]byte) ([]byte, error)
	}{
		{
			name: "proving key",
			file: "proving_key.bin",
			load: func(path string) error {
				_, _, err := loadPK(path)
				return err
			},
			raw: func(data []byte) ([]byte, error) {
				key, err := artifacts.ReadProvingKeyStrict(bytes.NewReader(data))
				if err != nil {
					return nil, err
				}
				var encoded bytes.Buffer
				if _, err := key.WriteRawTo(&encoded); err != nil {
					return nil, err
				}
				return encoded.Bytes(), nil
			},
		},
		{
			name: "verifying key",
			file: "verifying_key.bin",
			load: func(path string) error {
				_, _, err := loadVK(path)
				return err
			},
			raw: func(data []byte) ([]byte, error) {
				key, err := artifacts.ReadVerifyingKeyStrict(bytes.NewReader(data))
				if err != nil {
					return nil, err
				}
				var encoded bytes.Buffer
				if _, err := key.WriteRawTo(&encoded); err != nil {
					return nil, err
				}
				return encoded.Bytes(), nil
			},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			data, err := os.ReadFile(filepath.Join(sourceDir, tc.file))
			if err != nil {
				t.Fatalf("read committed %s: %v", tc.name, err)
			}
			path := filepath.Join(t.TempDir(), tc.file)
			if err := os.WriteFile(path, data, 0o600); err != nil {
				t.Fatalf("write canonical %s: %v", tc.name, err)
			}
			if err := tc.load(path); err != nil {
				t.Fatalf("load canonical %s: %v", tc.name, err)
			}
			if err := os.WriteFile(path, append(data, 0x42), 0o600); err != nil {
				t.Fatalf("write suffixed %s: %v", tc.name, err)
			}
			if err := tc.load(path); err == nil {
				t.Fatalf("%s with trailing byte must fail", tc.name)
			}
			raw, err := tc.raw(data)
			if err != nil {
				t.Fatalf("encode raw %s: %v", tc.name, err)
			}
			if err := os.WriteFile(path, raw, 0o600); err != nil {
				t.Fatalf("write raw %s: %v", tc.name, err)
			}
			if err := tc.load(path); err == nil {
				t.Fatalf("raw %s encoding must fail", tc.name)
			}
		})
	}
}

func TestRunCheckVKJSONRequiresCanonicalStrictJSON(t *testing.T) {
	source := filepath.Join("..", "..", "artifacts", "note_reshape1x8", "verifying_key.bin")
	vkBytes, err := os.ReadFile(source)
	if err != nil {
		t.Fatalf("read committed verifying key: %v", err)
	}
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "verifying_key.bin"), vkBytes, 0o600); err != nil {
		t.Fatalf("write verifying key: %v", err)
	}
	vk, _, err := loadVK(filepath.Join(dir, "verifying_key.bin"))
	if err != nil {
		t.Fatalf("load verifying key: %v", err)
	}
	jsonPath := filepath.Join(dir, "verifying_key.json")
	if err := artifacts.WriteJSON(jsonPath, artifacts.EncodeVerifyingKeyJSON(vk)); err != nil {
		t.Fatalf("write canonical verifying key JSON: %v", err)
	}
	if err := runCheckVKJSON([]string{"--artifact-dir", dir}); err != nil {
		t.Fatalf("canonical verifying key JSON should pass: %v", err)
	}
	canonical, err := os.ReadFile(jsonPath)
	if err != nil {
		t.Fatalf("read canonical verifying key JSON: %v", err)
	}

	duplicate := bytes.Replace(
		canonical,
		[]byte("{\n"),
		[]byte("{\n  \"alpha_g1\": null,\n"),
		1,
	)
	unknown := bytes.Replace(
		canonical,
		[]byte("{\n"),
		[]byte("{\n  \"unknown\": 0,\n"),
		1,
	)
	compact, err := json.Marshal(artifacts.EncodeVerifyingKeyJSON(vk))
	if err != nil {
		t.Fatalf("encode compact verifying key JSON: %v", err)
	}
	for _, tc := range []struct {
		name string
		data []byte
	}{
		{name: "duplicate", data: duplicate},
		{name: "unknown", data: unknown},
		{name: "noncanonical", data: compact},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if err := os.WriteFile(jsonPath, tc.data, 0o600); err != nil {
				t.Fatalf("write mutated verifying key JSON: %v", err)
			}
			if err := runCheckVKJSON([]string{"--artifact-dir", dir}); err == nil {
				t.Fatalf("%s verifying key JSON must fail", tc.name)
			}
		})
	}
}
