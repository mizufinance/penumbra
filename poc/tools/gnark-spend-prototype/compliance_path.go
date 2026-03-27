package prototype

import (
	"encoding/base64"
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
)

const complianceQuadTreeDepth = 16

type indexedLeafInputs struct {
	Value          frontend.Variable
	NextIndex      frontend.Variable
	NextValue      frontend.Variable
	DKPub          gnarkte.Point
	Threshold      frontend.Variable
	ChannelsHash   frontend.Variable
	RingPK         gnarkte.Point
	RingIDHash     frontend.Variable
	PolicyIDHash   frontend.Variable
	PermissionHash frontend.Variable
	ResourceHash   frontend.Variable
}

func littleEndianBytesToBigInt(le []byte) *big.Int {
	be := append([]byte(nil), le...)
	for i, j := 0, len(be)-1; i < j; i, j = i+1, j-1 {
		be[i], be[j] = be[j], be[i]
	}
	return new(big.Int).SetBytes(be)
}

func fqFromBase64String(value string) (*big.Int, error) {
	decoded, err := base64.StdEncoding.DecodeString(value)
	if err != nil {
		return nil, fmt.Errorf("decode base64 fq: %w", err)
	}
	if len(decoded) != 32 {
		return nil, fmt.Errorf("expected 32 fq bytes, got %d", len(decoded))
	}
	return littleEndianBytesToBigInt(decoded), nil
}

func indexedLeafInputsFromFixture(fixture spendFixture) (indexedLeafInputs, error) {
	leaf := fixture.Private.AssetIndexedLeaf

	return indexedLeafInputs{
		Value:     littleEndianBytesToBigInt(leaf.Value),
		NextIndex: leaf.NextIndex,
		NextValue: littleEndianBytesToBigInt(leaf.NextValue),
		DKPub: gnarkte.Point{
			X: mustBigInt(fixture.Private.AssetIndexedLeafDKPubAffine.X),
			Y: mustBigInt(fixture.Private.AssetIndexedLeafDKPubAffine.Y),
		},
		Threshold:    leaf.Threshold.String(),
		ChannelsHash: littleEndianBytesToBigInt(leaf.ChannelsHash),
		RingPK: gnarkte.Point{
			X: mustBigInt(fixture.Private.AssetIndexedLeafRingPKAffine.X),
			Y: mustBigInt(fixture.Private.AssetIndexedLeafRingPKAffine.Y),
		},
		RingIDHash:     littleEndianBytesToBigInt(leaf.RingIDHash),
		PolicyIDHash:   littleEndianBytesToBigInt(leaf.PolicyIDHash),
		PermissionHash: littleEndianBytesToBigInt(leaf.PermissionHash),
		ResourceHash:   littleEndianBytesToBigInt(leaf.ResourceHash),
	}, nil
}

func quadPathFromFixture(path merklePathFixture) ([complianceQuadTreeDepth][3]*big.Int, error) {
	var out [complianceQuadTreeDepth][3]*big.Int
	for i := 0; i < complianceQuadTreeDepth; i++ {
		for j := 0; j < 3; j++ {
			out[i][j] = big.NewInt(0)
		}
	}
	for i, layer := range path.Layers {
		if i >= complianceQuadTreeDepth {
			return out, fmt.Errorf("path has %d layers, max %d", len(path.Layers), complianceQuadTreeDepth)
		}
		if len(layer.Siblings) != 3 {
			return out, fmt.Errorf("layer %d has %d siblings, expected 3", i, len(layer.Siblings))
		}
		for j, sibling := range layer.Siblings {
			value, err := fqFromBase64String(sibling)
			if err != nil {
				return out, fmt.Errorf("decode sibling %d/%d: %w", i, j, err)
			}
			out[i][j] = value
		}
	}
	return out, nil
}

func IndexedLeafCommitmentNative(inputs indexedLeafInputs) (*big.Int, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	dkPubFq, err := Decaf377CompressToFieldNative(inputs.DKPub)
	if err != nil {
		return nil, err
	}
	paramsHash, err := Poseidon377Hash3Native(
		mustBigInt(vectors.Poseidon377.IMTParamsDomain),
		[3]*big.Int{dkPubFq, mustBigInt(inputs.Threshold.(string)), inputs.ChannelsHash.(*big.Int)},
	)
	if err != nil {
		return nil, err
	}

	ringPKFq, err := Decaf377CompressToFieldNative(inputs.RingPK)
	if err != nil {
		return nil, err
	}
	ringHash, err := Poseidon377Hash5Native(
		mustBigInt(vectors.Poseidon377.IMTRingDomain),
		[5]*big.Int{
			ringPKFq,
			inputs.RingIDHash.(*big.Int),
			inputs.PolicyIDHash.(*big.Int),
			inputs.PermissionHash.(*big.Int),
			inputs.ResourceHash.(*big.Int),
		},
	)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash5Native(
		mustBigInt(vectors.Poseidon377.IMTLeafDomain),
		[5]*big.Int{
			inputs.Value.(*big.Int),
			new(big.Int).SetUint64(inputs.NextIndex.(uint64)),
			inputs.NextValue.(*big.Int),
			paramsHash,
			ringHash,
		},
	)
}

