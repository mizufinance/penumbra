package abi

import (
	"bytes"
	"fmt"

	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
)

const (
	consolidateWitnessV1Magic = "PCWG"
	splitWitnessV1Magic       = "PSWG"
	noteReshapeWitnessVersion = 1
	maxNoteReshapeItems       = 8
	minNoteReshapeSpendBytes  = 32*7 + 8 + 4 + 32 + 64*3
	minNoteReshapeOutputBytes = 32*6 + 64*2
	minNoteReshapeTailBytes   = 64 * 2
)

type NoteReshapeSpendWitnessV1Binary struct {
	Nullifier                 [32]byte
	SpentNoteBlinding         [32]byte
	SpentNoteAmount           [32]byte
	SpentNoteAssetID          [32]byte
	SpentTransmissionKey      [32]byte
	SpentClueKey              [32]byte
	StateCommitmentCommitment [32]byte
	StateCommitmentPosition   uint64
	StateCommitmentAuthPath   [][3][32]byte
	SpendAuthRandomizer       [32]byte
	RKAffine                  PointAffineBinary
	SpentDivGenAffine         PointAffineBinary
	SpentTransmissionAffine   PointAffineBinary
}

type NoteReshapeOutputWitnessV1Binary struct {
	NoteCommitment            [32]byte
	CreatedNoteBlinding       [32]byte
	CreatedNoteAmount         [32]byte
	CreatedNoteAssetID        [32]byte
	CreatedTransmissionKey    [32]byte
	CreatedClueKey            [32]byte
	CreatedDivGenAffine       PointAffineBinary
	CreatedTransmissionAffine PointAffineBinary
}

type ConsolidateWitnessV1Binary struct {
	TotalLength uint32
	FamilyID    uint32
	NIn         uint32
	NOut        uint32

	Anchor                [32]byte
	BalanceCommitment     [32]byte
	ClaimedStatementHash  [32]byte
	StatementFields       [][32]byte
	ActionBalanceBlinding [32]byte
	AK                    [32]byte
	NK                    [32]byte

	Spends  []NoteReshapeSpendWitnessV1Binary
	Outputs []NoteReshapeOutputWitnessV1Binary

	BalanceCommitmentAffine PointAffineBinary
	AKAffine                PointAffineBinary
}

type SplitWitnessV1Binary struct {
	TotalLength uint32
	FamilyID    uint32
	NIn         uint32
	NOut        uint32

	Anchor                [32]byte
	BalanceCommitment     [32]byte
	ClaimedStatementHash  [32]byte
	StatementFields       [][32]byte
	ActionBalanceBlinding [32]byte
	AK                    [32]byte
	NK                    [32]byte

	Spends  []NoteReshapeSpendWitnessV1Binary
	Outputs []NoteReshapeOutputWitnessV1Binary

	BalanceCommitmentAffine PointAffineBinary
	AKAffine                PointAffineBinary
}

func DecodeConsolidateWitnessV1(payload []byte) (*ConsolidateWitnessV1Binary, generated.ConsolidateFamilySpec, error) {
	decoded, familyID, nIn, nOut, err := decodeNoteReshapeWitnessV1(consolidateWitnessV1Magic, payload)
	if err != nil {
		return nil, generated.ConsolidateFamilySpec{}, err
	}
	family, ok := generated.ConsolidateFamilyByID(familyID)
	if !ok {
		return nil, generated.ConsolidateFamilySpec{}, fmt.Errorf("unknown consolidate family id %d", familyID)
	}
	if nIn != family.NIn || nOut != family.NOut {
		return nil, generated.ConsolidateFamilySpec{}, fmt.Errorf("consolidate witness shape mismatch: got %dx%d, expected %dx%d", nIn, nOut, family.NIn, family.NOut)
	}
	return &ConsolidateWitnessV1Binary{
		TotalLength:             decoded.TotalLength,
		FamilyID:                decoded.FamilyID,
		NIn:                     decoded.NIn,
		NOut:                    decoded.NOut,
		Anchor:                  decoded.Anchor,
		BalanceCommitment:       decoded.BalanceCommitment,
		ClaimedStatementHash:    decoded.ClaimedStatementHash,
		StatementFields:         decoded.StatementFields,
		ActionBalanceBlinding:   decoded.ActionBalanceBlinding,
		AK:                      decoded.AK,
		NK:                      decoded.NK,
		Spends:                  decoded.Spends,
		Outputs:                 decoded.Outputs,
		BalanceCommitmentAffine: decoded.BalanceCommitmentAffine,
		AKAffine:                decoded.AKAffine,
	}, family, nil
}

