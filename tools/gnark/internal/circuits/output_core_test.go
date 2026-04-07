package circuits

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type counterpartyLeafHashCircuit struct {
	DiversifiedGenX  frontend.Variable
	DiversifiedGenY  frontend.Variable
	TransmissionKeyX frontend.Variable
	TransmissionKeyY frontend.Variable
	AssetID          frontend.Variable
	D                frontend.Variable
	TxBlindingNonce  frontend.Variable

	Expected frontend.Variable `gnark:",public"`
}

func (c *counterpartyLeafHashCircuit) Define(api frontend.API) error {
	counterpartyLeafHash, err := CounterpartyLeafHash(
		api,
		gnarkte.Point{X: c.DiversifiedGenX, Y: c.DiversifiedGenY},
		gnarkte.Point{X: c.TransmissionKeyX, Y: c.TransmissionKeyY},
		c.AssetID,
		c.D,
		c.TxBlindingNonce,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(counterpartyLeafHash, c.Expected)
	return nil
}

type negativeBalanceCommitmentCircuit struct {
	ClaimedX        frontend.Variable
	ClaimedY        frontend.Variable
	NoteAmount      frontend.Variable
	NoteAssetID     frontend.Variable
	BalanceBlinding frontend.Variable
}

func (c *negativeBalanceCommitmentCircuit) Define(api frontend.API) error {
	return AssertNegativeBalanceCommitment(
		api,
		gnarkte.Point{X: c.ClaimedX, Y: c.ClaimedY},
		c.NoteAmount,
		c.NoteAssetID,
		c.BalanceBlinding,
	)
}

func TestCounterpartyLeafHashMatchesFixtureBinding(t *testing.T) {
	fixture, err := primitives.LoadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	assignment := &counterpartyLeafHashCircuit{
		DiversifiedGenX:  fixture.Private.UserDiversifiedGeneratorAffine.X,
		DiversifiedGenY:  fixture.Private.UserDiversifiedGeneratorAffine.Y,
		TransmissionKeyX: fixture.Private.UserTransmissionKeyAffine.X,
		TransmissionKeyY: fixture.Private.UserTransmissionKeyAffine.Y,
		AssetID:          fixture.Private.NoteAssetID,
		D:                fixture.Private.UserDDecimal,
		TxBlindingNonce:  fixture.Private.TxBlindingNonce,
		Expected:         fixture.Public.SenderLeafHash,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&counterpartyLeafHashCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestNegativeBalanceCommitmentCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &negativeBalanceCommitmentCircuit{})
	if err != nil {
		t.Fatalf("compile negative balance commitment circuit: %v", err)
	}
}
