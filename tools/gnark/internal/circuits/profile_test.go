package circuits

import (
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/mizufinance/penumbra/tools/gnark/internal/compliance"
	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
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
	_, err := decafgnark.CompressToField(api, gnarkte.Point{X: c.X, Y: c.Y})
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

type transferSaltProfileCircuit struct {
	TransferNonceRoot frontend.Variable
}

func (c *transferSaltProfileCircuit) Define(api frontend.API) error {
	_, err := compliance.DeriveTransferSalt(api, c.TransferNonceRoot, compliance.TransferSenderExtSaltLabel)
	return err
}

type transferMetadataProfileCircuit struct {
	PolicyIDHash    frontend.Variable
	ResourceHash    frontend.Variable
	PermissionHash  frontend.Variable
	TargetTimestamp frontend.Variable
	Salt            frontend.Variable
}

func (c *transferMetadataProfileCircuit) Define(api frontend.API) error {
	_, err := compliance.ComputeMetadataHash(
		api,
		c.PolicyIDHash,
		c.ResourceHash,
		c.PermissionHash,
		2,
		c.TargetTimestamp,
		c.Salt,
	)
	return err
}

type transferDetectionProfileCircuit struct {
	IsRegulated     frontend.Variable
	IsFlagged       frontend.Variable
	SharedSecretX   frontend.Variable
	SharedSecretY   frontend.Variable
	SenderCoreEPKFq frontend.Variable
	DetectionSalt   frontend.Variable
	AssetID         frontend.Variable
	Ciphertext0     frontend.Variable
	Ciphertext1     frontend.Variable
}

func (c *transferDetectionProfileCircuit) Define(api frontend.API) error {
	return compliance.VerifyPoseidonEncryptionTransferDetection(
		api,
		c.IsRegulated,
		c.IsFlagged,
		gnarkte.Point{X: c.SharedSecretX, Y: c.SharedSecretY},
		c.SenderCoreEPKFq,
		c.DetectionSalt,
		c.AssetID,
		[compliance.TransferDetectionFQCount]frontend.Variable{c.Ciphertext0, c.Ciphertext1},
	)
}

type transferAmountCiphertextProfileCircuit struct {
	IsRegulated   frontend.Variable
	SharedSecretX frontend.Variable
	SharedSecretY frontend.Variable
	C2            frontend.Variable
	Amount        frontend.Variable
	Ciphertext0   frontend.Variable
}

func (c *transferAmountCiphertextProfileCircuit) Define(api frontend.API) error {
	return compliance.VerifyPoseidonEncryptionTransferAmount(
		api,
		c.IsRegulated,
		gnarkte.Point{X: c.SharedSecretX, Y: c.SharedSecretY},
		c.C2,
		c.Amount,
		[compliance.TransferCoreCiphertextFQCount]frontend.Variable{c.Ciphertext0},
	)
}

type transferAddressCiphertextProfileCircuit struct {
	IsRegulated            frontend.Variable
	SharedSecretX          frontend.Variable
	SharedSecretY          frontend.Variable
	C2                     frontend.Variable
	DiversifiedGeneratorFq frontend.Variable
	TransmissionKeyFq      frontend.Variable
	Ciphertext0            frontend.Variable
	Ciphertext1            frontend.Variable
	Ciphertext2            frontend.Variable
}

func (c *transferAddressCiphertextProfileCircuit) Define(api frontend.API) error {
	return compliance.VerifyPoseidonEncryptionTransferAddress(
		api,
		c.IsRegulated,
		gnarkte.Point{X: c.SharedSecretX, Y: c.SharedSecretY},
		c.C2,
		c.DiversifiedGeneratorFq,
		c.TransmissionKeyFq,
		[compliance.TransferExtCiphertextFQCount]frontend.Variable{
			c.Ciphertext0,
			c.Ciphertext1,
			c.Ciphertext2,
		},
	)
}

type transferSharedProfileCircuit struct {
	TransferCircuit
}

func newTransferSharedProfileCircuit() *transferSharedProfileCircuit {
	return &transferSharedProfileCircuit{TransferCircuit: *NewTransferCircuit()}
}

func (c *transferSharedProfileCircuit) Define(api frontend.API) error {
	_, err := c.TransferCircuit.verifySharedTransferContext(api)
	return err
}

type transferSharedSpendProfileCircuit struct {
	TransferCircuit
}

func newTransferSharedSpendProfileCircuit() *transferSharedSpendProfileCircuit {
	return &transferSharedSpendProfileCircuit{TransferCircuit: *NewTransferCircuit()}
}

func (c *transferSharedSpendProfileCircuit) Define(api frontend.API) error {
	shared, err := c.TransferCircuit.verifySharedTransferContext(api)
	if err != nil {
		return err
	}
	statementData := c.TransferCircuit.newTransferStatementData()
	return c.TransferCircuit.verifyTransferSpend(api, &shared, &statementData, &c.Spends[0], 0)
}

type transferSharedReceiverOutputProfileCircuit struct {
	TransferCircuit
}

func newTransferSharedReceiverOutputProfileCircuit() *transferSharedReceiverOutputProfileCircuit {
	return &transferSharedReceiverOutputProfileCircuit{TransferCircuit: *NewTransferCircuit()}
}

func (c *transferSharedReceiverOutputProfileCircuit) Define(api frontend.API) error {
	shared, err := c.TransferCircuit.verifySharedTransferContext(api)
	if err != nil {
		return err
	}
	statementData := c.TransferCircuit.newTransferStatementData()
	return c.TransferCircuit.verifyTransferOutput(api, &shared, &statementData, &c.Outputs[0], 0)
}

type transferSharedReceiverComplianceProfileCircuit struct {
	TransferCircuit
}

func newTransferSharedReceiverComplianceProfileCircuit() *transferSharedReceiverComplianceProfileCircuit {
	return &transferSharedReceiverComplianceProfileCircuit{TransferCircuit: *NewTransferCircuit()}
}

func (c *transferSharedReceiverComplianceProfileCircuit) Define(api frontend.API) error {
	shared, err := c.TransferCircuit.verifySharedTransferContext(api)
	if err != nil {
		return err
	}
	statementData := c.TransferCircuit.newTransferStatementData()
	if err := c.TransferCircuit.verifyTransferOutput(api, &shared, &statementData, &c.Outputs[0], 0); err != nil {
		return err
	}
	return c.TransferCircuit.verifyTransferComplianceCiphertexts(api, &shared, &statementData)
}

func compileConstraintCount(t *testing.T, name string, circuit frontend.Circuit) int {
	t.Helper()
	ccs, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, circuit)
	if err != nil {
		t.Fatalf("compile %s: %v", name, err)
	}
	count := ccs.GetNbConstraints()
	t.Logf("%s: %d constraints", name, count)
	return count
}