func DecodeSplitWitnessV1(payload []byte) (*SplitWitnessV1Binary, generated.SplitFamilySpec, error) {
	decoded, familyID, nIn, nOut, err := decodeNoteReshapeWitnessV1(splitWitnessV1Magic, payload)
	if err != nil {
		return nil, generated.SplitFamilySpec{}, err
	}
	family, ok := generated.SplitFamilyByID(familyID)
	if !ok {
		return nil, generated.SplitFamilySpec{}, fmt.Errorf("unknown split family id %d", familyID)
	}
	if nIn != family.NIn || nOut != family.NOut {
		return nil, generated.SplitFamilySpec{}, fmt.Errorf("split witness shape mismatch: got %dx%d, expected %dx%d", nIn, nOut, family.NIn, family.NOut)
	}
	return &SplitWitnessV1Binary{
		TotalLength:             decoded.TotalLength,
		FamilyID:                decoded.FamilyID,
		NIn:                     decoded.NIn,
		NOut:                    decoded.NOut,
		Anchor:                  decoded.Anchor,
		BalanceCommitment:       decoded.BalanceCommitment,
		ClaimedStatementHash:    decoded.ClaimedStatementHash,
		StatementFields:         decoded.StatementFields,
		ActionBalanceBlinding:   decoded.ActionBalanceBlinding,
		AK:                      decoded.AK,
		NK:                      decoded.NK,
		Spends:                  decoded.Spends,
		Outputs:                 decoded.Outputs,
		BalanceCommitmentAffine: decoded.BalanceCommitmentAffine,
		AKAffine:                decoded.AKAffine,
	}, family, nil
}

type decodedNoteReshapeWitnessV1 struct {
	TotalLength uint32
	FamilyID    uint32
	NIn         uint32
	NOut        uint32

	Anchor                [32]byte
	BalanceCommitment     [32]byte
	ClaimedStatementHash  [32]byte
	StatementFields       [][32]byte
	ActionBalanceBlinding [32]byte
	AK                    [32]byte
	NK                    [32]byte

	Spends  []NoteReshapeSpendWitnessV1Binary
	Outputs []NoteReshapeOutputWitnessV1Binary

	BalanceCommitmentAffine PointAffineBinary
	AKAffine                PointAffineBinary
}

