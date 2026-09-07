package artifacts

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"math/big"
	"strings"

	"github.com/consensys/gnark-crypto/ecc/bls12-377/fp"
)

// EncodeCanonicalJSON returns the repository's canonical artifact encoding.
func EncodeCanonicalJSON(value any) ([]byte, error) {
	var out bytes.Buffer
	encoder := json.NewEncoder(&out)
	encoder.SetIndent("", "  ")
	if err := encoder.Encode(value); err != nil {
		return nil, err
	}
	return out.Bytes(), nil
}

// DecodeCanonicalVerifyingKeyJSON rejects unknown fields, trailing JSON, and
// any byte representation other than EncodeCanonicalJSON's output.
func DecodeCanonicalVerifyingKeyJSON(data []byte) (VerifyingKeyJSON, error) {
	var vk VerifyingKeyJSON
	if err := decodeCanonicalJSON(data, &vk, "verifying key JSON"); err != nil {
		return VerifyingKeyJSON{}, err
	}
	if err := validateVerifyingKeyCoordinates(vk); err != nil {
		return VerifyingKeyJSON{}, err
	}
	return vk, nil
}

// DecodeCanonicalCircuitMetadataJSON accepts exactly the canonical metadata encoding.
func DecodeCanonicalCircuitMetadataJSON(data []byte) (CircuitMetadataJSON, error) {
	var metadata CircuitMetadataJSON
	if err := decodeCanonicalJSON(data, &metadata, "circuit metadata JSON"); err != nil {
		return CircuitMetadataJSON{}, err
	}
	if metadata.Schema != CircuitMetadataSchema {
		return CircuitMetadataJSON{}, fmt.Errorf(
			"unsupported circuit metadata schema %q",
			metadata.Schema,
		)
	}
	for label, digest := range map[string]string{
		"SR1CS":                metadata.SR1CSSHA256Hex,
		"setup provenance":     metadata.SetupProvenanceSHA256Hex,
		"proving key":          metadata.ProvingKeySHA256Hex,
		"binary verifying key": metadata.VerifyingKeyBinarySHA256Hex,
		"JSON verifying key":   metadata.VerifyingKeyJSONSHA256Hex,
	} {
		if !isLowerSHA256(digest) {
			return CircuitMetadataJSON{}, fmt.Errorf(
				"circuit metadata %s hash is not a lowercase SHA-256 digest",
				label,
			)
		}
	}
	return metadata, nil
}

func decodeCanonicalJSON(data []byte, value any, label string) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()

	if err := decoder.Decode(value); err != nil {
		return fmt.Errorf("decode %s: %w", label, err)
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		if err == nil {
			return fmt.Errorf("%s contains trailing value", label)
		}
		return fmt.Errorf("decode trailing %s: %w", label, err)
	}

	canonical, err := EncodeCanonicalJSON(value)
	if err != nil {
		return fmt.Errorf("re-encode %s: %w", label, err)
	}
	if !bytes.Equal(data, canonical) {
		return fmt.Errorf("%s is not canonical", label)
	}
	return nil
}

func isLowerSHA256(value string) bool {
	if len(value) != 64 || value != strings.ToLower(value) {
		return false
	}
	_, err := hex.DecodeString(value)
	return err == nil
}

func validateVerifyingKeyCoordinates(vk VerifyingKeyJSON) error {
	coordinates := []struct {
		label string
		value string
	}{
		{"alpha_g1.x", vk.AlphaG1.X},
		{"alpha_g1.y", vk.AlphaG1.Y},
		{"beta_g2.x.a0", vk.BetaG2.X.A0},
		{"beta_g2.x.a1", vk.BetaG2.X.A1},
		{"beta_g2.y.a0", vk.BetaG2.Y.A0},
		{"beta_g2.y.a1", vk.BetaG2.Y.A1},
		{"gamma_g2.x.a0", vk.GammaG2.X.A0},
		{"gamma_g2.x.a1", vk.GammaG2.X.A1},
		{"gamma_g2.y.a0", vk.GammaG2.Y.A0},
		{"gamma_g2.y.a1", vk.GammaG2.Y.A1},
		{"delta_g2.x.a0", vk.DeltaG2.X.A0},
		{"delta_g2.x.a1", vk.DeltaG2.X.A1},
		{"delta_g2.y.a0", vk.DeltaG2.Y.A0},
		{"delta_g2.y.a1", vk.DeltaG2.Y.A1},
	}
	for i, point := range vk.GammaABCG1 {
		coordinates = append(coordinates,
			struct {
				label string
				value string
			}{fmt.Sprintf("gamma_abc_g1[%d].x", i), point.X},
			struct {
				label string
				value string
			}{fmt.Sprintf("gamma_abc_g1[%d].y", i), point.Y},
		)
	}
	for _, coordinate := range coordinates {
		if err := validateCanonicalFieldDecimal(coordinate.value); err != nil {
			return fmt.Errorf("verifying key coordinate %s: %w", coordinate.label, err)
		}
	}
	return nil
}

func validateCanonicalFieldDecimal(value string) error {
	integer, ok := new(big.Int).SetString(value, 10)
	if !ok || integer.Sign() < 0 || integer.String() != value {
		return fmt.Errorf("not a canonical nonnegative decimal")
	}
	if integer.Cmp(fp.Modulus()) >= 0 {
		return fmt.Errorf("outside the BLS12-377 base field")
	}
	return nil
}
