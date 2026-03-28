package prototype

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"math/big"
	"reflect"
	"strings"
)

func DecodeSpendWitnessRawDumpV1(payload []byte) (string, error) {
	witness, err := decodeSpendWitnessV1(payload)
	if err != nil {
		return "", err
	}
	return rawDumpSpendWitnessV1(witness, payload), nil
}

func DumpSpendCircuitAssignmentFromWitnessV1(payload []byte) (string, error) {
	assignment, err := NewSpendCircuitAssignmentFromWitnessV1(payload)
	if err != nil {
		return "", err
	}

	var out strings.Builder
	dumpReflectValue(&out, "assignment", reflect.ValueOf(*assignment))
	return out.String(), nil
}

func rawDumpSpendWitnessV1(witness *spendWitnessV1Binary, payload []byte) string {
	var out strings.Builder

	fmt.Fprintf(&out, "header.magic=%s\n", spendWitnessV1Magic)
	fmt.Fprintf(&out, "header.version=%d\n", spendWitnessV1Version)
	fmt.Fprintf(&out, "header.total_length=%d\n", witness.TotalLength)
	payloadHash := sha256.Sum256(payload)
	fmt.Fprintf(&out, "payload.sha256=%s\n", hex.EncodeToString(payloadHash[:]))

	appendLEBytesLine(&out, "public.anchor", witness.Anchor[:])
	appendPointLine(&out, "public.balance_commitment", witness.BalanceCommitment[:], witness.BalanceCommitmentAffine)
	appendLEBytesLine(&out, "public.nullifier", witness.Nullifier[:])
	appendPointLine(&out, "public.rk", witness.RK[:], witness.RKAffine)
	appendLEBytesLine(&out, "public.asset_anchor", witness.AssetAnchor[:])
	appendLEBytesLine(&out, "public.compliance_anchor", witness.ComplianceAnchor[:])
	appendPointLine(&out, "public.epk", witness.Epk[:], witness.EpkAffine)
	appendLEBytesLine(&out, "public.c2_core", witness.C2Core[:])
	fmt.Fprintf(&out, "public.compliance_ciphertext.len=%d\n", len(witness.ComplianceCiphertext))
	for i := range witness.ComplianceCiphertext {
		appendLEBytesLine(&out, fmt.Sprintf("public.compliance_ciphertext[%d]", i), witness.ComplianceCiphertext[i][:])
	}
	appendLEBytesLine(&out, "public.target_timestamp", witness.TargetTimestamp[:])
	appendLEBytesLine(&out, "public.dleq_c", witness.DleqC[:])
	appendLEBytesLine(&out, "public.dleq_s", witness.DleqS[:])
	appendLEBytesLine(&out, "public.sender_leaf_hash", witness.SenderLeafHash[:])
	appendLEBytesLine(&out, "public.claimed_statement_hash", witness.ClaimedStatementHash[:])
	fmt.Fprintf(&out, "public.statement_fields.len=%d\n", len(witness.StatementFields))
	for i := range witness.StatementFields {
		appendLEBytesLine(&out, fmt.Sprintf("public.statement_fields[%d]", i), witness.StatementFields[i][:])
	}

	appendLEBytesLine(&out, "private.note_blinding", witness.NoteBlinding[:])
	appendLEBytesLine(&out, "private.note_amount", witness.NoteAmount[:])
	appendLEBytesLine(&out, "private.note_asset_id", witness.NoteAssetID[:])
	appendPointLine(&out, "private.note.diversified_generator", witness.DiversifiedGenerator[:], witness.DiversifiedGeneratorAffine)
	appendPointLine(&out, "private.note.transmission_key", witness.TransmissionKey[:], witness.TransmissionKeyAffine)
	appendLEBytesLine(&out, "private.note.clue_key", witness.ClueKey[:])
	fmt.Fprintf(&out, "private.note.note_bytes.hex=%s\n", hex.EncodeToString(witness.NoteBytes[:]))
	appendLEBytesLine(&out, "private.state_commitment.commitment", witness.StateCommitmentCommitment[:])
	fmt.Fprintf(&out, "private.state_commitment.position=%d\n", witness.StateCommitmentPosition)
	fmt.Fprintf(&out, "private.state_commitment.auth_path.len=%d\n", len(witness.StateCommitmentAuthPath))
	for i := range witness.StateCommitmentAuthPath {
		for j := 0; j < len(witness.StateCommitmentAuthPath[i]); j++ {
			appendLEBytesLine(
				&out,
				fmt.Sprintf("private.state_commitment.auth_path[%d][%d]", i, j),
				witness.StateCommitmentAuthPath[i][j][:],
			)
		}
	}
	appendLEBytesLine(&out, "private.v_blinding", witness.VBlinding[:])
	appendLEBytesLine(&out, "private.spend_auth_randomizer", witness.SpendAuthRandomizer[:])
	appendPointLine(&out, "private.ak", witness.AK[:], witness.AKAffine)
	appendLEBytesLine(&out, "private.nk", witness.NK[:])

	appendMerklePathLines(&out, "private.asset_path", witness.AssetPath)
	fmt.Fprintf(&out, "private.asset_position=%d\n", witness.AssetPosition)
	appendLEBytesLine(&out, "private.asset_indexed_leaf.value", witness.AssetIndexedLeaf.Value[:])
	fmt.Fprintf(&out, "private.asset_indexed_leaf.next_index=%d\n", witness.AssetIndexedLeaf.NextIndex)
	appendLEBytesLine(&out, "private.asset_indexed_leaf.next_value", witness.AssetIndexedLeaf.NextValue[:])
	appendPointLine(&out, "private.asset_indexed_leaf.dk_pub", witness.AssetIndexedLeaf.DKPub[:], witness.AssetIndexedLeafDKPub)
	appendLEBytesLine(&out, "private.asset_indexed_leaf.threshold", witness.AssetIndexedLeaf.Threshold[:])
	appendLEBytesLine(&out, "private.asset_indexed_leaf.channels_hash", witness.AssetIndexedLeaf.ChannelsHash[:])
	appendPointLine(&out, "private.asset_indexed_leaf.ring_pk", witness.AssetIndexedLeaf.RingPK[:], witness.AssetIndexedLeafRingPK)
	appendLEBytesLine(&out, "private.asset_indexed_leaf.ring_id_hash", witness.AssetIndexedLeaf.RingIDHash[:])
	appendLEBytesLine(&out, "private.asset_indexed_leaf.policy_id_hash", witness.AssetIndexedLeaf.PolicyIDHash[:])
	appendLEBytesLine(&out, "private.asset_indexed_leaf.permission_hash", witness.AssetIndexedLeaf.PermissionHash[:])
	appendLEBytesLine(&out, "private.asset_indexed_leaf.resource_hash", witness.AssetIndexedLeaf.ResourceHash[:])
	fmt.Fprintf(&out, "private.is_regulated=%d\n", boolToUint8(witness.IsRegulated))

	appendMerklePathLines(&out, "private.compliance_path", witness.CompliancePath)
	fmt.Fprintf(&out, "private.compliance_position=%d\n", witness.CompliancePosition)
	fmt.Fprintf(&out, "private.user_leaf.address.hex=%s\n", hex.EncodeToString(witness.UserAddress[:]))
	appendLEBytesLine(&out, "private.user_leaf.asset_id", witness.UserAssetID[:])
	appendLEBytesLine(&out, "private.user_leaf.d", witness.UserD[:])
	appendPointLine(&out, "private.user_leaf.diversified_generator", witness.UserDiversifiedGenerator.X[:0], witness.UserDiversifiedGenerator)
	appendPointLine(&out, "private.user_leaf.transmission_key", witness.UserTransmissionKey.X[:0], witness.UserTransmissionKey)
	appendLEBytesLine(&out, "private.compliance_ephemeral_secret", witness.ComplianceEphemeralSecret[:])
	appendLEBytesLine(&out, "private.tx_blinding_nonce", witness.TxBlindingNonce[:])
	fmt.Fprintf(&out, "private.is_flagged=%d\n", boolToUint8(witness.IsFlagged))
	appendLEBytesLine(&out, "private.salt", witness.Salt[:])

	return out.String()
}

