package abi

import (
	"bytes"
	"fmt"

	"github.com/penumbra-zone/penumbra/tools/gnark/internal/generated"
)

const (
	transferWitnessV1Magic   = "PTWG"
	transferWitnessV1Version = 3
)

type TransferSpendWitnessV1Binary struct {
	Nullifier                   [32]byte
	SpendC2Core                 [32]byte
	SpendComplianceCiphertext   [][32]byte
	SpendDleqC                  [32]byte
	SpendDleqS                  [32]byte
	SpentNoteBlinding           [32]byte
	SpentNoteAmount             [32]byte
	SpentNoteAssetID            [32]byte
	SpentTransmissionKey        [32]byte
	SpentClueKey                [32]byte
	StateCommitmentCommitment   [32]byte
	StateCommitmentPosition     uint64
	StateCommitmentAuthPath     [][3][32]byte
	SpendAuthRandomizer         [32]byte
	SpendComplianceEphemeral    [32]byte
	SpendIsFlagged              bool
	SpendSalt                   [32]byte
	RKAffine                    PointAffineBinary
	SpendEPKAffine              PointAffineBinary
	SpentDiversifiedGeneratorXY PointAffineBinary
	SpentTransmissionKeyXY      PointAffineBinary
}

type TransferOutputWitnessV1Binary struct {
	NoteCommitment                [32]byte
	OutputC2Core                  [32]byte
	OutputC2Ext                   [32]byte
	OutputC2Sext                  [32]byte
	OutputComplianceCiphertext    [][32]byte
	OutputDleqC1                  [32]byte
	OutputDleqS1                  [32]byte
	OutputDleqC2                  [32]byte
	OutputDleqS2                  [32]byte
	OutputDleqC3                  [32]byte
	OutputDleqS3                  [32]byte
	CreatedNoteBlinding           [32]byte
	CreatedNoteAmount             [32]byte
	CreatedNoteAssetID            [32]byte
	CreatedTransmissionKey        [32]byte
	CreatedClueKey                [32]byte
	RecipientCompliancePath       MerklePathBinary
	RecipientCompliancePosition   uint64
	RecipientAssetID              [32]byte
	RecipientD                    [32]byte
	OutputComplianceEphemeral     [32]byte
	OutputR2                      [32]byte
	OutputR3                      [32]byte
	OutputIsFlagged               bool
	OutputSalt                    [32]byte
	OutputEPK1Affine              PointAffineBinary
	OutputEPK2Affine              PointAffineBinary
	OutputEPK3Affine              PointAffineBinary
	CreatedDiversifiedGeneratorXY PointAffineBinary
	CreatedTransmissionKeyXY      PointAffineBinary
	RecipientDiversifiedGenerator PointAffineBinary
	RecipientTransmissionKey      PointAffineBinary
}

type TransferWitnessV1Binary struct {
	TotalLength uint32
	FamilyID    uint32
	NIn         uint32
	NOut        uint32

	Anchor               [32]byte
	BalanceCommitment    [32]byte
	AssetAnchor          [32]byte
	ComplianceAnchor     [32]byte
	TargetTimestamp      [32]byte
	ClaimedStatementHash [32]byte
	StatementFields      [][32]byte

	ActionBalanceBlinding    [32]byte
	AK                       [32]byte
	NK                       [32]byte
	AssetPath                MerklePathBinary
	AssetPosition            uint64
	AssetIndexedLeaf         IndexedLeafBinary
	IsRegulated              bool
	SenderCompliancePath     MerklePathBinary
	SenderCompliancePosition uint64
	SenderAssetID            [32]byte
	SenderD                  [32]byte
	TxBlindingNonce          [32]byte

	Spends  []TransferSpendWitnessV1Binary
	Outputs []TransferOutputWitnessV1Binary

	BalanceCommitmentAffine    PointAffineBinary
	AKAffine                   PointAffineBinary
	AssetIndexedLeafDKPub      PointAffineBinary
	AssetIndexedLeafRingPK     PointAffineBinary
	SenderDiversifiedGenerator PointAffineBinary
	SenderTransmissionKey      PointAffineBinary
}

func DecodeTransferWitnessV1(payload []byte) (*TransferWitnessV1Binary, generated.TransferFamilySpec, error) {
	reader := bytes.NewReader(payload)

	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	if string(magic) != transferWitnessV1Magic {
		return nil, generated.TransferFamilySpec{}, fmt.Errorf("invalid TransferWitnessV1 magic %q", string(magic))
	}
	version, err := readU32(reader)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	if version != transferWitnessV1Version {
		return nil, generated.TransferFamilySpec{}, fmt.Errorf("unsupported TransferWitnessV1 version %d", version)
	}
	totalLength, err := readU32(reader)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, generated.TransferFamilySpec{}, fmt.Errorf("payload length mismatch: header=%d actual=%d", totalLength, len(payload))
	}
	familyID, err := readU32(reader)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	family, ok := generated.TransferFamilyByID(familyID)
	if !ok {
		return nil, generated.TransferFamilySpec{}, fmt.Errorf("unknown transfer family id %d", familyID)
	}
	witness, err := decodeTransferWitnessV1WithFamily("Transfer", payload, family.NIn, family.NOut, family.ID)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	return witness, family, nil
}

