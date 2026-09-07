package abi

import (
	"bytes"
	"encoding/binary"
	"fmt"
)

const noteSeizureWitnessMagic = "NSZW"

type NoteSeizureRecoveryWitnessBinary struct {
	Commitment            [32]byte
	EPKAffine             PointAffineBinary
	C2                    [32]byte
	Salt                  [32]byte
	KeyConfirmation       [32]byte
	EncryptedAmount       [32]byte
	EncryptedNoteBlinding [32]byte
	Seed                  [32]byte
}

type NoteSeizureWitnessBinary struct {
	TotalLength                 uint32
	ClaimedStatementHash        [32]byte
	Anchor                      [32]byte
	NoteCommitment              [32]byte
	Nullifier                   [32]byte
	HistoryRequired             bool
	RecentPositionFloor         [32]byte
	AddressDiversifiedGenerator [32]byte
	AddressTransmissionKey      [32]byte
	AssetID                     [32]byte
	Amount                      [32]byte
	AuthorizationCommitment     [32]byte
	Recovery                    NoteSeizureRecoveryWitnessBinary
	NoteBlinding                [32]byte
	Position                    uint64
	StateCommitmentAuthPath     [][3][32]byte
	RNK                         [32]byte
	RnkCommitment               [32]byte
}

func EncodeNoteSeizureWitness(witness *NoteSeizureWitnessBinary) ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(noteSeizureWitnessMagic)
	_ = binary.Write(&buf, binary.LittleEndian, uint32(0))
	for _, value := range [][32]byte{
		witness.ClaimedStatementHash,
		witness.Anchor,
		witness.NoteCommitment,
		witness.Nullifier,
	} {
		buf.Write(value[:])
	}
	if witness.HistoryRequired {
		buf.WriteByte(1)
	} else {
		buf.WriteByte(0)
	}
	for _, value := range [][32]byte{
		witness.RecentPositionFloor,
		witness.AddressDiversifiedGenerator,
		witness.AddressTransmissionKey,
		witness.AssetID,
		witness.Amount,
		witness.AuthorizationCommitment,
		witness.Recovery.Commitment,
	} {
		buf.Write(value[:])
	}
	for _, value := range [][32]byte{
		witness.Recovery.EPKAffine.X,
		witness.Recovery.EPKAffine.Y,
		witness.Recovery.C2,
		witness.Recovery.Salt,
		witness.Recovery.KeyConfirmation,
		witness.Recovery.EncryptedAmount,
		witness.Recovery.EncryptedNoteBlinding,
		witness.Recovery.Seed,
		witness.NoteBlinding,
	} {
		buf.Write(value[:])
	}
	_ = binary.Write(&buf, binary.LittleEndian, witness.Position)
	if len(witness.StateCommitmentAuthPath) != maxTriplePathLength {
		return nil, fmt.Errorf("triple path length %d must equal %d", len(witness.StateCommitmentAuthPath), maxTriplePathLength)
	}
	_ = binary.Write(&buf, binary.LittleEndian, uint32(len(witness.StateCommitmentAuthPath)))
	for _, siblings := range witness.StateCommitmentAuthPath {
		for _, sibling := range siblings {
			buf.Write(sibling[:])
		}
	}
	buf.Write(witness.RNK[:])
	buf.Write(witness.RnkCommitment[:])
	payload := buf.Bytes()
	if len(payload) > int(^uint32(0)) {
		return nil, fmt.Errorf("note seizure witness exceeds u32 length")
	}
	binary.LittleEndian.PutUint32(payload[4:8], uint32(len(payload)))
	return payload, nil
}

func DecodeNoteSeizureWitness(payload []byte) (*NoteSeizureWitnessBinary, error) {
	reader := bytes.NewReader(payload)
	magic, err := readExact(reader, 4)
	if err != nil {
		return nil, err
	}
	if string(magic) != noteSeizureWitnessMagic {
		return nil, fmt.Errorf("invalid note seizure witness magic %q", string(magic))
	}
	totalLength, err := readU32(reader)
	if err != nil {
		return nil, err
	}
	if totalLength != uint32(len(payload)) {
		return nil, fmt.Errorf("note seizure witness length mismatch: header=%d actual=%d", totalLength, len(payload))
	}
	witness := &NoteSeizureWitnessBinary{TotalLength: totalLength}
	for _, target := range []*[32]byte{
		&witness.ClaimedStatementHash,
		&witness.Anchor,
		&witness.NoteCommitment,
		&witness.Nullifier,
	} {
		if *target, err = read32(reader); err != nil {
			return nil, err
		}
	}
	if witness.HistoryRequired, err = readBool(reader); err != nil {
		return nil, err
	}
	for _, target := range []*[32]byte{
		&witness.RecentPositionFloor,
		&witness.AddressDiversifiedGenerator,
		&witness.AddressTransmissionKey,
		&witness.AssetID,
	} {
		if *target, err = read32(reader); err != nil {
			return nil, err
		}
	}
	if witness.Amount, err = readU128Field(reader); err != nil {
		return nil, err
	}
	if witness.AuthorizationCommitment, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Recovery.Commitment, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.Recovery.EPKAffine, err = readPointAffine(reader); err != nil {
		return nil, err
	}
	for _, target := range []*[32]byte{
		&witness.Recovery.C2,
		&witness.Recovery.Salt,
		&witness.Recovery.KeyConfirmation,
		&witness.Recovery.EncryptedAmount,
		&witness.Recovery.EncryptedNoteBlinding,
		&witness.Recovery.Seed,
		&witness.NoteBlinding,
	} {
		if *target, err = read32(reader); err != nil {
			return nil, err
		}
	}
	if witness.Position, err = readU64(reader); err != nil {
		return nil, err
	}
	if witness.StateCommitmentAuthPath, err = readTriplePath(reader); err != nil {
		return nil, err
	}
	if witness.RNK, err = read32(reader); err != nil {
		return nil, err
	}
	if witness.RnkCommitment, err = read32(reader); err != nil {
		return nil, err
	}
	if reader.Len() != 0 {
		return nil, fmt.Errorf("note seizure witness has %d trailing bytes", reader.Len())
	}
	return witness, nil
}