func appendLEBytesLine(out *strings.Builder, key string, bytes []byte) {
	fmt.Fprintf(out, "%s.le_hex=%s\n", key, hex.EncodeToString(bytes))
	fmt.Fprintf(out, "%s.dec=%s\n", key, littleEndianBytesToBigInt(bytes).String())
}

func appendPointLine(out *strings.Builder, key string, encoding []byte, point pointAffineBinary) {
	if len(encoding) != 0 {
		fmt.Fprintf(out, "%s.encoding_hex=%s\n", key, hex.EncodeToString(encoding))
	}
	appendLEBytesLine(out, key+".x", point.X[:])
	appendLEBytesLine(out, key+".y", point.Y[:])
}

func appendMerklePathLines(out *strings.Builder, key string, path merklePathBinary) {
	fmt.Fprintf(out, "%s.layers=%d\n", key, len(path.Layers))
	for i := range path.Layers {
		fmt.Fprintf(out, "%s[%d].siblings=%d\n", key, i, len(path.Layers[i]))
		for j := range path.Layers[i] {
			fmt.Fprintf(
				out,
				"%s[%d][%d].hex=%s\n",
				key,
				i,
				j,
				hex.EncodeToString(path.Layers[i][j][:]),
			)
		}
	}
}

func boolToUint8(value bool) uint8 {
	if value {
		return 1
	}
	return 0
}

func dumpReflectValue(out *strings.Builder, prefix string, value reflect.Value) {
	switch value.Kind() {
	case reflect.Array:
		for i := 0; i < value.Len(); i++ {
			dumpReflectValue(out, fmt.Sprintf("%s[%d]", prefix, i), value.Index(i))
		}
	case reflect.String:
		fmt.Fprintf(out, "%s=%s\n", prefix, value.String())
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		fmt.Fprintf(out, "%s=%d\n", prefix, value.Uint())
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		fmt.Fprintf(out, "%s=%d\n", prefix, value.Int())
	case reflect.Bool:
		if value.Bool() {
			fmt.Fprintf(out, "%s=1\n", prefix)
		} else {
			fmt.Fprintf(out, "%s=0\n", prefix)
		}
	case reflect.Struct:
		for i := 0; i < value.NumField(); i++ {
			field := value.Type().Field(i)
			dumpReflectValue(out, prefix+"."+field.Name, value.Field(i))
		}
	case reflect.Interface:
		if value.IsNil() {
			fmt.Fprintf(out, "%s=<nil>\n", prefix)
			return
		}
		dumpReflectValue(out, prefix, value.Elem())
	default:
		if value.CanInterface() {
			switch v := value.Interface().(type) {
			case *big.Int:
				fmt.Fprintf(out, "%s=%s\n", prefix, v.String())
			default:
				fmt.Fprintf(out, "%s=%v\n", prefix, v)
			}
			return
		}
		fmt.Fprintf(out, "%s=%v\n", prefix, value)
	}
}