func TestConstraintProfiles(t *testing.T) {
	compileConstraintCount(t, "note commitment", &noteCommitmentProfileCircuit{})
	compileConstraintCount(t, "threshold comparator", &thresholdProfileCircuit{})
	compileConstraintCount(t, "point compression", &pointCompressionProfileCircuit{})
	compileConstraintCount(t, "compliance leaf commitment", &complianceLeafProfileCircuit{})
	compileConstraintCount(t, "quad path", &quadPathProfileCircuit{})
	compileConstraintCount(t, "dleq", &dleqProfileCircuit{})
	compileConstraintCount(t, "spend shared secrets", &spendSharedSecretsProfileCircuit{})
	transferSalt := compileConstraintCount(t, "transfer salt derivation", &transferSaltProfileCircuit{})
	transferMetadata := compileConstraintCount(t, "transfer metadata hash", &transferMetadataProfileCircuit{})
	transferDetection := compileConstraintCount(t, "transfer detection ciphertext", &transferDetectionProfileCircuit{})
	transferAmount := compileConstraintCount(t, "transfer amount ciphertext", &transferAmountCiphertextProfileCircuit{})
	transferAddress := compileConstraintCount(t, "transfer address ciphertext", &transferAddressCiphertextProfileCircuit{})

	transferShared := compileConstraintCount(
		t,
		"transfer shared context",
		newTransferSharedProfileCircuit(),
	)
	transferSharedSpend := compileConstraintCount(
		t,
		"transfer shared + one spend leg",
		newTransferSharedSpendProfileCircuit(),
	)
	transferSharedReceiver := compileConstraintCount(
		t,
		"transfer shared + receiver output leg",
		newTransferSharedReceiverOutputProfileCircuit(),
	)
	transferSharedReceiverCompliance := compileConstraintCount(
		t,
		"transfer shared + receiver output leg + grouped compliance block",
		newTransferSharedReceiverComplianceProfileCircuit(),
	)

	fullCounts := make(map[string]int, len(generated.TransferFamilies))
	for _, family := range generated.TransferFamilies {
		fullCounts[family.Label] = compileConstraintCount(
			t,
			family.Label+" full circuit",
			NewTransferCircuit(),
		)
	}

	baseShared := transferShared
	baseSpendLeg := transferSharedSpend - transferShared
	baseReceiverLeg := transferSharedReceiver - transferShared
	baseCompliance := transferSharedReceiverCompliance - transferSharedReceiver
	t.Logf("  transfer total: %d", fullCounts["transfer"])
	t.Logf("transfer inferred breakdown:")
	t.Logf("  shared context: %d", baseShared)
	t.Logf("  first spend leg: %d", baseSpendLeg)
	t.Logf("  receiver output leg: %d", baseReceiverLeg)
	t.Logf("  grouped compliance block: %d", baseCompliance)

	counterpartyTierEstimate := transferSalt + 9033 + transferAddress + transferMetadata + 17835 + 1046
	t.Logf("transfer compliance sub-gadgets:")
	t.Logf("  transfer detection ciphertext: %d", transferDetection)
	t.Logf("  transfer amount ciphertext: %d", transferAmount)
	t.Logf("  transfer address ciphertext: %d", transferAddress)
	t.Logf("  transfer salt derivation: %d", transferSalt)
	t.Logf("  transfer metadata hash: %d", transferMetadata)
	t.Logf("estimated cost of one distinct counterparty ciphertext tier:")
	t.Logf("  salt + shared-secret + address ciphertext + metadata + DLEQ + EPK compression: %d", counterpartyTierEstimate)
	t.Logf("  as share of transfer: %.2f%%", 100*float64(counterpartyTierEstimate)/float64(fullCounts["transfer"]))
}
