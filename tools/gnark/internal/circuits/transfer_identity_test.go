package circuits_test

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
)

type transferIdentityOwnershipAliasProbe struct {
	DivGen   circuits.Point2D
	IVK0     frontend.Variable
	IVK1     frontend.Variable
	NK0      frontend.Variable
	NK1      frontend.Variable
	Blinding frontend.Variable
	Amount   frontend.Variable
	AssetID  frontend.Variable
	Position frontend.Variable
}

func defineTransferIdentityOwnershipAlias(
	api frontend.API,
	probe *transferIdentityOwnershipAliasProbe,
	enforceNonIdentity bool,
) error {
	divGen := gnarkte.Point{X: probe.DivGen.X, Y: probe.DivGen.Y}
	if enforceNonIdentity {
		circuits.AssertDecafNonIdentity(api, divGen)
	}

	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return err
	}
	transmission0 := compliance.ScalarMulLE(api, curve, divGen, probe.IVK0, 251)
	transmission1 := compliance.ScalarMulLE(api, curve, divGen, probe.IVK1, 251)
	divGenFq, err := decafgnark.CompressToField(api, divGen)
	if err != nil {
		return err
	}
	transmission0Fq, err := decafgnark.CompressToField(api, transmission0)
	if err != nil {
		return err
	}
	transmission1Fq, err := decafgnark.CompressToField(api, transmission1)
	if err != nil {
		return err
	}

	commitment0, err := circuits.NoteCommitmentWithCompressedDivGen(
		api,
		probe.Blinding,
		probe.Amount,
		probe.AssetID,
		divGenFq,
		transmission0Fq,
		0,
	)
	if err != nil {
		return err
	}
	commitment1, err := circuits.NoteCommitmentWithCompressedDivGen(
		api,
		probe.Blinding,
		probe.Amount,
		probe.AssetID,
		divGenFq,
		transmission1Fq,
		0,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(commitment0, commitment1)

	nullifier0, err := circuits.Nullifier(api, probe.NK0, commitment0, probe.Position)
	if err != nil {
		return err
	}
	nullifier1, err := circuits.Nullifier(api, probe.NK1, commitment1, probe.Position)
	if err != nil {
		return err
	}
	api.AssertIsDifferent(probe.NK0, probe.NK1)
	api.AssertIsDifferent(nullifier0, nullifier1)
	return nil
}

func (p *transferIdentityOwnershipAliasProbe) Define(api frontend.API) error {
	return defineTransferIdentityOwnershipAlias(api, p, false)
}

type hardenedTransferIdentityOwnershipAliasProbe struct {
	transferIdentityOwnershipAliasProbe
}

func (p *hardenedTransferIdentityOwnershipAliasProbe) Define(api frontend.API) error {
	return defineTransferIdentityOwnershipAlias(
		api,
		&p.transferIdentityOwnershipAliasProbe,
		true,
	)
}

func transferIdentityOwnershipAliasAssignment() transferIdentityOwnershipAliasProbe {
	return transferIdentityOwnershipAliasProbe{
		DivGen:   circuits.Point2D{X: 0, Y: 1},
		IVK0:     1,
		IVK1:     2,
		NK0:      3,
		NK1:      4,
		Blinding: 5,
		Amount:   6,
		AssetID:  7,
		Position: 9,
	}
}

func TestTransferUnguardedIdentityDiversifierAliasesOwnership(
	t *testing.T,
) {
	assignment := transferIdentityOwnershipAliasAssignment()
	if err := test.IsSolved(
		&transferIdentityOwnershipAliasProbe{},
		&assignment,
		ecc.BLS12_377.ScalarField(),
	); err != nil {
		t.Fatalf(
			"unguarded identity diversifier should admit one commitment under distinct nullifier keys: %v",
			err,
		)
	}

	hardened := hardenedTransferIdentityOwnershipAliasProbe{
		transferIdentityOwnershipAliasProbe: assignment,
	}
	if err := test.IsSolved(
		&hardenedTransferIdentityOwnershipAliasProbe{},
		&hardened,
		ecc.BLS12_377.ScalarField(),
	); err == nil {
		t.Fatal("the hardened relation must reject the identity-diversifier ownership alias")
	}
}

func TestTransferCircuitRejectsIdentityAuthorizationAndDiversifiedGenerators(
	t *testing.T,
) {
	_, canonical := loadTransferAssignment(t)
	if err := test.IsSolved(
		circuits.NewTransferCircuit(),
		canonical,
		ecc.BLS12_377.ScalarField(),
	); err != nil {
		t.Fatalf("canonical Transfer witness must satisfy non-identity guards: %v", err)
	}

	// IVK and transmission mutations also disturb derivation and commitment rows.
	tests := []struct {
		name   string
		mutate func(*circuits.TransferCircuit)
	}{
		{
			name: "authorization_key",
			mutate: func(c *circuits.TransferCircuit) {
				c.Auth.AK = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "sender_diversified_generator",
			mutate: func(c *circuits.TransferCircuit) {
				c.Sender.DivGen = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "receiver_diversified_generator",
			mutate: func(c *circuits.TransferCircuit) {
				c.ReceiverOutput.Recipient.DivGen = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "incoming_viewing_key_zero",
			mutate: func(c *circuits.TransferCircuit) {
				c.Auth.IVKReduced = 0
			},
		},
		{
			name: "sender_transmission_key",
			mutate: func(c *circuits.TransferCircuit) {
				c.Sender.Transmission = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "receiver_transmission_key",
			mutate: func(c *circuits.TransferCircuit) {
				c.ReceiverOutput.Recipient.Transmission = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "sender_core_ephemeral_key",
			mutate: func(c *circuits.TransferCircuit) {
				c.Compliance.SenderCore.Epk = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "sender_extension_ephemeral_key",
			mutate: func(c *circuits.TransferCircuit) {
				c.Compliance.SenderExt.Epk = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "output_core_ephemeral_key",
			mutate: func(c *circuits.TransferCircuit) {
				c.Compliance.OutputCore.Epk = circuits.Point2D{X: 0, Y: 1}
			},
		},
		{
			name: "output_extension_ephemeral_key",
			mutate: func(c *circuits.TransferCircuit) {
				c.Compliance.OutputExt.Epk = circuits.Point2D{X: 0, Y: 1}
			},
		},
	}
	for _, testCase := range tests {
		t.Run(testCase.name, func(t *testing.T) {
			_, assignment := loadTransferAssignment(t)
			testCase.mutate(assignment)
			if err := test.IsSolved(
				circuits.NewTransferCircuit(),
				assignment,
				ecc.BLS12_377.ScalarField(),
			); err == nil {
				t.Fatalf("Transfer must reject identity %s", testCase.name)
			}
		})
	}
}
