package prototype

import (
	"bytes"
	"encoding/base64"
	"encoding/hex"
	"math/big"
	"testing"
)

func TestSpendWitnessV1DecodesAndMatchesFixture(t *testing.T) {
	fixture, err := loadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}

	witness, err := decodeSpendWitnessV1(loadSpendWitnessV1())
	if err != nil {
		t.Fatalf("decode spend witness v1: %v", err)
	}

	if int(witness.TotalLength) != len(loadSpendWitnessV1()) {
		t.Fatalf("witness length mismatch: got %d want %d", witness.TotalLength, len(loadSpendWitnessV1()))
	}

	if got, want := witness.ClaimedStatementHash[:], decimalToLE32(t, fixture.ClaimedStatementHash); !bytes.Equal(got, want) {
		t.Fatalf("claimed statement hash mismatch")
	}
	if got, want := len(witness.StatementFields), len(fixture.StatementFields); got != want {
		t.Fatalf("statement field count mismatch: got %d want %d", got, want)
	}
	for i, field := range fixture.StatementFields {
		if got, want := witness.StatementFields[i][:], decimalToLE32(t, field); !bytes.Equal(got, want) {
			t.Fatalf("statement field %d mismatch", i)
		}
	}
	if got, want := witness.NoteBlinding[:], decimalToLE32(t, fixture.Private.NoteBlinding); !bytes.Equal(got, want) {
		t.Fatalf("note blinding mismatch")
	}
	if got, want := witness.NoteAmount[:], decimalToLE32(t, fixture.Private.NoteAmount); !bytes.Equal(got, want) {
		t.Fatalf("note amount mismatch")
	}
	if got, want := witness.NoteAssetID[:], decimalToLE32(t, fixture.Private.NoteAssetID); !bytes.Equal(got, want) {
		t.Fatalf("note asset id mismatch")
	}
	if got, want := witness.DiversifiedGenerator[:], decodeHex32(t, fixture.Private.DiversifiedGeneratorHex); !bytes.Equal(got, want) {
		t.Fatalf("diversified generator mismatch")
	}
	if got, want := witness.TransmissionKey[:], decodeHex32(t, fixture.Private.TransmissionKeyHex); !bytes.Equal(got, want) {
		t.Fatalf("transmission key mismatch")
	}
	if got, want := witness.ClueKey[:], decimalToLE32(t, fixture.Private.ClueKey); !bytes.Equal(got, want) {
		t.Fatalf("clue key mismatch")
	}

	noteBytes, err := hex.DecodeString(fixture.Private.NoteBytesHex)
	if err != nil {
		t.Fatalf("decode note bytes hex: %v", err)
	}
	if !bytes.Equal(witness.NoteBytes[:], noteBytes) {
		t.Fatalf("note bytes mismatch")
	}

	if got, want := witness.StateCommitmentPosition, fixture.Private.StateCommitmentProof.Position; got != want {
		t.Fatalf("state commitment position mismatch: got %d want %d", got, want)
	}
	if got, want := len(witness.StateCommitmentAuthPath), len(fixture.Private.StateCommitmentProof.AuthPath); got != want {
		t.Fatalf("state commitment auth path depth mismatch: got %d want %d", got, want)
	}
	for i, siblings := range fixture.Private.StateCommitmentProof.AuthPath {
		for j, sibling := range siblings {
			want := decimalToLE32(t, sibling)
			if got := witness.StateCommitmentAuthPath[i][j][:]; !bytes.Equal(got, want) {
				t.Fatalf("state commitment sibling mismatch at layer=%d index=%d", i, j)
			}
		}
	}

	if got, want := witness.AssetPosition, fixture.Private.AssetPosition; got != want {
		t.Fatalf("asset position mismatch: got %d want %d", got, want)
	}
	if got, want := len(witness.AssetPath.Layers), len(fixture.Private.AssetPath.Layers); got != want {
		t.Fatalf("asset path layer count mismatch: got %d want %d", got, want)
	}
	for i, layer := range fixture.Private.AssetPath.Layers {
		if got, want := len(witness.AssetPath.Layers[i]), len(layer.Siblings); got != want {
			t.Fatalf("asset path sibling count mismatch at layer %d: got %d want %d", i, got, want)
		}
		for j, sibling := range layer.Siblings {
			want := decodeBase64(t, sibling)
			if got := witness.AssetPath.Layers[i][j][:]; !bytes.Equal(got, want) {
				t.Fatalf("asset path sibling mismatch at layer=%d index=%d", i, j)
			}
		}
	}

	if got, want := witness.AssetIndexedLeaf.Value[:], bytesFromArray(fixture.Private.AssetIndexedLeaf.Value); !bytes.Equal(got, want) {
		t.Fatalf("asset indexed leaf value mismatch")
	}
	if got, want := witness.AssetIndexedLeaf.NextIndex, fixture.Private.AssetIndexedLeaf.NextIndex; got != want {
		t.Fatalf("asset indexed leaf next index mismatch: got %d want %d", got, want)
	}
	if got, want := littleEndianToBigInt(witness.AssetIndexedLeaf.Threshold[:]).String(), fixture.Private.AssetIndexedLeaf.Threshold.String(); got != want {
		t.Fatalf("asset indexed leaf threshold mismatch: got %s want %s", got, want)
	}

	if !witness.IsRegulated {
		t.Fatalf("expected regulated witness")
	}
	if got, want := witness.CompliancePosition, fixture.Private.CompliancePosition; got != want {
		t.Fatalf("compliance position mismatch: got %d want %d", got, want)
	}
	if got, want := len(witness.CompliancePath.Layers), len(fixture.Private.CompliancePath.Layers); got != want {
		t.Fatalf("compliance path layer count mismatch: got %d want %d", got, want)
	}

	if got, want := witness.UserAddress[:], decodeBase64(t, fixture.Private.UserLeaf.Address.Inner); !bytes.Equal(got, want) {
		t.Fatalf("user address mismatch")
	}
	if got, want := witness.UserAssetID[:], decodeBase64(t, fixture.Private.UserLeaf.AssetID.Inner); !bytes.Equal(got, want) {
		t.Fatalf("user asset id mismatch")
	}
	if got, want := witness.UserD[:], decodeBase64(t, fixture.Private.UserLeaf.D); !bytes.Equal(got, want) {
		t.Fatalf("user d mismatch")
	}

	if got, want := witness.ComplianceEphemeralSecret[:], decimalFrToLE32(t, fixture.Private.ComplianceEphemeralSecret); !bytes.Equal(got, want) {
		t.Fatalf("compliance ephemeral secret mismatch")
	}
	if got, want := witness.TxBlindingNonce[:], decimalFrToLE32(t, fixture.Private.TxBlindingNonce); !bytes.Equal(got, want) {
		t.Fatalf("tx blinding nonce mismatch")
	}
	if witness.IsFlagged {
		t.Fatalf("expected unflagged witness")
	}
	if got, want := witness.Salt[:], decimalToLE32(t, fixture.Private.Salt); !bytes.Equal(got, want) {
		t.Fatalf("salt mismatch")
	}
	if got, want := witness.BalanceCommitmentAffine.X[:], decimalToLE32(t, fixture.Public.BalanceCommitmentAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("balance commitment affine x mismatch")
	}
	if got, want := witness.BalanceCommitmentAffine.Y[:], decimalToLE32(t, fixture.Public.BalanceCommitmentAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("balance commitment affine y mismatch")
	}
	if got, want := witness.RKAffine.X[:], decimalToLE32(t, fixture.Public.RKAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("rk affine x mismatch")
	}
	if got, want := witness.RKAffine.Y[:], decimalToLE32(t, fixture.Public.RKAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("rk affine y mismatch")
	}
	if got, want := witness.EpkAffine.X[:], decimalToLE32(t, fixture.Public.EpkAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("epk affine x mismatch")
	}
	if got, want := witness.EpkAffine.Y[:], decimalToLE32(t, fixture.Public.EpkAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("epk affine y mismatch")
	}
	if got, want := witness.DiversifiedGeneratorAffine.X[:], decimalToLE32(t, fixture.Private.DiversifiedGeneratorAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("note diversified generator affine x mismatch")
	}
	if got, want := witness.DiversifiedGeneratorAffine.Y[:], decimalToLE32(t, fixture.Private.DiversifiedGeneratorAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("note diversified generator affine y mismatch")
	}
	if got, want := witness.TransmissionKeyAffine.X[:], decimalToLE32(t, fixture.Private.TransmissionKeyAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("transmission key affine x mismatch")
	}
	if got, want := witness.TransmissionKeyAffine.Y[:], decimalToLE32(t, fixture.Private.TransmissionKeyAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("transmission key affine y mismatch")
	}
	if got, want := witness.AKAffine.X[:], decimalToLE32(t, fixture.Private.AKAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("ak affine x mismatch")
	}
	if got, want := witness.AKAffine.Y[:], decimalToLE32(t, fixture.Private.AKAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("ak affine y mismatch")
	}
	if got, want := witness.AssetIndexedLeafDKPub.X[:], decimalToLE32(t, fixture.Private.AssetIndexedLeafDKPubAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("asset indexed leaf dk_pub affine x mismatch")
	}
	if got, want := witness.AssetIndexedLeafDKPub.Y[:], decimalToLE32(t, fixture.Private.AssetIndexedLeafDKPubAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("asset indexed leaf dk_pub affine y mismatch")
	}
	if got, want := witness.AssetIndexedLeafRingPK.X[:], decimalToLE32(t, fixture.Private.AssetIndexedLeafRingPKAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("asset indexed leaf ring_pk affine x mismatch")
	}
	if got, want := witness.AssetIndexedLeafRingPK.Y[:], decimalToLE32(t, fixture.Private.AssetIndexedLeafRingPKAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("asset indexed leaf ring_pk affine y mismatch")
	}
	if got, want := witness.UserDiversifiedGenerator.X[:], decimalToLE32(t, fixture.Private.UserDiversifiedGeneratorAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("user diversified generator affine x mismatch")
	}
	if got, want := witness.UserDiversifiedGenerator.Y[:], decimalToLE32(t, fixture.Private.UserDiversifiedGeneratorAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("user diversified generator affine y mismatch")
	}
	if got, want := witness.UserTransmissionKey.X[:], decimalToLE32(t, fixture.Private.UserTransmissionKeyAffine.X); !bytes.Equal(got, want) {
		t.Fatalf("user transmission key affine x mismatch")
	}
	if got, want := witness.UserTransmissionKey.Y[:], decimalToLE32(t, fixture.Private.UserTransmissionKeyAffine.Y); !bytes.Equal(got, want) {
		t.Fatalf("user transmission key affine y mismatch")
	}
}

