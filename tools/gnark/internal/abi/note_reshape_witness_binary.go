package abi

import (
	"bytes"
	"fmt"

	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
)

const (
	noteReshapeWitnessMagic = "PNWG"
	maxNoteReshapeItems     = 8
)

type NoteReshapeSpendWitnessBinary struct {
	IsDummy                     bool
	Nullifier                   [32]byte
	DummyNullifierSeed          [32]byte
	SpentNoteBlinding           [32]byte
	SpentNoteAmount             [32]byte
	SpentNoteRecoveryCommitment [32]byte
	StateCommitmentCommitment   [32]byte
	StateCommitmentPosition     uint64
	StateCommitmentAuthPath     [][3][32]byte
	SpendAuthRandomizer         [32]byte
	RKAffine                    PointAffineBinary
	HistoryRequired             bool
}

type NoteReshapeOutputWitnessBinary struct {
	NoteCommitment      [32]byte
	RecoveryCommitment  [32]byte
	CreatedNoteBlinding [32]byte
	CreatedNoteAmount   [32]byte
	RecoveryCapsule     RecoveryCapsuleWitnessBinary
}

type NoteReshapeSharedNoteContextWitnessBinary struct {
	AssetID [32]byte
	DivGen  PointAffineBinary
}

type NoteReshapeWitnessBinary struct {
	TotalLength              uint32
	FamilyID                 uint32
	NIn                      uint32
	NOut                     uint32
	Anchor                   [32]byte
	ClaimedStatementHash     [32]byte
	AssetAnchor              [32]byte
	ComplianceAnchor         [32]byte
	RoutingTag               [32]byte
	RoutingParameterSetID    [32]byte
	RecentPositionFloor      [32]byte
	ActionBalanceBlinding    [32]byte
	NK                       [32]byte
	AssetPath                MerklePathBinary
	AssetPosition            uint64
	AssetIndexedLeaf         IndexedLeafBinary
	AssetIndexedLeafDKPub    PointAffineBinary
	AssetIndexedLeafRingPK   PointAffineBinary
	IsRegulated              bool
	RegulatedPrecision       uint8
	UnregulatedPrecision     uint8
	RoutingAsOfHeight        uint64
	RoutingNonce             [32]byte
	SenderCompliancePath     MerklePathBinary
	SenderCompliancePosition uint64
	SenderCapkAffine         PointAffineBinary
	SenderRnkDhPkAffine      PointAffineBinary
	SenderRnkCommitment      [32]byte
	SenderStatus             [32]byte
	Shared                   NoteReshapeSharedNoteContextWitnessBinary
	Spends                   []NoteReshapeSpendWitnessBinary
	Outputs                  []NoteReshapeOutputWitnessBinary
	BalanceCommitmentAffine  PointAffineBinary
	AKAffine                 PointAffineBinary
}

func DecodeNoteReshapeWitness(payload []byte) (*NoteReshapeWitnessBinary, generated.NoteReshapeFamilySpec, error) {
	witness, err := decodeNoteReshapeWitness(payload)
	if err != nil {
		return nil, generated.NoteReshapeFamilySpec{}, err
	}
	family, ok := generated.NoteReshapeFamilyByID(witness.FamilyID)
	if !ok {
		return nil, generated.NoteReshapeFamilySpec{}, fmt.Errorf("unknown note reshape family id %d", witness.FamilyID)
	}
	if int(witness.NIn) != family.NIn || int(witness.NOut) != family.NOut {
		return nil, generated.NoteReshapeFamilySpec{}, fmt.Errorf("note reshape witness shape mismatch: got %dx%d, expected %dx%d", witness.NIn, witness.NOut, family.NIn, family.NOut)
	}
	return witness, family, nil
}

