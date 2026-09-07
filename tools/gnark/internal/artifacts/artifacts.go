package artifacts

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"path/filepath"

	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	"github.com/consensys/gnark/constraint"
)

type G1PointJSON struct {
	X string `json:"x"`
	Y string `json:"y"`
}

type Fq2JSON struct {
	A0 string `json:"a0"`
	A1 string `json:"a1"`
}

type G2PointJSON struct {
	X Fq2JSON `json:"x"`
	Y Fq2JSON `json:"y"`
}

type ProofJSON struct {
	A G1PointJSON `json:"a"`
	B G2PointJSON `json:"b"`
	C G1PointJSON `json:"c"`
}

type VerifyingKeyJSON struct {
	AlphaG1    G1PointJSON   `json:"alpha_g1"`
	BetaG2     G2PointJSON   `json:"beta_g2"`
	GammaG2    G2PointJSON   `json:"gamma_g2"`
	DeltaG2    G2PointJSON   `json:"delta_g2"`
	GammaABCG1 []G1PointJSON `json:"gamma_abc_g1"`
}

type TimingsJSON struct {
	CompileMS float64 `json:"compile_ms"`
	SetupMS   float64 `json:"setup_ms,omitempty"`
	LoadPKMS  float64 `json:"load_pk_ms,omitempty"`
	LoadVKMS  float64 `json:"load_vk_ms,omitempty"`
	ProveMS   float64 `json:"prove_ms"`
	VerifyMS  float64 `json:"verify_ms"`
}

type ArtifactJSON struct {
	Curve                string           `json:"curve"`
	Circuit              string           `json:"circuit"`
	PublicInputs         []string         `json:"public_inputs"`
	StatementFields      []string         `json:"statement_fields"`
	ClaimedStatementHash string           `json:"claimed_statement_hash"`
	Proof                ProofJSON        `json:"proof"`
	VerifyingKey         VerifyingKeyJSON `json:"verifying_key"`
	Timings              TimingsJSON      `json:"timings"`
}

type CircuitMetadataJSON struct {
	Schema                      string `json:"schema"`
	Curve                       string `json:"curve"`
	Circuit                     string `json:"circuit"`
	ProvingKeySize              int64  `json:"proving_key_size_bytes"`
	VerifyingKeySize            int64  `json:"verifying_key_size_bytes"`
	NbConstraints               int    `json:"nb_constraints"`
	NbPublic                    int    `json:"nb_public_variables"`
	NbSecret                    int    `json:"nb_secret_variables"`
	SR1CSSHA256Hex              string `json:"sr1cs_sha256_hex"`
	SetupProvenanceSHA256Hex    string `json:"setup_provenance_sha256_hex"`
	ProvingKeySHA256Hex         string `json:"proving_key_sha256_hex"`
	VerifyingKeyBinarySHA256Hex string `json:"verifying_key_binary_sha256_hex"`
	VerifyingKeyJSONSHA256Hex   string `json:"verifying_key_json_sha256_hex"`
}

const CircuitMetadataSchema = "shieldd.gnark.circuit_metadata"

func EncodeProofJSON(proof *groth16bls.Proof) ProofJSON {
	return ProofJSON{
		A: G1PointJSON{X: proof.Ar.X.String(), Y: proof.Ar.Y.String()},
		B: G2PointJSON{
			X: Fq2JSON{A0: proof.Bs.X.A0.String(), A1: proof.Bs.X.A1.String()},
			Y: Fq2JSON{A0: proof.Bs.Y.A0.String(), A1: proof.Bs.Y.A1.String()},
		},
		C: G1PointJSON{X: proof.Krs.X.String(), Y: proof.Krs.Y.String()},
	}
}

func EncodeVerifyingKeyJSON(vk *groth16bls.VerifyingKey) VerifyingKeyJSON {
	k := make([]G1PointJSON, len(vk.G1.K))
	for i := range vk.G1.K {
		k[i] = G1PointJSON{
			X: vk.G1.K[i].X.String(),
			Y: vk.G1.K[i].Y.String(),
		}
	}
	return VerifyingKeyJSON{
		AlphaG1: G1PointJSON{X: vk.G1.Alpha.X.String(), Y: vk.G1.Alpha.Y.String()},
		BetaG2: G2PointJSON{
			X: Fq2JSON{A0: vk.G2.Beta.X.A0.String(), A1: vk.G2.Beta.X.A1.String()},
			Y: Fq2JSON{A0: vk.G2.Beta.Y.A0.String(), A1: vk.G2.Beta.Y.A1.String()},
		},
		GammaG2: G2PointJSON{
			X: Fq2JSON{A0: vk.G2.Gamma.X.A0.String(), A1: vk.G2.Gamma.X.A1.String()},
			Y: Fq2JSON{A0: vk.G2.Gamma.Y.A0.String(), A1: vk.G2.Gamma.Y.A1.String()},
		},
		DeltaG2: G2PointJSON{
			X: Fq2JSON{A0: vk.G2.Delta.X.A0.String(), A1: vk.G2.Delta.X.A1.String()},
			Y: Fq2JSON{A0: vk.G2.Delta.Y.A0.String(), A1: vk.G2.Delta.Y.A1.String()},
		},
		GammaABCG1: k,
	}
}