func TestSpendWitnessV1RejectsBadHeader(t *testing.T) {
	payload := append([]byte(nil), loadSpendWitnessV1()...)
	payload[0] = 'X'
	if _, err := decodeSpendWitnessV1(payload); err == nil {
		t.Fatalf("expected bad header error")
	}
}

func TestSpendWitnessV1RejectsTruncatedPayload(t *testing.T) {
	payload := append([]byte(nil), loadSpendWitnessV1()...)
	payload = payload[:len(payload)-5]
	if _, err := decodeSpendWitnessV1(payload); err == nil {
		t.Fatalf("expected truncated payload error")
	}
}

func decimalToLE32(t *testing.T, decimal string) []byte {
	t.Helper()
	value, ok := new(big.Int).SetString(decimal, 10)
	if !ok {
		t.Fatalf("invalid decimal field element %q", decimal)
	}
	out := value.Bytes()
	reversed := make([]byte, 32)
	for i := range out {
		reversed[i] = out[len(out)-1-i]
	}
	return reversed
}

func decimalFrToLE32(t *testing.T, decimal string) []byte {
	t.Helper()
	return decimalToLE32(t, decimal)
}

func decodeBase64(t *testing.T, value string) []byte {
	t.Helper()
	out, err := base64.StdEncoding.DecodeString(value)
	if err != nil {
		t.Fatalf("decode base64 %q: %v", value, err)
	}
	return out
}

func decodeHex32(t *testing.T, value string) []byte {
	t.Helper()
	out, err := hex.DecodeString(value)
	if err != nil {
		t.Fatalf("decode hex %q: %v", value, err)
	}
	return out
}

func bytesFromArray(values []byte) []byte {
	return append([]byte(nil), values...)
}

func littleEndianToBigInt(le []byte) *big.Int {
	be := append([]byte(nil), le...)
	for i, j := 0, len(be)-1; i < j; i, j = i+1, j-1 {
		be[i], be[j] = be[j], be[i]
	}
	return new(big.Int).SetBytes(be)
}