func decodeNoteReshapeWitness(payload []byte) (*NoteReshapeWitnessBinary, error) {
	reader := bytes.NewReader(payload)
	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, err
	}
	if string(magic) != noteReshapeWitnessMagic {
		return nil, fmt.Errorf("invalid note reshape witness magic %q", string(magic))
	}
	totalLength, err := readU32(reader)
	if err != nil {
		return nil, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, fmt.Errorf("payload length mismatch: header=%d actual=%d", totalLength, len(payload))
	}
	witness := &NoteReshapeWitnessBinary{TotalLength: totalLength}
	if witness.FamilyID, err = readU32(reader); err != nil {
		return nil, err
	}
	if witness.NIn, err = readU32(reader); err != nil {
		return nil, err
	}
	if witness.NOut, err = readU32(reader); err != nil {
		return nil, err
	}
	family, ok := generated.NoteReshapeFamilyByID(witness.FamilyID)
	if !ok {
		return nil, fmt.Errorf("unknown note reshape family id %d", witness.FamilyID)
	}
	if witness.Anchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ClaimedStatementHash, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.AssetAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ComplianceAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.RoutingTag, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.RoutingParameterSetID, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.RecentPositionFloor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ActionBalanceBlinding, err = readFr32(reader); err != nil {
		return nil, err
	}
	if witness.NK, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.AssetPath, err = readMerklePath(reader); err != nil {
		return nil, err
	}
	if witness.AssetPosition, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.AssetIndexedLeaf, err = readIndexedLeaf(reader); err != nil {
		return nil, err
	}
	if witness.AssetIndexedLeafDKPub, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.AssetIndexedLeafRingPK, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.IsRegulated, err = readBool(reader); err != nil {
		return nil, err
	}
	if witness.RegulatedPrecision, err = readU8(reader); err != nil {
		return nil, err
	}
	if witness.UnregulatedPrecision, err = readU8(reader); err != nil {
		return nil, err
	}
	if witness.RoutingAsOfHeight, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.RoutingNonce, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.SenderCompliancePath, err = readMerklePath(reader); err != nil {
		return nil, err
	}
	if witness.SenderCompliancePosition, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.SenderCapkAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.SenderRnkDhPkAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.SenderRnkCommitment, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.SenderStatus, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Shared.AssetID, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Shared.DivGen, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.NIn > maxNoteReshapeItems || witness.NOut > maxNoteReshapeItems {
		return nil, fmt.Errorf("note reshape witness exceeds maximum item count")
	}
	witness.Spends = make([]NoteReshapeSpendWitnessBinary, witness.NIn)
	for i := range witness.Spends {
		if witness.Spends[i], err = readNoteReshapeSpend(reader, family.InputPadding == generated.InputPaddingSyntheticPrivate); err != nil {
			return nil, err
		}
	}
	witness.Outputs = make([]NoteReshapeOutputWitnessBinary, witness.NOut)
	for i := range witness.Outputs {
		if witness.Outputs[i], err = readNoteReshapeOutput(reader); err != nil {
			return nil, err
		}
	}
	if witness.BalanceCommitmentAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.AKAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if reader.Len() != 0 {
		return nil, fmt.Errorf("trailing bytes in note reshape witness: %d", reader.Len())
	}
	return witness, nil
}

func readNoteReshapeSpend(reader *bytes.Reader, syntheticPrivatePadding bool) (NoteReshapeSpendWitnessBinary, error) {
	var out NoteReshapeSpendWitnessBinary
	var err error
	if syntheticPrivatePadding {
		flag, readErr := readU32(reader)
		if readErr != nil {
			return out, readErr
		}
		if flag > 1 {
			return out, fmt.Errorf("invalid note reshape input dummy flag %d", flag)
		}
		out.IsDummy = flag == 1
		if out.DummyNullifierSeed, err = read32(reader); err != nil {
			return out, err
		}
	}
	if out.Nullifier, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentNoteBlinding, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentNoteAmount, err = read32(reader); err != nil {
		return out, err
	}
	if out.SpentNoteRecoveryCommitment, err = read32(reader); err != nil {
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
	if out.SpendAuthRandomizer, err = readFr32(reader); err != nil {
		return out, err
	}
	if out.RKAffine, err = readPointAffine(reader); err != nil {
		return out, err
	}
	if out.HistoryRequired, err = readBool(reader); err != nil {
		return out, err
	}
	return out, nil
}

func readNoteReshapeOutput(reader *bytes.Reader) (NoteReshapeOutputWitnessBinary, error) {
	var out NoteReshapeOutputWitnessBinary
	var err error
	if out.NoteCommitment, err = read32(reader); err != nil {
		return out, err
	}
	if out.RecoveryCommitment, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteBlinding, err = read32(reader); err != nil {
		return out, err
	}
	if out.CreatedNoteAmount, err = read32(reader); err != nil {
		return out, err
	}
	if out.RecoveryCapsule, err = readRecoveryCapsule(reader); err != nil {
		return out, err
	}
	return out, nil
}
