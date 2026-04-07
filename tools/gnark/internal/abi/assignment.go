package abi

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/circuits"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

func pointAffineBinaryToStrings(point PointAffineBinary) primitives.PointAffineFixture {
	return primitives.PointAffineFixture{
		X: primitives.LittleEndianBytesToBigInt(point.X[:]).String(),
		Y: primitives.LittleEndianBytesToBigInt(point.Y[:]).String(),
	}
}

func incomingViewingKeyReductionFromBinary(nk [32]byte, akCompressed [32]byte) (*big.Int, uint64, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return nil, 0, err
	}
	ivkModQ, err := primitives.Poseidon377Hash2Native(
		primitives.MustBigInt(vectors.Poseidon377.IVKDomain),
		[2]*big.Int{
			primitives.LittleEndianBytesToBigInt(nk[:]),
			primitives.LittleEndianBytesToBigInt(akCompressed[:]),
		},
	)
	if err != nil {
		return nil, 0, err
	}
	rModulus := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order)
	ivkModR := new(big.Int).Mod(new(big.Int).Set(ivkModQ), rModulus)
	quotient := new(big.Int).Sub(ivkModQ, ivkModR)
	quotient.Div(quotient, rModulus)
	quotientUint64, err := quotientAsUint64(quotient)
	if err != nil {
		return nil, 0, err
	}
	return ivkModR, quotientUint64, nil
}

func quotientAsUint64(quotient *big.Int) (uint64, error) {
	if !quotient.IsUint64() {
		return 0, fmt.Errorf("ivk reduction quotient %s does not fit in uint64", quotient.String())
	}
	return quotient.Uint64(), nil
}

func indexedLeafInputsFromBinary(witness *SpendWitnessV1Binary) compliance.IndexedLeafInputs {
	return compliance.IndexedLeafInputs{
		Value:          primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.Value[:]).String(),
		NextIndex:      witness.AssetIndexedLeaf.NextIndex,
		NextValue:      primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.NextValue[:]).String(),
		DKPub:          circuits.PointAffineToNative(pointAffineBinaryToStrings(witness.AssetIndexedLeafDKPub)),
		Threshold:      primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.Threshold[:]).String(),
		ChannelsHash:   primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.ChannelsHash[:]).String(),
		RingPK:         circuits.PointAffineToNative(pointAffineBinaryToStrings(witness.AssetIndexedLeafRingPK)),
		RingIDHash:     primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.RingIDHash[:]).String(),
		PolicyIDHash:   primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.PolicyIDHash[:]).String(),
		PermissionHash: primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.PermissionHash[:]).String(),
		ResourceHash:   primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.ResourceHash[:]).String(),
	}
}

func indexedLeafInputsFromIndexedLeafBinary(
	leaf IndexedLeafBinary,
	dkPub PointAffineBinary,
	ringPK PointAffineBinary,
) compliance.IndexedLeafInputs {
	return compliance.IndexedLeafInputs{
		Value:          primitives.LittleEndianBytesToBigInt(leaf.Value[:]).String(),
		NextIndex:      leaf.NextIndex,
		NextValue:      primitives.LittleEndianBytesToBigInt(leaf.NextValue[:]).String(),
		DKPub:          circuits.PointAffineToNative(pointAffineBinaryToStrings(dkPub)),
		Threshold:      primitives.LittleEndianBytesToBigInt(leaf.Threshold[:]).String(),
		ChannelsHash:   primitives.LittleEndianBytesToBigInt(leaf.ChannelsHash[:]).String(),
		RingPK:         circuits.PointAffineToNative(pointAffineBinaryToStrings(ringPK)),
		RingIDHash:     primitives.LittleEndianBytesToBigInt(leaf.RingIDHash[:]).String(),
		PolicyIDHash:   primitives.LittleEndianBytesToBigInt(leaf.PolicyIDHash[:]).String(),
		PermissionHash: primitives.LittleEndianBytesToBigInt(leaf.PermissionHash[:]).String(),
		ResourceHash:   primitives.LittleEndianBytesToBigInt(leaf.ResourceHash[:]).String(),
	}
}

