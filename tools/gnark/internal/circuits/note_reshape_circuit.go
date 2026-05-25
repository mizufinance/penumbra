package circuits

import (
	"fmt"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

type NoteReshapeOutputCircuitFields struct {
	NoteCommitment frontend.Variable
	Note           NoteFields
}

type NoteReshapeCircuit struct {
	label string
	nIn   int
	nOut  int

	ClaimedStatementHash frontend.Variable `gnark:",public"`

	Anchor                frontend.Variable
	BalanceCommitment     Point2D
	ActionBalanceBlinding frontend.Variable

	Auth    TransferAuthSharedFields
	Spends  []TransferSpendCircuitFields
	Outputs []NoteReshapeOutputCircuitFields
}

func NewConsolidateCircuit(nIn int) *NoteReshapeCircuit {
	return &NoteReshapeCircuit{
		label:   fmt.Sprintf("consolidate%dx1", nIn),
		nIn:     nIn,
		nOut:    1,
		Spends:  make([]TransferSpendCircuitFields, nIn),
		Outputs: make([]NoteReshapeOutputCircuitFields, 1),
	}
}

func NewSplitCircuit(nOut int) *NoteReshapeCircuit {
	return &NoteReshapeCircuit{
		label:   fmt.Sprintf("split1x%d", nOut),
		nIn:     1,
		nOut:    nOut,
		Spends:  make([]TransferSpendCircuitFields, 1),
		Outputs: make([]NoteReshapeOutputCircuitFields, nOut),
	}
}

func (c *NoteReshapeCircuit) Define(api frontend.API) error {
	if c.nIn <= 0 || c.nOut <= 0 {
		return fmt.Errorf("%s circuit shape must be positive, got %dx%d", c.label, c.nIn, c.nOut)
	}
	if len(c.Spends) != c.nIn || len(c.Outputs) != c.nOut {
		return fmt.Errorf("%s circuit shape mismatch: expected %dx%d, got %dx%d", c.label, c.nIn, c.nOut, len(c.Spends), len(c.Outputs))
	}

	sharedAK := gnarkte.Point{X: c.Auth.AK.X, Y: c.Auth.AK.Y}
	claimedBalanceCommitment := gnarkte.Point{X: c.BalanceCommitment.X, Y: c.BalanceCommitment.Y}
	sharedDivGen := gnarkte.Point{X: c.Spends[0].Note.DivGen.X, Y: c.Spends[0].Note.DivGen.Y}
	sharedTransmission := gnarkte.Point{X: c.Spends[0].Note.Transmission.X, Y: c.Spends[0].Note.Transmission.Y}
	sharedAssetID := c.Spends[0].Note.AssetID

	inputAmounts := make([]frontend.Variable, 0, c.nIn)
	outputAmounts := make([]frontend.Variable, 0, c.nOut)
	statementFields := make([]frontend.Variable, 0, NoteReshapeStatementFieldCount(c.nIn, c.nOut))
	outputCommitments := make([]frontend.Variable, 0, c.nOut)
	nullifiersAndRKs := make([]frontend.Variable, 0, 2*c.nIn)

	for i := range c.Spends {
		amount, nullifier, rkCompressed, err := c.verifyNoteReshapeSpend(
			api,
			sharedAK,
			sharedDivGen,
			sharedTransmission,
			sharedAssetID,
			&c.Spends[i],
		)
		if err != nil {
			return err
		}
		inputAmounts = append(inputAmounts, amount)
		nullifiersAndRKs = append(nullifiersAndRKs, nullifier, rkCompressed)
	}

	for i := range c.Outputs {
		amount, commitment, err := c.verifyNoteReshapeOutput(
			api,
			sharedAK,
			sharedDivGen,
			sharedTransmission,
			sharedAssetID,
			&c.Outputs[i],
		)
		if err != nil {
			return err
		}
		outputAmounts = append(outputAmounts, amount)
		outputCommitments = append(outputCommitments, commitment)
	}

	balanceCommitmentPoint, err := computeTransferNetBalanceCommitment(
		api,
		inputAmounts,
		outputAmounts,
		sharedAssetID,
		c.ActionBalanceBlinding,
	)
	if err != nil {
		return err
	}
	decafgnark.AssertEquivalent(api, balanceCommitmentPoint, claimedBalanceCommitment)
	balanceCommitmentFq, err := decafgnark.CompressToField(api, balanceCommitmentPoint)
	if err != nil {
		return err
	}

	statementFields = append(statementFields, c.Anchor)
	statementFields = append(statementFields, outputCommitments...)
	statementFields = append(statementFields, balanceCommitmentFq)
	statementFields = append(statementFields, nullifiersAndRKs...)

	statementHash, err := noteReshapeStatementHash(api, c.label, c.nIn, c.nOut, statementFields)
	if err != nil {
		return err
	}
	api.AssertIsEqual(statementHash, c.ClaimedStatementHash)
	return nil
}

func noteReshapeStatementHash(
	api frontend.API,
	label string,
	nIn, nOut int,
	fields []frontend.Variable,
) (frontend.Variable, error) {
	switch label {
	case fmt.Sprintf("consolidate%dx1", nIn):
		return ConsolidateStatementHashForShape(api, nIn, fields)
	case fmt.Sprintf("split1x%d", nOut):
		return SplitStatementHashForShape(api, nOut, fields)
	default:
		return nil, fmt.Errorf("unsupported note reshape statement hash label %q", label)
	}
}

func (c *NoteReshapeCircuit) verifyNoteReshapeSpend(
	api frontend.API,
	sharedAK gnarkte.Point,
	sharedDivGen gnarkte.Point,
	sharedTransmission gnarkte.Point,
	sharedAssetID frontend.Variable,
	spend *TransferSpendCircuitFields,
) (frontend.Variable, frontend.Variable, frontend.Variable, error) {
	spentDivGen := gnarkte.Point{X: spend.Note.DivGen.X, Y: spend.Note.DivGen.Y}
	spentTransmission := gnarkte.Point{X: spend.Note.Transmission.X, Y: spend.Note.Transmission.Y}
	rkClaimed := gnarkte.Point{X: spend.RK.X, Y: spend.RK.Y}

	spentDivGenFq, err := decafgnark.CompressToField(api, spentDivGen)
	if err != nil {
		return nil, nil, nil, err
	}
	spentCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		spend.Note.Blinding,
		spend.Note.Amount,
		spend.Note.AssetID,
		spentDivGenFq,
		spend.Note.TransmissionKeyS,
		spend.Note.ClueKey,
	)
	if err != nil {
		return nil, nil, nil, err
	}
	api.AssertIsEqual(spentCommitment, spend.StateProof.Commitment)

	nullifier, err := Nullifier(api, c.Auth.NK, spend.StateProof.Commitment, spend.StateProof.Position)
	if err != nil {
		return nil, nil, nil, err
	}
	api.AssertIsEqual(nullifier, spend.Nullifier)

	statePath := make([][3]frontend.Variable, len(spend.StateProof.Path))
	copy(statePath, spend.StateProof.Path[:])
	anchor, err := VerifyStateCommitmentPath(api, spend.StateProof.Commitment, spend.StateProof.Position, statePath)
	if err != nil {
		return nil, nil, nil, err
	}
	api.AssertIsEqual(anchor, c.Anchor)

	computedRK, err := RandomizedVerificationKey(api, sharedAK, spend.AuthRandomizer)
	if err != nil {
		return nil, nil, nil, err
	}
	decafgnark.AssertEquivalent(api, computedRK, rkClaimed)
	rkCompressed, err := decafgnark.CompressToField(api, rkClaimed)
	if err != nil {
		return nil, nil, nil, err
	}

	computedTransmission, err := DiversifiedTransmissionKey(
		api,
		c.Auth.NK,
		sharedAK,
		spentDivGen,
		c.Auth.IVKReduced,
		c.Auth.IVKQuotientA,
	)
	if err != nil {
		return nil, nil, nil, err
	}
	decafgnark.AssertEquivalent(api, computedTransmission, spentTransmission)
	decafgnark.AssertEquivalent(api, spentDivGen, sharedDivGen)
	decafgnark.AssertEquivalent(api, spentTransmission, sharedTransmission)
	api.AssertIsEqual(spend.Note.AssetID, sharedAssetID)

	return spend.Note.Amount, nullifier, rkCompressed, nil
}