func decodeTransferWitnessV1WithFamily(
	label string,
	payload []byte,
	expectedNIn, expectedNOut int,
	expectedFamilyID uint32,
) (*TransferWitnessV1Binary, error) {
	reader := bytes.NewReader(payload)

	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, err
	}
	if string(magic) != transferWitnessV1Magic {
		return nil, fmt.Errorf("invalid %sWitnessV1 magic %q", label, string(magic))
	}
	version, err := readU32(reader)
	if err != nil {
		return nil, err
	}
	if version != transferWitnessV1Version {
		return nil, fmt.Errorf("unsupported %sWitnessV1 version %d", label, version)
	}
	totalLength, err := readU32(reader)
	if err != nil {
		return nil, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, fmt.Errorf("payload length mismatch: header=%d actual=%d", totalLength, len(payload))
	}

	witness := &TransferWitnessV1Binary{TotalLength: totalLength}
	if witness.FamilyID, err = readU32(reader); err != nil {
		return nil, err
	}
	if expectedFamilyID != 0 && witness.FamilyID != expectedFamilyID {
		return nil, fmt.Errorf("%s witness family mismatch: got %d, expected %d", label, witness.FamilyID, expectedFamilyID)
	}
	if witness.NIn, err = readU32(reader); err != nil {
		return nil, err
	}
	if witness.NOut, err = readU32(reader); err != nil {
		return nil, err
	}
	if int(witness.NIn) != expectedNIn || int(witness.NOut) != expectedNOut {
		return nil, fmt.Errorf("%s witness shape mismatch: got %dx%d, expected %dx%d", label, witness.NIn, witness.NOut, expectedNIn, expectedNOut)
	}
	if witness.Anchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.BalanceCommitment, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.AssetAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ComplianceAnchor, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.TargetTimestamp, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.ClaimedStatementHash, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.StatementFields, err = readVec32(reader); err != nil {
		return nil, err
	}
	if witness.ActionBalanceBlinding, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.AK, err = read32(reader); err != nil {
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
	if witness.IsRegulated, err = readBool(reader); err != nil {
		return nil, err
	}
	if witness.SenderCompliancePath, err = readMerklePath(reader); err != nil {
		return nil, err
	}
	if witness.SenderCompliancePosition, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.SenderAssetID, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.SenderD, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.TxBlindingNonce, err = read32(reader); err != nil {
		return nil, err
	}

	witness.Spends = make([]TransferSpendWitnessV1Binary, witness.NIn)
	for i := range witness.Spends {
		if witness.Spends[i].Nullifier, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpendC2Core, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpendComplianceCiphertext, err = readVec32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpendDleqC, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpendDleqS, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpentNoteBlinding, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpentNoteAmount, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpentNoteAssetID, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpentTransmissionKey, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpentClueKey, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].StateCommitmentCommitment, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].StateCommitmentPosition, err = readU64(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].StateCommitmentAuthPath, err = readTriplePath(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpendAuthRandomizer, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpendComplianceEphemeral, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpendIsFlagged, err = readBool(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpendSalt, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].RKAffine, err = readPointAffine(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpendEPKAffine, err = readPointAffine(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpentDiversifiedGeneratorXY, err = readPointAffine(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].SpentTransmissionKeyXY, err = readPointAffine(reader); err != nil {
			return nil, err
		}
	}

	witness.Outputs = make([]TransferOutputWitnessV1Binary, witness.NOut)
	for i := range witness.Outputs {
		if witness.Outputs[i].NoteCommitment, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputC2Core, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputC2Ext, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputC2Sext, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputComplianceCiphertext, err = readVec32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputDleqC1, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputDleqS1, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputDleqC2, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputDleqS2, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputDleqC3, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputDleqS3, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].CreatedNoteBlinding, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].CreatedNoteAmount, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].CreatedNoteAssetID, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].CreatedTransmissionKey, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].CreatedClueKey, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].RecipientCompliancePath, err = readMerklePath(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].RecipientCompliancePosition, err = readU64(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].RecipientAssetID, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].RecipientD, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputComplianceEphemeral, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputR2, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputR3, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputIsFlagged, err = readBool(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputSalt, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputEPK1Affine, err = readPointAffine(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputEPK2Affine, err = readPointAffine(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].OutputEPK3Affine, err = readPointAffine(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].CreatedDiversifiedGeneratorXY, err = readPointAffine(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].CreatedTransmissionKeyXY, err = readPointAffine(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].RecipientDiversifiedGenerator, err = readPointAffine(reader); err != nil {
			return nil, err
		}
		if witness.Outputs[i].RecipientTransmissionKey, err = readPointAffine(reader); err != nil {
			return nil, err
		}
	}

	if witness.BalanceCommitmentAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.AKAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.AssetIndexedLeafDKPub, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.AssetIndexedLeafRingPK, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.SenderDiversifiedGenerator, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if witness.SenderTransmissionKey, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	if rem := reader.Len(); rem != 0 {
		return nil, fmt.Errorf("%sWitnessV1 has %d trailing bytes", label, rem)
	}
	return witness, nil
}
