package circuits

import (
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

// NoteSeizureCircuit proves that authority-approved public facts consume one
// real SCT note with its canonical compliance nullifier.
type NoteSeizureCircuit struct {
	wiringTrace *WiringTranscript

	ClaimedStatementHash frontend.Variable `gnark:",public"`

	Anchor                  frontend.Variable
	NoteCommitment          frontend.Variable
	Nullifier               frontend.Variable
	HistoryRequired         frontend.Variable
	RecentPositionFloor     frontend.Variable
	AddressDivGen           frontend.Variable
	AddressTransmissionKey  frontend.Variable
	AssetID                 frontend.Variable
	Amount                  frontend.Variable
	RnkCommitment           frontend.Variable
	AuthorizationCommitment frontend.Variable

	Recovery RecoveryCapsuleFields

	NoteBlinding frontend.Variable
	Position     frontend.Variable
	Path         [StateCommitmentDepth][3]frontend.Variable
	RNK          frontend.Variable
}

func NewNoteSeizureCircuit() *NoteSeizureCircuit {
	return &NoteSeizureCircuit{}
}

func (c *NoteSeizureCircuit) Define(api frontend.API) error {
	c.bindWiringTrace(api)
	c.bindSemantic("statement_hash", c.ClaimedStatementHash)
	c.traceWiring("note_seizure")

	api.AssertIsBoolean(c.HistoryRequired)
	api.ToBinary(c.Amount, 128)

	if err := VerifyRecoveryCapsulePlaintext(
		api,
		c.Amount,
		c.NoteBlinding,
		c.Recovery,
	); err != nil {
		return err
	}

	computedNoteCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		c.NoteBlinding,
		c.Amount,
		c.AssetID,
		c.AddressDivGen,
		c.AddressTransmissionKey,
		c.Recovery.Commitment,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(computedNoteCommitment, c.NoteCommitment)

	rnkCommitment, err := ComplianceNullifierKeyCommitment(api, c.RNK)
	if err != nil {
		return err
	}
	api.AssertIsEqual(rnkCommitment, c.RnkCommitment)

	computedNullifier, err := Nullifier(api, c.RNK, c.NoteCommitment, c.Position)
	if err != nil {
		return err
	}
	api.AssertIsEqual(computedNullifier, c.Nullifier)

	path := make([][3]frontend.Variable, StateCommitmentDepth)
	copy(path, c.Path[:])
	computedAnchor, err := VerifyStateCommitmentPath(api, c.NoteCommitment, c.Position, path)
	if err != nil {
		return err
	}
	api.AssertIsEqual(computedAnchor, c.Anchor)

	computedHistoryRequired := historyRequired(api, c.Position, c.RecentPositionFloor, 0)
	api.AssertIsEqual(computedHistoryRequired, c.HistoryRequired)

	epkFq, err := decafgnark.CompressToField(
		api,
		gnarkte.Point{X: c.Recovery.EPK.X, Y: c.Recovery.EPK.Y},
	)
	if err != nil {
		return err
	}
	statementHash, err := NoteSeizureStatementHash(api, []frontend.Variable{
		c.Anchor,
		c.NoteCommitment,
		c.Nullifier,
		c.HistoryRequired,
		c.RecentPositionFloor,
		c.AddressDivGen,
		c.AddressTransmissionKey,
		c.AssetID,
		c.Amount,
		c.Recovery.Commitment,
		epkFq,
		c.Recovery.C2,
		c.Recovery.Salt,
		c.Recovery.KeyConfirmation,
		c.Recovery.EncryptedAmount,
		c.Recovery.EncryptedNoteBlinding,
		c.Recovery.Seed,
		c.RnkCommitment,
		c.AuthorizationCommitment,
	})
	if err != nil {
		return err
	}
	api.AssertIsEqual(statementHash, c.ClaimedStatementHash)
	return nil
}