func (c *NoteReshapeCircuit) verifyNoteReshapeOutput(
	api frontend.API,
	sharedAK gnarkte.Point,
	sharedDivGen gnarkte.Point,
	sharedTransmission gnarkte.Point,
	sharedAssetID frontend.Variable,
	output *NoteReshapeOutputCircuitFields,
) (frontend.Variable, frontend.Variable, error) {
	createdDivGen := gnarkte.Point{X: output.Note.DivGen.X, Y: output.Note.DivGen.Y}
	createdTransmission := gnarkte.Point{X: output.Note.Transmission.X, Y: output.Note.Transmission.Y}

	createdDivGenFq, err := decafgnark.CompressToField(api, createdDivGen)
	if err != nil {
		return nil, nil, err
	}
	noteCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		output.Note.Blinding,
		output.Note.Amount,
		output.Note.AssetID,
		createdDivGenFq,
		output.Note.TransmissionKeyS,
		output.Note.ClueKey,
	)
	if err != nil {
		return nil, nil, err
	}
	api.AssertIsEqual(noteCommitment, output.NoteCommitment)

	computedTransmission, err := DiversifiedTransmissionKey(
		api,
		c.Auth.NK,
		sharedAK,
		createdDivGen,
		c.Auth.IVKReduced,
		c.Auth.IVKQuotientA,
	)
	if err != nil {
		return nil, nil, err
	}
	decafgnark.AssertEquivalent(api, computedTransmission, createdTransmission)
	decafgnark.AssertEquivalent(api, createdDivGen, sharedDivGen)
	decafgnark.AssertEquivalent(api, createdTransmission, sharedTransmission)
	api.AssertIsEqual(output.Note.AssetID, sharedAssetID)

	return output.Note.Amount, noteCommitment, nil
}
