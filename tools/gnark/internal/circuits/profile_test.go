package circuits

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/generated"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type noteCommitmentProfileCircuit struct {
	NoteBlinding     frontend.Variable
	NoteAmount       frontend.Variable
	NoteAssetID      frontend.Variable
	DiversifiedGenX  frontend.Variable
	DiversifiedGenY  frontend.Variable
	TransmissionKeyS frontend.Variable
	ClueKey          frontend.Variable
}

func (c *noteCommitmentProfileCircuit) Define(api frontend.API) error {
	_, err := NoteCommitment(
		api,
		c.NoteBlinding,
		c.NoteAmount,
		c.NoteAssetID,
		gnarkte.Point{X: c.DiversifiedGenX, Y: c.DiversifiedGenY},
		c.TransmissionKeyS,
		c.ClueKey,
	)
	return err
}

type complianceLeafProfileCircuit struct {
	DivGenX frontend.Variable
	DivGenY frontend.Variable
	TransX  frontend.Variable
	TransY  frontend.Variable
	AssetID frontend.Variable
	D       frontend.Variable
}

func (c *complianceLeafProfileCircuit) Define(api frontend.API) error {
	_, err := ComplianceLeafCommitment(
		api,
		gnarkte.Point{X: c.DivGenX, Y: c.DivGenY},
		gnarkte.Point{X: c.TransX, Y: c.TransY},
		c.AssetID,
		c.D,
	)
	return err
}

type thresholdProfileCircuit struct {
	Amount    frontend.Variable
	Threshold frontend.Variable
	IsFlagged frontend.Variable
}

func (c *thresholdProfileCircuit) Define(api frontend.API) error {
	compliance.VerifyThresholdFlagSimple(api, c.Amount, c.Threshold, c.IsFlagged)
	return nil
}

type quadPathProfileCircuit struct {
	LeafHash frontend.Variable
	Path     [compliance.ComplianceQuadTreeDepth][3]frontend.Variable
	Position frontend.Variable
}

func (c *quadPathProfileCircuit) Define(api frontend.API) error {
	_, err := compliance.VerifyQuadPath(api, c.LeafHash, c.Path, c.Position)
	return err
}

type pointCompressionProfileCircuit struct {
	X frontend.Variable
	Y frontend.Variable
}

func (c *pointCompressionProfileCircuit) Define(api frontend.API) error {
	_, err := primitives.Decaf377CompressToField(api, gnarkte.Point{X: c.X, Y: c.Y})
	return err
}

type dleqProfileCircuit struct {
	R            frontend.Variable
	AckX         frontend.Variable
	AckY         frontend.Variable
	SPointX      frontend.Variable
	SPointY      frontend.Variable
	EpkX         frontend.Variable
	EpkY         frontend.Variable
	MetadataHash frontend.Variable
	PublishedC   frontend.Variable
	PublishedS   frontend.Variable
	IsRegulated  frontend.Variable
}

func (c *dleqProfileCircuit) Define(api frontend.API) error {
	return compliance.VerifyDLEQ(
		api,
		c.R,
		gnarkte.Point{X: c.AckX, Y: c.AckY},
		gnarkte.Point{X: c.SPointX, Y: c.SPointY},
		gnarkte.Point{X: c.EpkX, Y: c.EpkY},
		c.MetadataHash,
		c.PublishedC,
		c.PublishedS,
		c.IsRegulated,
	)
}

type spendSharedSecretsProfileCircuit struct {
	ESK       frontend.Variable
	AckX      frontend.Variable
	AckY      frontend.Variable
	DKPubX    frontend.Variable
	DKPubY    frontend.Variable
	IsFlagged frontend.Variable
	EpkX      frontend.Variable
	EpkY      frontend.Variable
}

func (c *spendSharedSecretsProfileCircuit) Define(api frontend.API) error {
	_, _, _, err := compliance.DeriveSharedSecretsSpend(
		api,
		c.ESK,
		gnarkte.Point{X: c.AckX, Y: c.AckY},
		gnarkte.Point{X: c.DKPubX, Y: c.DKPubY},
		c.IsFlagged,
		gnarkte.Point{X: c.EpkX, Y: c.EpkY},
	)
	return err
}

type outputSharedSecretsProfileCircuit struct {
	R1        frontend.Variable
	R2        frontend.Variable
	R3        frontend.Variable
	AckRX     frontend.Variable
	AckRY     frontend.Variable
	AckSX     frontend.Variable
	AckSY     frontend.Variable
	DKPubX    frontend.Variable
	DKPubY    frontend.Variable
	IsFlagged frontend.Variable
	Epk1X     frontend.Variable
	Epk1Y     frontend.Variable
	Epk2X     frontend.Variable
	Epk2Y     frontend.Variable
	Epk3X     frontend.Variable
	Epk3Y     frontend.Variable
}

func (c *outputSharedSecretsProfileCircuit) Define(api frontend.API) error {
	_, _, _, _, _, _, _, err := compliance.DeriveSharedSecretsOutput(
		api,
		c.R1,
		c.R2,
		c.R3,
		gnarkte.Point{X: c.AckRX, Y: c.AckRY},
		gnarkte.Point{X: c.AckSX, Y: c.AckSY},
		gnarkte.Point{X: c.DKPubX, Y: c.DKPubY},
		c.IsFlagged,
		gnarkte.Point{X: c.Epk1X, Y: c.Epk1Y},
		gnarkte.Point{X: c.Epk2X, Y: c.Epk2Y},
		gnarkte.Point{X: c.Epk3X, Y: c.Epk3Y},
	)
	return err
}

func compileConstraintCount(t *testing.T, name string, circuit frontend.Circuit) {
	t.Helper()
	ccs, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, circuit)
	if err != nil {
		t.Fatalf("compile %s: %v", name, err)
	}
	t.Logf("%s: %d constraints", name, ccs.GetNbConstraints())
}

func TestConstraintProfiles(t *testing.T) {
	compileConstraintCount(t, "note commitment", &noteCommitmentProfileCircuit{})
	compileConstraintCount(t, "balance commitment", &balanceCommitmentCircuit{})
	compileConstraintCount(t, "randomized verification key", &randomizedVerificationKeyCircuit{})
	compileConstraintCount(t, "threshold comparator", &thresholdProfileCircuit{})
	compileConstraintCount(t, "point compression", &pointCompressionProfileCircuit{})
	compileConstraintCount(t, "compliance leaf commitment", &complianceLeafProfileCircuit{})
	compileConstraintCount(t, "quad path", &quadPathProfileCircuit{})
	compileConstraintCount(t, "dleq", &dleqProfileCircuit{})
	compileConstraintCount(t, "spend shared secrets", &spendSharedSecretsProfileCircuit{})
	compileConstraintCount(t, "output shared secrets", &outputSharedSecretsProfileCircuit{})
	for _, family := range generated.TransferFamilies {
		compileConstraintCount(
			t,
			family.Label+" full circuit",
			NewTransferCircuit(family.NIn, family.NOut),
		)
	}
}