func FileSize(path string) (int64, error) {
	info, err := os.Stat(path)
	if err != nil {
		return 0, err
	}
	return info.Size(), nil
}

func FillCircuitMetadataShape(metadata *CircuitMetadataJSON, ccs constraint.ConstraintSystem) {
	metadata.NbConstraints = ccs.GetNbConstraints()
	metadata.NbPublic = ccs.GetNbPublicVariables()
	metadata.NbSecret = ccs.GetNbSecretVariables()
}

func WriteConstraintSystem(path string, ccs constraint.ConstraintSystem) error {
	file, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("create constraint system file: %w", err)
	}
	writer := bufio.NewWriter(file)
	if err := writeConstraintSystem(writer, ccs); err != nil {
		_ = file.Close()
		return err
	}
	if err := writer.Flush(); err != nil {
		_ = file.Close()
		return fmt.Errorf("flush constraint system: %w", err)
	}
	if err := file.Close(); err != nil {
		return fmt.Errorf("close constraint system: %w", err)
	}
	return nil
}

// ConstraintSystemSHA256Hex hashes the canonical SR1CS encoding.
func ConstraintSystemSHA256Hex(ccs constraint.ConstraintSystem) (string, error) {
	digest := sha256.New()
	writer := bufio.NewWriter(digest)
	if err := writeConstraintSystem(writer, ccs); err != nil {
		return "", err
	}
	if err := writer.Flush(); err != nil {
		return "", fmt.Errorf("flush constraint-system digest: %w", err)
	}
	return hex.EncodeToString(digest.Sum(nil)), nil
}

func writeConstraintSystem(writer io.Writer, ccs constraint.ConstraintSystem) error {
	if ccs == nil {
		return fmt.Errorf("missing compiled constraint system")
	}
	r1cs, ok := ccs.(constraint.R1CS[constraint.U64])
	if !ok {
		return fmt.Errorf("constraint system is not a U64 R1CS")
	}
	if _, err := fmt.Fprintf(writer, "(prime-number %s)\n", ccs.Field().String()); err != nil {
		return err
	}

	for wire := ccs.GetNbPublicVariables(); wire < ccs.GetNbPublicVariables()+ccs.GetNbSecretVariables(); wire++ {
		if _, err := fmt.Fprintf(writer, "(in %d)\n", wire); err != nil {
			return err
		}
	}
	for wire := 1; wire < ccs.GetNbPublicVariables(); wire++ {
		if _, err := fmt.Fprintf(writer, "(out %d)\n", wire); err != nil {
			return err
		}
	}

	for _, r1c := range r1cs.GetR1Cs() {
		if _, err := fmt.Fprint(writer, "(constraint "); err != nil {
			return err
		}
		if err := writeLinearExpression(writer, ccs, r1c.L); err != nil {
			return err
		}
		if _, err := fmt.Fprint(writer, " "); err != nil {
			return err
		}
		if err := writeLinearExpression(writer, ccs, r1c.R); err != nil {
			return err
		}
		if _, err := fmt.Fprint(writer, " "); err != nil {
			return err
		}
		if err := writeLinearExpression(writer, ccs, r1c.O); err != nil {
			return err
		}
		if _, err := fmt.Fprintln(writer, ")"); err != nil {
			return err
		}
	}
	return nil
}

func writeLinearExpression(writer io.Writer, resolver constraint.Resolver, expr constraint.LinearExpression) error {
	if _, err := fmt.Fprint(writer, "["); err != nil {
		return err
	}
	for _, term := range expr {
		wireID := term.WireID()
		if term.IsConstant() {
			wireID = 0
		}
		if _, err := fmt.Fprintf(writer, "(%s %d) ", resolver.CoeffToString(term.CoeffID()), wireID); err != nil {
			return err
		}
	}
	_, err := fmt.Fprint(writer, "]")
	return err
}

