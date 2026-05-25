package abi

import (
	"fmt"

	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

func NewConsolidateCircuitAssignmentFromWitnessV1(payload []byte) (*circuits.NoteReshapeCircuit, generated.ConsolidateFamilySpec, error) {
	witness, family, err := DecodeConsolidateWitnessV1(payload)
	if err != nil {
		return nil, generated.ConsolidateFamilySpec{}, fmt.Errorf("decode ConsolidateWitnessV1: %w", err)
	}
	assignment, err := newConsolidateCircuitAssignment(witness, family.NIn, family.NOut)
	if err != nil {
		return nil, generated.ConsolidateFamilySpec{}, err
	}
	return assignment, family, nil
}

func NewSplitCircuitAssignmentFromWitnessV1(payload []byte) (*circuits.NoteReshapeCircuit, generated.SplitFamilySpec, error) {
	witness, family, err := DecodeSplitWitnessV1(payload)
	if err != nil {
		return nil, generated.SplitFamilySpec{}, fmt.Errorf("decode SplitWitnessV1: %w", err)
	}
	assignment, err := newSplitCircuitAssignment(witness, family.NIn, family.NOut)
	if err != nil {
		return nil, generated.SplitFamilySpec{}, err
	}
	return assignment, family, nil
}

func expectedNoteReshapeStatementFieldCount(nIn, nOut int) int {
	return primitives.NoteReshapeStatementFieldCount(nIn, nOut)
}

func newNoteReshapeAuthSharedFields(
	nk [32]byte,
	akCompressed [32]byte,
	akAffine PointAffineBinary,
) (circuits.TransferAuthSharedFields, error) {
	ivkReduced, quotientA, err := incomingViewingKeyReductionFromBinary(nk, akCompressed)
	if err != nil {
		return circuits.TransferAuthSharedFields{}, fmt.Errorf("compute note reshape ivk reduction from binary witness: %w", err)
	}
	return circuits.TransferAuthSharedFields{
		AK:           point2DString(akAffine),
		NK:           primitives.LittleEndianBytesToBigInt(nk[:]).String(),
		IVKReduced:   ivkReduced.String(),
		IVKQuotientA: quotientA,
	}, nil
}

func newNoteReshapeSpendCircuitFields(
	witness *NoteReshapeSpendWitnessV1Binary,
) (circuits.TransferSpendCircuitFields, error) {
	var zero circuits.TransferSpendCircuitFields
	statePath, err := statePathFromBinary(witness.StateCommitmentAuthPath)
	if err != nil {
		return zero, fmt.Errorf("decode note reshape spend state commitment auth path: %w", err)
	}
	return circuits.TransferSpendCircuitFields{
		Nullifier: fqString(witness.Nullifier),
		RK:        point2DString(witness.RKAffine),
		Note: noteFields(
			fqString(witness.SpentNoteBlinding),
			fqString(witness.SpentNoteAmount),
			fqString(witness.SpentNoteAssetID),
			fqString(witness.SpentDivGenAffine.X),
			fqString(witness.SpentDivGenAffine.Y),
			fqString(witness.SpentTransmissionKey),
			fqString(witness.SpentTransmissionAffine.X),
			fqString(witness.SpentTransmissionAffine.Y),
			fqString(witness.SpentClueKey),
		),
		StateProof: circuits.StateCommitmentFields{
			Commitment: fqString(witness.StateCommitmentCommitment),
			Position:   witness.StateCommitmentPosition,
			Path:       statePath,
		},
		AuthRandomizer:     fqString(witness.SpendAuthRandomizer),
		IsDummy:            "0",
		DummyNullifierSeed: "0",
		DummySpendAuthKey:  "0",
	}, nil
}

func newNoteReshapeOutputCircuitFields(
	witness *NoteReshapeOutputWitnessV1Binary,
) circuits.NoteReshapeOutputCircuitFields {
	return circuits.NoteReshapeOutputCircuitFields{
		NoteCommitment: fqString(witness.NoteCommitment),
		Note: noteFields(
			fqString(witness.CreatedNoteBlinding),
			fqString(witness.CreatedNoteAmount),
			fqString(witness.CreatedNoteAssetID),
			fqString(witness.CreatedDivGenAffine.X),
			fqString(witness.CreatedDivGenAffine.Y),
			fqString(witness.CreatedTransmissionKey),
			fqString(witness.CreatedTransmissionAffine.X),
			fqString(witness.CreatedTransmissionAffine.Y),
			fqString(witness.CreatedClueKey),
		),
	}
}

func newConsolidateCircuitAssignment(
	witness *ConsolidateWitnessV1Binary,
	expectedNIn, expectedNOut int,
) (*circuits.NoteReshapeCircuit, error) {
	if int(witness.NIn) != expectedNIn || int(witness.NOut) != expectedNOut {
		return nil, fmt.Errorf("consolidate witness shape mismatch: got %dx%d, expected %dx%d", witness.NIn, witness.NOut, expectedNIn, expectedNOut)
	}
	if len(witness.StatementFields) != expectedNoteReshapeStatementFieldCount(expectedNIn, expectedNOut) {
		return nil, fmt.Errorf("expected %d consolidate statement fields, got %d", expectedNoteReshapeStatementFieldCount(expectedNIn, expectedNOut), len(witness.StatementFields))
	}
	if len(witness.Spends) != expectedNIn || len(witness.Outputs) != expectedNOut {
		return nil, fmt.Errorf("consolidate witness counts mismatch: spends=%d outputs=%d expected=%dx%d", len(witness.Spends), len(witness.Outputs), expectedNIn, expectedNOut)
	}
	auth, err := newNoteReshapeAuthSharedFields(witness.NK, witness.AK, witness.AKAffine)
	if err != nil {
		return nil, err
	}
	assignment := circuits.NewConsolidateCircuit(expectedNIn)
	assignment.ClaimedStatementHash = fqString(witness.ClaimedStatementHash)
	assignment.Anchor = fqString(witness.Anchor)
	assignment.BalanceCommitment = point2DString(witness.BalanceCommitmentAffine)
	assignment.ActionBalanceBlinding = fqString(witness.ActionBalanceBlinding)
	assignment.Auth = auth
	for i := range witness.Spends {
		spend, err := newNoteReshapeSpendCircuitFields(&witness.Spends[i])
		if err != nil {
			return nil, err
		}
		assignment.Spends[i] = spend
	}
	for i := range witness.Outputs {
		assignment.Outputs[i] = newNoteReshapeOutputCircuitFields(&witness.Outputs[i])
	}
	return assignment, nil
}

func newSplitCircuitAssignment(
	witness *SplitWitnessV1Binary,
	expectedNIn, expectedNOut int,
) (*circuits.NoteReshapeCircuit, error) {
	if int(witness.NIn) != expectedNIn || int(witness.NOut) != expectedNOut {
		return nil, fmt.Errorf("split witness shape mismatch: got %dx%d, expected %dx%d", witness.NIn, witness.NOut, expectedNIn, expectedNOut)
	}
	if len(witness.StatementFields) != expectedNoteReshapeStatementFieldCount(expectedNIn, expectedNOut) {
		return nil, fmt.Errorf("expected %d split statement fields, got %d", expectedNoteReshapeStatementFieldCount(expectedNIn, expectedNOut), len(witness.StatementFields))
	}
	if len(witness.Spends) != expectedNIn || len(witness.Outputs) != expectedNOut {
		return nil, fmt.Errorf("split witness counts mismatch: spends=%d outputs=%d expected=%dx%d", len(witness.Spends), len(witness.Outputs), expectedNIn, expectedNOut)
	}
	auth, err := newNoteReshapeAuthSharedFields(witness.NK, witness.AK, witness.AKAffine)
	if err != nil {
		return nil, err
	}
	assignment := circuits.NewSplitCircuit(expectedNOut)
	assignment.ClaimedStatementHash = fqString(witness.ClaimedStatementHash)
	assignment.Anchor = fqString(witness.Anchor)
	assignment.BalanceCommitment = point2DString(witness.BalanceCommitmentAffine)
	assignment.ActionBalanceBlinding = fqString(witness.ActionBalanceBlinding)
	assignment.Auth = auth
	for i := range witness.Spends {
		spend, err := newNoteReshapeSpendCircuitFields(&witness.Spends[i])
		if err != nil {
			return nil, err
		}
		assignment.Spends[i] = spend
	}
	for i := range witness.Outputs {
		assignment.Outputs[i] = newNoteReshapeOutputCircuitFields(&witness.Outputs[i])
	}
	return assignment, nil
}
