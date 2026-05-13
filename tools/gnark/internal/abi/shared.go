package abi

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/compliance"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

func pointAffineBinaryToStrings(point PointAffineBinary) primitives.PointAffineFixture {
	return primitives.PointAffineFixture{
		X: primitives.LittleEndianBytesToBigInt(point.X[:]).String(),
		Y: primitives.LittleEndianBytesToBigInt(point.Y[:]).String(),
	}
}

func incomingViewingKeyReductionFromBinary(
	nk [32]byte,
	akCompressed [32]byte,
) (*big.Int, uint64, error) {
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

func indexedLeafFieldsFromIndexedLeafBinary(
	leaf IndexedLeafBinary,
	dkPub PointAffineBinary,
	ringPK PointAffineBinary,
) circuits.IndexedLeafFields {
	return circuits.IndexedLeafFields{
		Value:     primitives.LittleEndianBytesToBigInt(leaf.Value[:]).String(),
		NextIndex: leaf.NextIndex,
		NextValue: primitives.LittleEndianBytesToBigInt(leaf.NextValue[:]).String(),
		DKPub: circuits.Point2D{
			X: primitives.LittleEndianBytesToBigInt(dkPub.X[:]).String(),
			Y: primitives.LittleEndianBytesToBigInt(dkPub.Y[:]).String(),
		},
		Threshold:    primitives.LittleEndianBytesToBigInt(leaf.Threshold[:]).String(),
		ChannelsHash: primitives.LittleEndianBytesToBigInt(leaf.ChannelsHash[:]).String(),
		RingPK: circuits.Point2D{
			X: primitives.LittleEndianBytesToBigInt(ringPK.X[:]).String(),
			Y: primitives.LittleEndianBytesToBigInt(ringPK.Y[:]).String(),
		},
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