func LoadCircuitMetadata(dir string) (*CircuitMetadataJSON, error) {
	path := filepath.Join(dir, "circuit_metadata.json")
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	metadata, err := LoadCircuitMetadataBytes(data, path)
	if err != nil {
		return nil, err
	}
	provenancePath := filepath.Join(dir, "setup_provenance.json")
	provenance, err := os.ReadFile(provenancePath)
	if err != nil {
		return nil, fmt.Errorf("read setup provenance: %w", err)
	}
	sum := sha256.Sum256(provenance)
	actual := hex.EncodeToString(sum[:])
	if actual != metadata.SetupProvenanceSHA256Hex {
		return nil, fmt.Errorf(
			"setup provenance hash mismatch: metadata says %s, got %s",
			metadata.SetupProvenanceSHA256Hex,
			actual,
		)
	}
	return metadata, nil
}

func LoadCircuitMetadataBytes(data []byte, source string) (*CircuitMetadataJSON, error) {
	metadata, err := DecodeCanonicalCircuitMetadataJSON(data)
	if err != nil {
		return nil, fmt.Errorf("decode %s: %w", source, err)
	}
	return &metadata, nil
}

func ValidateCircuitMetadataForCircuit(metadata *CircuitMetadataJSON, expectedCircuit string, ccs constraint.ConstraintSystem) error {
	if metadata == nil {
		return fmt.Errorf("missing circuit metadata")
	}
	if ccs == nil {
		return fmt.Errorf("missing compiled constraint system")
	}
	if metadata.Schema != CircuitMetadataSchema {
		return fmt.Errorf("unsupported circuit metadata schema %q", metadata.Schema)
	}
	if metadata.Curve != "bls12-377" {
		return fmt.Errorf("artifact curve %q does not match expected bls12-377", metadata.Curve)
	}
	if metadata.Circuit != expectedCircuit {
		return fmt.Errorf("artifact circuit %q does not match expected %s", metadata.Circuit, expectedCircuit)
	}
	if metadata.NbConstraints <= 0 || metadata.NbPublic <= 0 || metadata.NbSecret <= 0 {
		return fmt.Errorf("artifact metadata is missing circuit shape; rerun `gnarkctl setup`")
	}
	if metadata.ProvingKeySize <= 0 || metadata.VerifyingKeySize <= 0 {
		return fmt.Errorf("artifact metadata is missing key sizes; rerun `gnarkctl setup`")
	}
	if !isLowerSHA256(metadata.SetupProvenanceSHA256Hex) {
		return fmt.Errorf(
			"artifact metadata is missing a canonical setup-provenance hash; rerun `gnarkctl setup`",
		)
	}
	gotConstraints := ccs.GetNbConstraints()
	if metadata.NbConstraints != gotConstraints {
		return fmt.Errorf(
			"artifact mismatch: compiled circuit has %d constraints but metadata says %d; rerun `gnarkctl setup`",
			gotConstraints,
			metadata.NbConstraints,
		)
	}
	gotPublic := ccs.GetNbPublicVariables()
	if metadata.NbPublic != gotPublic {
		return fmt.Errorf(
			"artifact mismatch: compiled circuit has %d public variables but metadata says %d; rerun `gnarkctl setup`",
			gotPublic,
			metadata.NbPublic,
		)
	}
	gotSecret := ccs.GetNbSecretVariables()
	if metadata.NbSecret != gotSecret {
		return fmt.Errorf(
			"artifact mismatch: compiled circuit has %d secret variables but metadata says %d; rerun `gnarkctl setup`",
			gotSecret,
			metadata.NbSecret,
		)
	}
	sr1csHash, err := ConstraintSystemSHA256Hex(ccs)
	if err != nil {
		return fmt.Errorf("hash compiled constraint system: %w", err)
	}
	if metadata.SR1CSSHA256Hex != sr1csHash {
		return fmt.Errorf(
			"artifact mismatch: compiled SR1CS hash %s but metadata binds %s; rerun `gnarkctl setup`",
			sr1csHash,
			metadata.SR1CSSHA256Hex,
		)
	}
	return nil
}

func ValidateProvingKeyBytes(metadata *CircuitMetadataJSON, data []byte) error {
	if metadata == nil {
		return fmt.Errorf("missing circuit metadata")
	}
	if int64(len(data)) != metadata.ProvingKeySize {
		return fmt.Errorf(
			"proving key size mismatch: metadata says %d bytes, got %d",
			metadata.ProvingKeySize,
			len(data),
		)
	}
	sum := sha256.Sum256(data)
	actual := hex.EncodeToString(sum[:])
	if actual != metadata.ProvingKeySHA256Hex {
		return fmt.Errorf(
			"proving key hash mismatch: metadata says %s, got %s",
			metadata.ProvingKeySHA256Hex,
			actual,
		)
	}
	return nil
}

func SHA256HexFile(path string) (string, error) {
	bytes, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(bytes)
	return hex.EncodeToString(sum[:]), nil
}