func decodeNoteReshapeWitnessV1(
	expectedMagic string,
	payload []byte,
) (*decodedNoteReshapeWitnessV1, uint32, int, int, error) {
	reader := bytes.NewReader(payload)

	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, 0, 0, 0, err
	}
	if string(magic) != expectedMagic {
		return nil, 0, 0, 0, fmt.Errorf("invalid note reshape witness magic %q", string(magic))
	}
	version, err := readU32(reader)
	if err != nil {
		return nil, 0, 0, 0, err
	}
	if version != noteReshapeWitnessVersion {
		return nil, 0, 0, 0, fmt.Errorf("unsupported note reshape witness version %d", version)
	}
	totalLength, err := readU32(reader)
	if err != nil {
		return nil, 0, 0, 0, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, 0, 0, 0, fmt.Errorf("payload length mismatch: header=%d actual=%d", totalLength, len(payload))
	}
	familyID, err := readU32(reader)
	if err != nil {
		return nil, 0, 0, 0, err
	}
	nIn, err := readU32(reader)
	if err != nil {
		return nil, 0, 0, 0, err
	}
	nOut, err := readU32(reader)
	if err != nil {
		return nil, 0, 0, 0, err
	}

	witness := &decodedNoteReshapeWitnessV1{
		TotalLength: totalLength,
		FamilyID:    familyID,
		NIn:         nIn,
		NOut:        nOut,
	}
	if witness.Anchor, err = read32(reader); err != nil {
		return nil, 0, 0, 0, err
	}
	if witness.BalanceCommitment, err = read32(reader); err != nil {
		return nil, 0, 0, 0, err
	}
	if witness.ClaimedStatementHash, err = read32(reader); err != nil {
		return nil, 0, 0, 0, err
	}
	if witness.StatementFields, err = readVec32(reader); err != nil {
		return nil, 0, 0, 0, err
	}
	if witness.ActionBalanceBlinding, err = read32(reader); err != nil {
		return nil, 0, 0, 0, err
	}
	if witness.AK, err = read32(reader); err != nil {
		return nil, 0, 0, 0, err
	}
	if witness.NK, err = read32(reader); err != nil {
		return nil, 0, 0, 0, err
	}
	if nIn > maxNoteReshapeItems {
		return nil, 0, 0, 0, fmt.Errorf("note reshape witness nIn %d exceeds max %d", nIn, maxNoteReshapeItems)
	}
	if nOut > maxNoteReshapeItems {
		return nil, 0, 0, 0, fmt.Errorf("note reshape witness nOut %d exceeds max %d", nOut, maxNoteReshapeItems)
	}
	minRemaining := int(nIn)*minNoteReshapeSpendBytes + int(nOut)*minNoteReshapeOutputBytes + minNoteReshapeTailBytes
	if reader.Len() < minRemaining {
		return nil, 0, 0, 0, fmt.Errorf(
			"note reshape witness too short for %dx%d items: remaining=%d min=%d",
			nIn, nOut, reader.Len(), minRemaining,
		)
	}

	witness.Spends = make([]NoteReshapeSpendWitnessV1Binary, nIn)
	for i := range witness.Spends {
		if witness.Spends[i], err = readNoteReshapeSpend(reader); err != nil {
			return nil, 0, 0, 0, err
		}
	}
	witness.Outputs = make([]NoteReshapeOutputWitnessV1Binary, nOut)
	for i := range witness.Outputs {
		if witness.Outputs[i], err = readNoteReshapeOutput(reader); err != nil {
			return nil, 0, 0, 0, err
		}
	}
	if witness.BalanceCommitmentAffine, err = readPointAffine(reader); err != nil {
		return nil, 0, 0, 0, err
	}
	if witness.AKAffine, err = readPointAffine(reader); err != nil {
		return nil, 0, 0, 0, err
	}
	if reader.Len() != 0 {
		return nil, 0, 0, 0, fmt.Errorf("trailing bytes in note reshape witness: %d", reader.Len())
	}
	return witness, familyID, int(nIn), int(nOut), nil
}

func readNoteReshapeSpend(reader *bytes.Reader) (NoteReshapeSpendWitnessV1Binary, error) {
	var out NoteReshapeSpendWitnessV1Binary
	var err error
	if out.Nullifier, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentNoteBlinding, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentNoteAmount, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentNoteAssetID, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentTransmissionKey, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentClueKey, err = read32(reader); err != nil {
		return out, err
	}
	if out.StateCommitmentCommitment, err = read32(reader); err != nil {
		return out, err
	}
	if out.StateCommitmentPosition, err = readU64(reader); err != nil {
		return out, err
	}
	if out.StateCommitmentAuthPath, err = readTriplePath(reader); err != nil {
		return out, err
	}
	if out.SpendAuthRandomizer, err = read32(reader); err != nil {
		return out, err
	}
	if out.RKAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	if out.SpentDivGenAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	if out.SpentTransmissionAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	return out, nil
}

func readNoteReshapeOutput(reader *bytes.Reader) (NoteReshapeOutputWitnessV1Binary, error) {
	var out NoteReshapeOutputWitnessV1Binary
	var err error
	if out.NoteCommitment, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteBlinding, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteAmount, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteAssetID, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedTransmissionKey, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedClueKey, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedDivGenAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	if out.CreatedTransmissionAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	return out, nil
}
