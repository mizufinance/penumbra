package abi

import (
	"bytes"
	"fmt"

	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
)

const (
	transferWitnessV1Magic   = "PTWG"
	transferWitnessV1Version = 7
)

type TransferComplianceCiphertextWitnessV1Binary struct {
	C2                 [32]byte
	Ciphertext         [][32]byte
	SubjectBD          [32]byte
	RingIDHash         [32]byte
	PolicyIDHash       [32]byte
	ResourceHash       [32]byte
	PermissionHash     [32]byte
	Tier               uint64
	StatementTimestamp [32]byte
	Salt               [32]byte
	Challenge          [32]byte
	Response           [32]byte
	EPKAffine          PointAffineBinary
	DerivedPKAffine    PointAffineBinary
	EncCmtAffine       PointAffineBinary
	SharedPointAffine  PointAffineBinary
}

type TransferSpendWitnessV1Binary struct {
	Nullifier                   [32]byte
	SpentNoteBlinding           [32]byte
	SpentNoteAmount             [32]byte
	SpentNoteAssetID            [32]byte
	SpentTransmissionKey        [32]byte
	SpentClueKey                [32]byte
	StateCommitmentCommitment   [32]byte
	StateCommitmentPosition     uint64
	StateCommitmentAuthPath     [][3][32]byte
	SpendAuthRandomizer         [32]byte
	IsDummy                     bool
	DummyNullifierSeed          [32]byte
	DummySpendAuthKey           [32]byte
	RKAffine                    PointAffineBinary
	SpentDiversifiedGeneratorXY PointAffineBinary
	SpentTransmissionKeyXY      PointAffineBinary
}

type TransferOutputWitnessV1Binary struct {
	NoteCommitment              [32]byte
	CreatedNoteBlinding         [32]byte
	CreatedNoteAmount           [32]byte
	CreatedNoteAssetID          [32]byte
	CreatedTransmissionKey      [32]byte
	CreatedClueKey              [32]byte
	RecipientCompliancePath     MerklePathBinary
	RecipientCompliancePosition uint64
	RecipientAssetID            [32]byte
	RecipientD                  [32]byte
	// Output 0 is the receiver leg. Output 1, when present, is sender-owned change.
	IsReceiver                    bool
	CreatedDiversifiedGeneratorXY PointAffineBinary
	CreatedTransmissionKeyXY      PointAffineBinary
	RecipientDiversifiedGenerator PointAffineBinary
	RecipientTransmissionKey      PointAffineBinary
}

type TransferWitnessV1Binary struct {
	TotalLength uint32
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
	TransferNonceRoot        [32]byte

	DetectionCiphertext [][32]byte
	SenderCore          TransferComplianceCiphertextWitnessV1Binary
	SenderExt           TransferComplianceCiphertextWitnessV1Binary
	OutputCore          TransferComplianceCiphertextWitnessV1Binary
	OutputExt           TransferComplianceCiphertextWitnessV1Binary
	SenderRCore         [32]byte
	SenderRExt          [32]byte
	OutputRCore         [32]byte
	OutputRExt          [32]byte

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
	family, ok := generated.TransferFamilyByLabel("transfer")
	if !ok {
		return nil, generated.TransferFamilySpec{}, fmt.Errorf("missing generated transfer spec")
	}
	witness, err := decodeTransferWitnessV1("Transfer", payload)
	if err != nil {
		return nil, generated.TransferFamilySpec{}, err
	}
	return witness, family, nil
}

func decodeTransferWitnessV1(
	label string,
	payload []byte,
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
	if witness.NIn, err = readU32(reader); err != nil {
		return nil, err
	}
	if witness.NOut, err = readU32(reader); err != nil {
		return nil, err
	}
	if int(witness.NIn) != circuits.TransferCircuitInputs || int(witness.NOut) != circuits.TransferCircuitOutputs {
		return nil, fmt.Errorf(
			"%s witness shape mismatch: got %dx%d, expected %dx%d",
			label,
			witness.NIn,
			witness.NOut,
			circuits.TransferCircuitInputs,
			circuits.TransferCircuitOutputs,
		)
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
	if witness.TransferNonceRoot, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.DetectionCiphertext, err = readVec32(reader); err != nil {
		return nil, err
	}
	if witness.SenderCore, err = readTransferComplianceTier(reader); err != nil {
		return nil, err
	}
	if witness.SenderExt, err = readTransferComplianceTier(reader); err != nil {
		return nil, err
	}
	if witness.OutputCore, err = readTransferComplianceTier(reader); err != nil {
		return nil, err
	}
	if witness.OutputExt, err = readTransferComplianceTier(reader); err != nil {
		return nil, err
	}
	if witness.SenderRCore, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.SenderRExt, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.OutputRCore, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.OutputRExt, err = read32(reader); err != nil {
		return nil, err
	}

	witness.Spends = make([]TransferSpendWitnessV1Binary, witness.NIn)
	for i := range witness.Spends {
		if witness.Spends[i].Nullifier, err = read32(reader); err != nil {
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
		if witness.Spends[i].IsDummy, err = readBool(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].DummyNullifierSeed, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].DummySpendAuthKey, err = read32(reader); err != nil {
			return nil, err
		}
		if witness.Spends[i].RKAffine, err = readPointAffine(reader); err != nil {
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
		if witness.Outputs[i].IsReceiver, err = readBool(reader); err != nil {
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

	if extra, err := readExact(reader, int(reader.Len())); err != nil {
		return nil, err
	} else if len(extra) != 0 {
		return nil, fmt.Errorf("%s witness has %d trailing bytes", label, len(extra))
	}

	return witness, nil
}

func readTransferComplianceTier(reader *bytes.Reader) (TransferComplianceCiphertextWitnessV1Binary, error) {
	var tier TransferComplianceCiphertextWitnessV1Binary
	var err error
	if tier.C2, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.Ciphertext, err = readVec32(reader); err != nil {
		return tier, err
	}
	if tier.SubjectBD, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.RingIDHash, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.PolicyIDHash, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.ResourceHash, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.PermissionHash, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.Tier, err = readU64(reader); err != nil {
		return tier, err
	}
	if tier.StatementTimestamp, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.Salt, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.Challenge, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.Response, err = read32(reader); err != nil {
		return tier, err
	}
	if tier.EPKAffine, err = readPointAffine(reader); err != nil {
		return tier, err
	}
	if tier.DerivedPKAffine, err = readPointAffine(reader); err != nil {
		return tier, err
	}
	if tier.EncCmtAffine, err = readPointAffine(reader); err != nil {
		return tier, err
	}
	if tier.SharedPointAffine, err = readPointAffine(reader); err != nil {
		return tier, err
	}
	return tier, nil
}
