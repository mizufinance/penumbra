package circuits

import (
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
)

type noteCommitmentProfileCircuit struct {
	NoteBlinding     frontend.Variable
	NoteAmount       frontend.Variable
	NoteAssetID      frontend.Variable
	DiversifiedGenX  frontend.Variable
	DiversifiedGenY  frontend.Variable
	TransmissionKeyS frontend.Variable
}

func (c *noteCommitmentProfileCircuit) Define(api frontend.API) error {
	_, err := NoteCommitment(
		api,
		c.NoteBlinding,
		c.NoteAmount,
		c.NoteAssetID,
		gnarkte.Point{X: c.DiversifiedGenX, Y: c.DiversifiedGenY},
		c.TransmissionKeyS,
		0,
	)
	return err
}

type complianceLeafProfileCircuit struct {
	DivGenX       frontend.Variable
	DivGenY       frontend.Variable
	TransX        frontend.Variable
	TransY        frontend.Variable
	AssetID       frontend.Variable
	CapkX         frontend.Variable
	CapkY         frontend.Variable
	RnkDhPkX      frontend.Variable
	RnkDhPkY      frontend.Variable
	RnkCommitment frontend.Variable
}

func (c *complianceLeafProfileCircuit) Define(api frontend.API) error {
	_, err := ComplianceLeafCommitment(
		api,
		gnarkte.Point{X: c.DivGenX, Y: c.DivGenY},
		gnarkte.Point{X: c.TransX, Y: c.TransY},
		c.AssetID,
		gnarkte.Point{X: c.CapkX, Y: c.CapkY},
		gnarkte.Point{X: c.RnkDhPkX, Y: c.RnkDhPkY},
		c.RnkCommitment,
		1,
	)
	return err
}

type amountComparisonProfileCircuit struct {
	A    frontend.Variable
	B    frontend.Variable
	Less frontend.Variable
}

func (c *amountComparisonProfileCircuit) Define(api frontend.API) error {
	api.AssertIsBoolean(c.Less)
	api.AssertIsEqual(compliance.FieldLessThan(api, c.A, c.B), c.Less)
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

type transferDetectionProfileCircuit struct {
	IsFlagged       frontend.Variable
	SharedSecretX   frontend.Variable
	SharedSecretY   frontend.Variable
	SenderCoreEPKFq frontend.Variable
	DetectionSalt   frontend.Variable
	AssetID         frontend.Variable
	Ciphertext0     frontend.Variable
	Ciphertext1     frontend.Variable
	Ciphertext2     frontend.Variable
	Ciphertext3     frontend.Variable
}

func (c *transferDetectionProfileCircuit) Define(api frontend.API) error {
	return compliance.VerifyPoseidonEncryptionTransferDetection(
		api,
		c.IsFlagged,
		gnarkte.Point{X: c.SharedSecretX, Y: c.SharedSecretY},
		c.SenderCoreEPKFq,
		c.DetectionSalt,
		c.AssetID,
		[compliance.TransferDetectionFQCount]frontend.Variable{
			c.Ciphertext0,
			c.Ciphertext1,
			c.Ciphertext2,
			c.Ciphertext3,
		},
	)
}

type transferAmountCiphertextProfileCircuit struct {
	SharedSecretX frontend.Variable
	SharedSecretY frontend.Variable
	C2            frontend.Variable
	EPKFq         frontend.Variable
	TierSalt      frontend.Variable
	Confirmation  frontend.Variable
	Amount        frontend.Variable
	Ciphertext0   frontend.Variable
}

func (c *transferAmountCiphertextProfileCircuit) Define(api frontend.API) error {
	return compliance.VerifyPoseidonEncryptionTransferAmount(
		api,
		gnarkte.Point{X: c.SharedSecretX, Y: c.SharedSecretY},
		c.C2,
		c.EPKFq,
		c.TierSalt,
		c.Confirmation,
		c.Amount,
		[compliance.TransferCoreCiphertextFQCount]frontend.Variable{c.Ciphertext0},
	)
}

type transferAddressCiphertextProfileCircuit struct {
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
	return c.TransferCircuit.verifyRequiredTransferSpend(
		api,
		&shared,
		&statementData,
		&c.RequiredSpend,
	)
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
	return c.TransferCircuit.verifyTransferReceiverOutput(
		api,
		&shared,
		&statementData,
		&c.ReceiverOutput,
	)
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
	if err := c.TransferCircuit.verifyTransferReceiverOutput(
		api,
		&shared,
		&statementData,
		&c.ReceiverOutput,
	); err != nil {
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
	compileConstraintCount(t, "amount comparator", &amountComparisonProfileCircuit{})
	compileConstraintCount(t, "point compression", &pointCompressionProfileCircuit{})
	compileConstraintCount(t, "compliance leaf commitment", &complianceLeafProfileCircuit{})
	compileConstraintCount(t, "quad path", &quadPathProfileCircuit{})
	compileConstraintCount(t, "spend shared secrets", &spendSharedSecretsProfileCircuit{})
	transferSalt := compileConstraintCount(t, "transfer salt derivation", &transferSaltProfileCircuit{})
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

	t.Logf("transfer compliance sub-gadgets:")
	t.Logf("  transfer detection ciphertext: %d", transferDetection)
	t.Logf("  transfer amount ciphertext: %d", transferAmount)
	t.Logf("  transfer address ciphertext: %d", transferAddress)
	t.Logf("  transfer salt derivation: %d", transferSalt)
}
