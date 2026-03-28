package prototype

import (
	"encoding/json"
	"fmt"
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
	Curve            string  `json:"curve"`
	Circuit          string  `json:"circuit"`
	CompileMS        float64 `json:"compile_ms"`
	SetupMS          float64 `json:"setup_ms"`
	ProvingKeySize   int64   `json:"proving_key_size_bytes"`
	VerifyingKeySize int64   `json:"verifying_key_size_bytes"`
	NbConstraints    int     `json:"nb_constraints"`
	NbPublic         int     `json:"nb_public_variables"`
	NbSecret         int     `json:"nb_secret_variables"`
}

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

func LoadCircuitMetadata(dir string) (*CircuitMetadataJSON, error) {
	path := filepath.Join(dir, "circuit_metadata.json")
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var metadata CircuitMetadataJSON
	if err := json.Unmarshal(data, &metadata); err != nil {
		return nil, fmt.Errorf("decode %s: %w", path, err)
	}
	return &metadata, nil
}

func ValidateCircuitMetadata(metadata *CircuitMetadataJSON, ccs constraint.ConstraintSystem) error {
	if metadata == nil {
		return fmt.Errorf("missing circuit metadata")
	}
	if metadata.Curve != "bls12-377" {
		return fmt.Errorf("artifact curve %q does not match expected bls12-377", metadata.Curve)
	}
	if metadata.Circuit != "spend" {
		return fmt.Errorf("artifact circuit %q does not match expected spend", metadata.Circuit)
	}
	if metadata.NbConstraints <= 0 || metadata.NbPublic <= 0 || metadata.NbSecret <= 0 {
		return fmt.Errorf("artifact metadata is missing circuit shape; rerun spendsetup")
	}
	gotConstraints := ccs.GetNbConstraints()
	if metadata.NbConstraints != gotConstraints {
		return fmt.Errorf(
			"artifact mismatch: compiled circuit has %d constraints but metadata says %d; rerun spendsetup",
			gotConstraints,
			metadata.NbConstraints,
		)
	}
	gotPublic := ccs.GetNbPublicVariables()
	if metadata.NbPublic != gotPublic {
		return fmt.Errorf(
			"artifact mismatch: compiled circuit has %d public variables but metadata says %d; rerun spendsetup",
			gotPublic,
			metadata.NbPublic,
		)
	}
	gotSecret := ccs.GetNbSecretVariables()
	if metadata.NbSecret != gotSecret {
		return fmt.Errorf(
			"artifact mismatch: compiled circuit has %d secret variables but metadata says %d; rerun spendsetup",
			gotSecret,
			metadata.NbSecret,
		)
	}
	return nil
}