func statePathFromBinary(path [][3][32]byte) ([circuits.StateCommitmentDepth][3]frontend.Variable, error) {
	var out [circuits.StateCommitmentDepth][3]frontend.Variable
	for i := 0; i < circuits.StateCommitmentDepth; i++ {
		for j := 0; j < 3; j++ {
			out[i][j] = 0
		}
	}
	if len(path) > circuits.StateCommitmentDepth {
		return out, fmt.Errorf("state path has %d layers, max %d", len(path), circuits.StateCommitmentDepth)
	}
	for i, siblings := range path {
		for j := 0; j < 3; j++ {
			out[i][j] = primitives.LittleEndianBytesToBigInt(siblings[j][:]).String()
		}
	}
	return out, nil
}

func quadPathFromBinary(path MerklePathBinary) ([compliance.ComplianceQuadTreeDepth][3]frontend.Variable, error) {
	var out [compliance.ComplianceQuadTreeDepth][3]frontend.Variable
	for i := 0; i < compliance.ComplianceQuadTreeDepth; i++ {
		for j := 0; j < 3; j++ {
			out[i][j] = 0
		}
	}
	for i, layer := range path.Layers {
		if i >= compliance.ComplianceQuadTreeDepth {
			return out, fmt.Errorf("path has %d layers, max %d", len(path.Layers), compliance.ComplianceQuadTreeDepth)
		}
		if len(layer) != 3 {
			return out, fmt.Errorf("layer %d has %d siblings, expected 3", i, len(layer))
		}
		for j, sibling := range layer {
			out[i][j] = primitives.LittleEndianBytesToBigInt(sibling[:])
		}
	}
	return out, nil
}

func zeroQuadPath() [compliance.ComplianceQuadTreeDepth][3]frontend.Variable {
	var out [compliance.ComplianceQuadTreeDepth][3]frontend.Variable
	for i := 0; i < compliance.ComplianceQuadTreeDepth; i++ {
		for j := 0; j < 3; j++ {
			out[i][j] = 0
		}
	}
	return out
}

func noteFields(
	blinding, amount, assetID frontend.Variable,
	divGenX, divGenY frontend.Variable,
	transmissionKeyS frontend.Variable,
	transX, transY frontend.Variable,
	clueKey frontend.Variable,
) circuits.NoteFields {
	return circuits.NoteFields{
		Blinding:         blinding,
		Amount:           amount,
		AssetID:          assetID,
		DivGen:           circuits.Point2D{X: divGenX, Y: divGenY},
		TransmissionKeyS: transmissionKeyS,
		Transmission:     circuits.Point2D{X: transX, Y: transY},
		ClueKey:          clueKey,
	}
}

func indexedLeafFields(
	value, nextValue, threshold, channelsHash frontend.Variable,
	nextIndex frontend.Variable,
	dkPubX, dkPubY frontend.Variable,
	ringPKX, ringPKY frontend.Variable,
	ringIDHash, policyIDHash, permissionHash, resourceHash frontend.Variable,
) circuits.IndexedLeafFields {
	return circuits.IndexedLeafFields{
		Value:          value,
		NextIndex:      nextIndex,
		NextValue:      nextValue,
		DKPub:          circuits.Point2D{X: dkPubX, Y: dkPubY},
		Threshold:      threshold,
		ChannelsHash:   channelsHash,
		RingPK:         circuits.Point2D{X: ringPKX, Y: ringPKY},
		RingIDHash:     ringIDHash,
		PolicyIDHash:   policyIDHash,
		PermissionHash: permissionHash,
		ResourceHash:   resourceHash,
	}
}