func IndexedLeafCommitment(api frontend.API, inputs indexedLeafInputs) (frontend.Variable, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	dkPubFq, err := Decaf377CompressToField(api, inputs.DKPub)
	if err != nil {
		return nil, err
	}
	paramsHash, err := Poseidon377Hash3(
		api,
		mustBigInt(vectors.Poseidon377.IMTParamsDomain),
		[3]frontend.Variable{dkPubFq, inputs.Threshold, inputs.ChannelsHash},
	)
	if err != nil {
		return nil, err
	}

	ringPKFq, err := Decaf377CompressToField(api, inputs.RingPK)
	if err != nil {
		return nil, err
	}
	ringHash, err := Poseidon377Hash5(
		api,
		mustBigInt(vectors.Poseidon377.IMTRingDomain),
		[5]frontend.Variable{
			ringPKFq,
			inputs.RingIDHash,
			inputs.PolicyIDHash,
			inputs.PermissionHash,
			inputs.ResourceHash,
		},
	)
	if err != nil {
		return nil, err
	}

	return Poseidon377Hash5(
		api,
		mustBigInt(vectors.Poseidon377.IMTLeafDomain),
		[5]frontend.Variable{
			inputs.Value,
			inputs.NextIndex,
			inputs.NextValue,
			paramsHash,
			ringHash,
		},
	)
}

func VerifyQuadPathNative(
	leafHash *big.Int,
	path [complianceQuadTreeDepth][3]*big.Int,
	position uint64,
) (*big.Int, error) {
	current := new(big.Int).Set(leafHash)
	for layerIdx := 0; layerIdx < complianceQuadTreeDepth; layerIdx++ {
		bit0 := (position >> (layerIdx * 2)) & 1
		bit1 := (position >> (layerIdx*2 + 1)) & 1
		index := int(bit0 + 2*bit1)

		children := [4]*big.Int{
			new(big.Int).Set(path[layerIdx][0]),
			new(big.Int).Set(path[layerIdx][1]),
			new(big.Int).Set(path[layerIdx][2]),
			new(big.Int).Set(path[layerIdx][2]),
		}
		switch index {
		case 0:
			children = [4]*big.Int{current, path[layerIdx][0], path[layerIdx][1], path[layerIdx][2]}
		case 1:
			children = [4]*big.Int{path[layerIdx][0], current, path[layerIdx][1], path[layerIdx][2]}
		case 2:
			children = [4]*big.Int{path[layerIdx][0], path[layerIdx][1], current, path[layerIdx][2]}
		case 3:
			children = [4]*big.Int{path[layerIdx][0], path[layerIdx][1], path[layerIdx][2], current}
		}

		parent, err := Poseidon377Hash4Native(big.NewInt(0), children)
		if err != nil {
			return nil, err
		}
		current = parent
	}
	return current, nil
}

func VerifyQuadPath(
	api frontend.API,
	leafHash frontend.Variable,
	path [complianceQuadTreeDepth][3]frontend.Variable,
	position frontend.Variable,
) (frontend.Variable, error) {
	current := leafHash
	posBits := api.ToBinary(position, 64)
	for layerIdx := 0; layerIdx < complianceQuadTreeDepth; layerIdx++ {
		bit0 := posBits[layerIdx*2]
		bit1 := posBits[layerIdx*2+1]
		isIndex0 := api.Mul(api.Sub(1, bit0), api.Sub(1, bit1))
		isIndex1 := api.Mul(bit0, api.Sub(1, bit1))
		isIndex2 := api.Mul(api.Sub(1, bit0), bit1)
		isIndex3 := api.Mul(bit0, bit1)

		child0 := api.Select(isIndex0, current, path[layerIdx][0])
		child1Not1 := api.Select(isIndex0, path[layerIdx][0], path[layerIdx][1])
		child1 := api.Select(isIndex1, current, child1Not1)
		child2Not2 := api.Select(bit1, path[layerIdx][2], path[layerIdx][1])
		child2 := api.Select(isIndex2, current, child2Not2)
		child3 := api.Select(isIndex3, current, path[layerIdx][2])

		parent, err := Poseidon377Hash4(
			api,
			0,
			[4]frontend.Variable{child0, child1, child2, child3},
		)
		if err != nil {
			return nil, err
		}
		current = parent
	}
	return current, nil
}