func userComplianceFields(
	divGenX, divGenY frontend.Variable,
	transX, transY frontend.Variable,
	assetID, d frontend.Variable,
	path [compliance.ComplianceQuadTreeDepth][3]frontend.Variable,
	position frontend.Variable,
) circuits.UserComplianceFields {
	return circuits.UserComplianceFields{
		DivGen:       circuits.Point2D{X: divGenX, Y: divGenY},
		Transmission: circuits.Point2D{X: transX, Y: transY},
		AssetID:      assetID,
		D:            d,
		Path:         path,
		Position:     position,
	}
}

func NewSpendCircuitAssignmentFromWitnessV1(payload []byte) (*circuits.SpendCircuit, error) {
	witness, err := DecodeSpendWitnessV1(payload)
	if err != nil {
		return nil, fmt.Errorf("decode SpendWitnessV1: %w", err)
	}
	return newSpendCircuitAssignment(witness)
}

func newSpendCircuitAssignment(witness *SpendWitnessV1Binary) (*circuits.SpendCircuit, error) {
	if len(witness.ComplianceCiphertext) != circuits.SpendCiphertextFQCount {
		return nil, fmt.Errorf("expected %d spend ciphertext elements, got %d", circuits.SpendCiphertextFQCount, len(witness.ComplianceCiphertext))
	}
	if len(witness.StatementFields) != primitives.SpendStatementFieldCount {
		return nil, fmt.Errorf("expected %d spend statement fields, got %d", primitives.SpendStatementFieldCount, len(witness.StatementFields))
	}
	statePath, err := statePathFromBinary(witness.StateCommitmentAuthPath)
	if err != nil {
		return nil, fmt.Errorf("decode state commitment auth path: %w", err)
	}
	assetPath, err := quadPathFromBinary(witness.AssetPath)
	if err != nil {
		return nil, fmt.Errorf("decode spend asset path: %w", err)
	}
	compliancePath, err := quadPathFromBinary(witness.CompliancePath)
	if err != nil {
		return nil, fmt.Errorf("decode spend compliance path: %w", err)
	}
	indexedLeaf := indexedLeafInputsFromBinary(witness)
	ivkReduced, quotientA, err := incomingViewingKeyReductionFromBinary(witness.NK, witness.AK)
	if err != nil {
		return nil, fmt.Errorf("compute ivk reduction from binary witness: %w", err)
	}
	assignment := &circuits.SpendCircuit{
		ClaimedStatementHash: primitives.LittleEndianBytesToBigInt(witness.ClaimedStatementHash[:]).String(),
		Anchor:               primitives.LittleEndianBytesToBigInt(witness.Anchor[:]).String(),
		BalanceCommitment: circuits.Point2D{
			X: primitives.LittleEndianBytesToBigInt(witness.BalanceCommitmentAffine.X[:]).String(),
			Y: primitives.LittleEndianBytesToBigInt(witness.BalanceCommitmentAffine.Y[:]).String(),
		},
		Nullifier:        primitives.LittleEndianBytesToBigInt(witness.Nullifier[:]).String(),
		RK:               circuits.Point2D{X: primitives.LittleEndianBytesToBigInt(witness.RKAffine.X[:]).String(), Y: primitives.LittleEndianBytesToBigInt(witness.RKAffine.Y[:]).String()},
		AssetAnchor:      primitives.LittleEndianBytesToBigInt(witness.AssetAnchor[:]).String(),
		ComplianceAnchor: primitives.LittleEndianBytesToBigInt(witness.ComplianceAnchor[:]).String(),
		TargetTimestamp:  primitives.LittleEndianBytesToBigInt(witness.TargetTimestamp[:]).String(),
		SenderLeafHash:   primitives.LittleEndianBytesToBigInt(witness.SenderLeafHash[:]).String(),
		StateProof: circuits.StateCommitmentFields{
			Commitment: primitives.LittleEndianBytesToBigInt(witness.StateCommitmentCommitment[:]).String(),
			Position:   witness.StateCommitmentPosition,
			Path:       statePath,
		},
		Note: noteFields(
			primitives.LittleEndianBytesToBigInt(witness.NoteBlinding[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.NoteAmount[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.NoteAssetID[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.DiversifiedGeneratorAffine.X[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.DiversifiedGeneratorAffine.Y[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.TransmissionKey[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.TransmissionKeyAffine.X[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.TransmissionKeyAffine.Y[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.ClueKey[:]).String(),
		),
		Auth: circuits.SpendAuthFields{
			VBlinding:    primitives.LittleEndianBytesToBigInt(witness.VBlinding[:]).String(),
			Randomizer:   primitives.LittleEndianBytesToBigInt(witness.SpendAuthRandomizer[:]).String(),
			AK:           circuits.Point2D{X: primitives.LittleEndianBytesToBigInt(witness.AKAffine.X[:]).String(), Y: primitives.LittleEndianBytesToBigInt(witness.AKAffine.Y[:]).String()},
			NK:           primitives.LittleEndianBytesToBigInt(witness.NK[:]).String(),
			IVKReduced:   ivkReduced.String(),
			IVKQuotientA: quotientA,
		},
		Asset: circuits.AssetTreeFields{
			Leaf: indexedLeafFields(
				indexedLeaf.Value,
				indexedLeaf.NextValue,
				indexedLeaf.Threshold,
				indexedLeaf.ChannelsHash,
				indexedLeaf.NextIndex,
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeafDKPub.X[:]).String(),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeafDKPub.Y[:]).String(),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeafRingPK.X[:]).String(),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeafRingPK.Y[:]).String(),
				indexedLeaf.RingIDHash,
				indexedLeaf.PolicyIDHash,
				indexedLeaf.PermissionHash,
				indexedLeaf.ResourceHash,
			),
			Path:     assetPath,
			Position: witness.AssetPosition,
		},
		User: userComplianceFields(
			primitives.LittleEndianBytesToBigInt(witness.UserDiversifiedGenerator.X[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.UserDiversifiedGenerator.Y[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.UserTransmissionKey.X[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.UserTransmissionKey.Y[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.UserAssetID[:]).String(),
			primitives.LittleEndianBytesToBigInt(witness.UserD[:]).String(),
			compliancePath,
			witness.CompliancePosition,
		),
		Enc: circuits.SpendEncryptionFields{
			Epk: circuits.Point2D{
				X: primitives.LittleEndianBytesToBigInt(witness.EpkAffine.X[:]).String(),
				Y: primitives.LittleEndianBytesToBigInt(witness.EpkAffine.Y[:]).String(),
			},
			C2Core:              primitives.LittleEndianBytesToBigInt(witness.C2Core[:]).String(),
			IsRegulated:         circuits.BoolToField(witness.IsRegulated),
			IsFlagged:           circuits.BoolToField(witness.IsFlagged),
			ComplianceEphemeral: primitives.LittleEndianBytesToBigInt(witness.ComplianceEphemeralSecret[:]).String(),
			Salt:                primitives.LittleEndianBytesToBigInt(witness.Salt[:]).String(),
			TxBlindingNonce:     primitives.LittleEndianBytesToBigInt(witness.TxBlindingNonce[:]).String(),
		},
		Dleq: circuits.DLEQFields{
			C: primitives.LittleEndianBytesToBigInt(witness.DleqC[:]).String(),
			S: primitives.LittleEndianBytesToBigInt(witness.DleqS[:]).String(),
		},
	}
	for i := range assignment.Enc.ComplianceCiphertext {
		assignment.Enc.ComplianceCiphertext[i] = primitives.LittleEndianBytesToBigInt(witness.ComplianceCiphertext[i][:]).String()
	}
	return assignment, nil
}

func NewOutputCircuitAssignmentFromWitnessV1(payload []byte) (*circuits.OutputCircuit, error) {
	witness, err := DecodeOutputWitnessV1(payload)
	if err != nil {
		return nil, err
	}
	return newOutputCircuitAssignment(witness)
}

func newOutputCircuitAssignment(witness *OutputWitnessV1Binary) (*circuits.OutputCircuit, error) {
	if len(witness.ComplianceCiphertext) != compliance.OutputCiphertextFQCount {
		return nil, fmt.Errorf("expected %d output ciphertext elements, got %d", compliance.OutputCiphertextFQCount, len(witness.ComplianceCiphertext))
	}
	if len(witness.StatementFields) != primitives.OutputStatementFieldCount {
		return nil, fmt.Errorf("expected %d output statement fields, got %d", primitives.OutputStatementFieldCount, len(witness.StatementFields))
	}
	assetPath, err := quadPathFromBinary(witness.AssetPath)
	if err != nil {
		return nil, fmt.Errorf("decode output asset path: %w", err)
	}
	compliancePath, err := quadPathFromBinary(witness.CompliancePath)
	if err != nil {
		return nil, fmt.Errorf("decode output compliance path: %w", err)
	}
	assignment := &circuits.OutputCircuit{
		ClaimedStatementHash:  primitives.LittleEndianBytesToBigInt(witness.ClaimedStatementHash[:]),
		ClaimedNoteCommitment: primitives.LittleEndianBytesToBigInt(witness.NoteCommitment[:]),
		BalanceCommitment: circuits.Point2D{
			X: primitives.LittleEndianBytesToBigInt(witness.BalanceCommitmentXY.X[:]),
			Y: primitives.LittleEndianBytesToBigInt(witness.BalanceCommitmentXY.Y[:]),
		},
		AssetAnchor:          primitives.LittleEndianBytesToBigInt(witness.AssetAnchor[:]),
		ComplianceAnchor:     primitives.LittleEndianBytesToBigInt(witness.ComplianceAnchor[:]),
		TargetTimestamp:      primitives.LittleEndianBytesToBigInt(witness.TargetTimestamp[:]),
		CounterpartyLeafHash: primitives.LittleEndianBytesToBigInt(witness.CounterpartyLeafHash[:]),
		Note: noteFields(
			primitives.LittleEndianBytesToBigInt(witness.NoteBlinding[:]),
			primitives.LittleEndianBytesToBigInt(witness.NoteAmount[:]),
			primitives.LittleEndianBytesToBigInt(witness.NoteAssetID[:]),
			primitives.LittleEndianBytesToBigInt(witness.NoteDivGenAffine.X[:]),
			primitives.LittleEndianBytesToBigInt(witness.NoteDivGenAffine.Y[:]),
			primitives.LittleEndianBytesToBigInt(witness.TransmissionKey[:]),
			primitives.LittleEndianBytesToBigInt(witness.NoteTransmissionAffine.X[:]),
			primitives.LittleEndianBytesToBigInt(witness.NoteTransmissionAffine.Y[:]),
			primitives.LittleEndianBytesToBigInt(witness.ClueKey[:]),
		),
		BalanceBlinding: primitives.LittleEndianBytesToBigInt(witness.BalanceBlinding[:]),
		Asset: circuits.AssetTreeFields{
			Leaf: indexedLeafFields(
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.Value[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.NextValue[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.Threshold[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.ChannelsHash[:]),
				witness.AssetIndexedLeaf.NextIndex,
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeafDKPub.X[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeafDKPub.Y[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeafRingPK.X[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeafRingPK.Y[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.RingIDHash[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.PolicyIDHash[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.PermissionHash[:]),
				primitives.LittleEndianBytesToBigInt(witness.AssetIndexedLeaf.ResourceHash[:]),
			),
			Path:     assetPath,
			Position: witness.AssetPosition,
		},
		User: userComplianceFields(
			primitives.LittleEndianBytesToBigInt(witness.UserDiversifiedGenerator.X[:]),
			primitives.LittleEndianBytesToBigInt(witness.UserDiversifiedGenerator.Y[:]),
			primitives.LittleEndianBytesToBigInt(witness.UserTransmissionKey.X[:]),
			primitives.LittleEndianBytesToBigInt(witness.UserTransmissionKey.Y[:]),
			primitives.LittleEndianBytesToBigInt(witness.UserAssetID[:]),
			primitives.LittleEndianBytesToBigInt(witness.UserD[:]),
			compliancePath,
			witness.CompliancePosition,
		),
		Counterparty: userComplianceFields(
			primitives.LittleEndianBytesToBigInt(witness.CounterpartyDiversifiedGenerator.X[:]),
			primitives.LittleEndianBytesToBigInt(witness.CounterpartyDiversifiedGenerator.Y[:]),
			primitives.LittleEndianBytesToBigInt(witness.CounterpartyTransmissionKey.X[:]),
			primitives.LittleEndianBytesToBigInt(witness.CounterpartyTransmissionKey.Y[:]),
			primitives.LittleEndianBytesToBigInt(witness.CounterpartyAssetID[:]),
			primitives.LittleEndianBytesToBigInt(witness.CounterpartyD[:]),
			zeroQuadPath(),
			0,
		),
		Enc: circuits.OutputEncryptionFields{
			Epk1:                circuits.Point2D{X: primitives.LittleEndianBytesToBigInt(witness.Epk1Affine.X[:]), Y: primitives.LittleEndianBytesToBigInt(witness.Epk1Affine.Y[:])},
			Epk2:                circuits.Point2D{X: primitives.LittleEndianBytesToBigInt(witness.Epk2Affine.X[:]), Y: primitives.LittleEndianBytesToBigInt(witness.Epk2Affine.Y[:])},
			Epk3:                circuits.Point2D{X: primitives.LittleEndianBytesToBigInt(witness.Epk3Affine.X[:]), Y: primitives.LittleEndianBytesToBigInt(witness.Epk3Affine.Y[:])},
			C2Core:              primitives.LittleEndianBytesToBigInt(witness.C2Core[:]),
			C2Ext:               primitives.LittleEndianBytesToBigInt(witness.C2Ext[:]),
			C2Sext:              primitives.LittleEndianBytesToBigInt(witness.C2Sext[:]),
			IsRegulated:         circuits.BoolToField(witness.IsRegulated),
			IsFlagged:           circuits.BoolToField(witness.IsFlagged),
			ComplianceEphemeral: primitives.LittleEndianBytesToBigInt(witness.ComplianceEphemeral[:]),
			R2:                  primitives.LittleEndianBytesToBigInt(witness.R2[:]),
			R3:                  primitives.LittleEndianBytesToBigInt(witness.R3[:]),
			TxBlindingNonce:     primitives.LittleEndianBytesToBigInt(witness.TxBlindingNonce[:]),
			Salt:                primitives.LittleEndianBytesToBigInt(witness.Salt[:]),
		},
		Dleq1: circuits.DLEQFields{
			C: primitives.LittleEndianBytesToBigInt(witness.DleqC1[:]),
			S: primitives.LittleEndianBytesToBigInt(witness.DleqS1[:]),
		},
		Dleq2: circuits.DLEQFields{
			C: primitives.LittleEndianBytesToBigInt(witness.DleqC2[:]),
			S: primitives.LittleEndianBytesToBigInt(witness.DleqS2[:]),
		},
		Dleq3: circuits.DLEQFields{
			C: primitives.LittleEndianBytesToBigInt(witness.DleqC3[:]),
			S: primitives.LittleEndianBytesToBigInt(witness.DleqS3[:]),
		},
	}
	for i := range witness.ComplianceCiphertext {
		assignment.Enc.ComplianceCiphertext[i] = primitives.LittleEndianBytesToBigInt(witness.ComplianceCiphertext[i][:])
	}
	return assignment, nil
}
