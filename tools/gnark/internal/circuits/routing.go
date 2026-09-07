package circuits

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"golang.org/x/crypto/blake2b"
)

const routingWordBits = 32

func routingDomain(label string) *big.Int {
	sum := blake2b.Sum512([]byte(label))
	return LittleEndianBytesToBigInt(sum[:])
}

var (
	routeDomain       = routingDomain("shieldd.discovery.route")
	tagRandomDomain   = routingDomain("shieldd.discovery.tag_random")
	permutationDomain = routingDomain("shieldd.discovery.permutation")
	parametersDomain  = routingDomain("shieldd.discovery.parameters")
)

func routingPrecisionBits(
	api frontend.API,
	precision frontend.Variable,
) ([routingWordBits]frontend.Variable, [routingWordBits + 1]frontend.Variable) {
	var matches [routingWordBits + 1]frontend.Variable
	sum := frontend.Variable(0)
	for candidate := 0; candidate <= routingWordBits; candidate++ {
		matches[candidate] = api.IsZero(api.Sub(precision, candidate))
		sum = api.Add(sum, matches[candidate])
	}
	api.AssertIsEqual(sum, 1)

	var active [routingWordBits]frontend.Variable
	for bit := 0; bit < routingWordBits; bit++ {
		active[bit] = 0
		for candidate := bit + 1; candidate <= routingWordBits; candidate++ {
			active[bit] = api.Add(active[bit], matches[candidate])
		}
	}
	return active, matches
}

func constrainOrderedRoutingPrecisions(
	api frontend.API,
	regulatedMatches [routingWordBits + 1]frontend.Variable,
	unregulatedMatches [routingWordBits + 1]frontend.Variable,
) {
	invalid := frontend.Variable(0)
	for regulated := 1; regulated <= routingWordBits; regulated++ {
		for unregulated := 0; unregulated < regulated; unregulated++ {
			invalid = api.Add(
				invalid,
				api.Mul(regulatedMatches[regulated], unregulatedMatches[unregulated]),
			)
		}
	}
	api.AssertIsEqual(invalid, 0)
}

func routingTagForSlot(
	api frontend.API,
	trace func(string, ...string),
	routeWord frontend.Variable,
	activePrecisionBits [routingWordBits]frontend.Variable,
	routingNonce frontend.Variable,
	slot int,
	meaningful frontend.Variable,
	publicTagBits []frontend.Variable,
) error {
	if len(publicTagBits) != routingWordBits {
		return fmt.Errorf("routing tag must expose %d bits, got %d", routingWordBits, len(publicTagBits))
	}
	trace(
		"routing.tag.route_bits",
		fmt.Sprintf("slot=%d", slot),
		"in=route_word",
	)
	routeBits := api.ToBinary(routeWord, 253)
	trace(
		"routing.tag.random_word",
		fmt.Sprintf("slot=%d", slot),
		"nonce=routing_nonce",
		"out=random_word",
	)
	randomWord, err := Poseidon377Hash2(
		api,
		tagRandomDomain,
		[2]frontend.Variable{routingNonce, slot},
	)
	if err != nil {
		return err
	}
	trace(
		"routing.tag.compose",
		fmt.Sprintf("slot=%d", slot),
		"route_word=route_word",
		"random_word=random_word",
		"meaningful=meaningful",
		"out=expected_tag",
	)
	randomBits := api.ToBinary(randomWord, 253)
	for bit := 0; bit < routingWordBits; bit++ {
		withPrefix := api.Select(activePrecisionBits[bit], routeBits[bit], randomBits[bit])
		expectedBit := api.Select(meaningful, withPrefix, randomBits[bit])
		api.AssertIsEqual(expectedBit, publicTagBits[bit])
	}
	return nil
}

func verifyRoutingAssetRegistry(
	api frontend.API,
	trace func(string, ...string),
	bind func(string, ...frontend.Variable),
	asset AssetTreeFields,
	assetAnchor frontend.Variable,
	assetID frontend.Variable,
	isRegulated frontend.Variable,
) error {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return err
	}
	trace("decaf.compress_to_field", "in=asset.leaf.dk_pub", "out=asset.leaf.dk_pub_fq")
	dkPubFq, err := decafgnark.CompressToField(
		api,
		gnarkte.Point{X: asset.Leaf.DKPub.X, Y: asset.Leaf.DKPub.Y},
	)
	if err != nil {
		return err
	}
	bind("asset.leaf.dk_pub_fq", dkPubFq)
	trace(
		"gadget.asset_registry_params_hash",
		"dk_pub_fq=asset.leaf.dk_pub_fq",
		"daily_volume_limit=asset.leaf.daily_volume_limit",
		"route_policy_hash=asset.leaf.route_policy_hash",
		"out=asset.leaf.params_hash",
	)
	paramsHash, err := Poseidon377Hash3(
		api,
		MustBigInt(vectors.Poseidon377.IMTParamsDomain),
		[3]frontend.Variable{
			dkPubFq,
			asset.Leaf.DailyVolumeLimit,
			asset.Leaf.RoutePolicyHash,
		},
	)
	if err != nil {
		return err
	}
	bind("asset.leaf.params_hash", paramsHash)
	trace("decaf.compress_to_field", "in=asset.leaf.ring_pk", "out=asset.leaf.ring_pk_fq")
	ringPKFq, err := decafgnark.CompressToField(
		api,
		gnarkte.Point{X: asset.Leaf.RingPK.X, Y: asset.Leaf.RingPK.Y},
	)
	if err != nil {
		return err
	}
	bind("asset.leaf.ring_pk_fq", ringPKFq)
	trace(
		"gadget.asset_registry_ring_hash",
		"ring_pk_fq=asset.leaf.ring_pk_fq",
		"ring_id_hash=asset.leaf.ring_id_hash",
		"policy_id_hash=asset.leaf.policy_id_hash",
		"permission_hash=asset.leaf.permission_hash",
		"resource_hash=asset.leaf.resource_hash",
		"out=asset.leaf.ring_hash",
	)
	ringHash, err := Poseidon377Hash5(
		api,
		MustBigInt(vectors.Poseidon377.IMTRingDomain),
		[5]frontend.Variable{
			ringPKFq,
			asset.Leaf.RingIDHash,
			asset.Leaf.PolicyIDHash,
			asset.Leaf.PermissionHash,
			asset.Leaf.ResourceHash,
		},
	)
	if err != nil {
		return err
	}
	bind("asset.leaf.ring_hash", ringHash)
	trace(
		"gadget.asset_registry_leaf_hash",
		"value=asset.leaf.value",
		"next_index=asset.leaf.next_index",
		"next_value=asset.leaf.next_value",
		"params_hash=asset.leaf.params_hash",
		"ring_hash=asset.leaf.ring_hash",
		"out=asset.leaf.commitment",
	)
	leafCommitment, err := Poseidon377Hash5(
		api,
		MustBigInt(vectors.Poseidon377.IMTLeafDomain),
		[5]frontend.Variable{
			asset.Leaf.Value,
			asset.Leaf.NextIndex,
			asset.Leaf.NextValue,
			paramsHash,
			ringHash,
		},
	)
	if err != nil {
		return err
	}
	bind("asset.leaf.commitment", leafCommitment)
	trace(
		"gadget.asset_registry_path",
		"leaf=asset.leaf.commitment",
		"path=asset.path",
		"position=asset.position",
		"out=asset.root.computed",
	)
	root, err := VerifyQuadPath(api, leafCommitment, asset.Path, asset.Position)
	if err != nil {
		return err
	}
	bind("asset.root.computed", root)
	trace("assert.eq", "lhs=asset.root.computed", "rhs=asset_anchor")
	api.AssertIsEqual(root, assetAnchor)
	trace(
		"gadget.asset_registry_gap",
		"asset_id=shared.asset_id",
		"is_regulated=is_regulated",
		"value=asset.leaf.value",
		"next_value=asset.leaf.next_value",
		"out=asset.gap_valid",
	)
	validProof := AssetRegistryGap(
		api, assetID, isRegulated, asset.Leaf.Value, asset.Leaf.NextValue,
	)
	bind("asset.gap_valid", validProof)
	trace("assert.eq", "lhs=asset.gap_valid", "rhs=1")
	api.AssertIsEqual(validProof, 1)
	return nil
}

func verifySingleRoutingTag(
	api frontend.API,
	trace func(string, ...string),
	tag frontend.Variable,
	parameterSetID frontend.Variable,
	isRegulated frontend.Variable,
	regulatedPrecision frontend.Variable,
	unregulatedPrecision frontend.Variable,
	asOfHeight frontend.Variable,
	routingNonce frontend.Variable,
	transmissionKeyS frontend.Variable,
) error {
	trace(
		"routing.precision.select",
		"regulated=regulated_precision",
		"unregulated=unregulated_precision",
		"selector=is_regulated",
	)
	regulatedBits, regulatedMatches := routingPrecisionBits(api, regulatedPrecision)
	unregulatedBits, unregulatedMatches := routingPrecisionBits(api, unregulatedPrecision)
	constrainOrderedRoutingPrecisions(api, regulatedMatches, unregulatedMatches)

	trace(
		"routing.parameters.hash",
		"regulated=regulated_precision",
		"unregulated=unregulated_precision",
		"as_of=routing_as_of_height",
		"out=routing_parameter_set_id",
	)
	expectedParameterSetID, err := Poseidon377Hash3(
		api,
		parametersDomain,
		[3]frontend.Variable{regulatedPrecision, unregulatedPrecision, asOfHeight},
	)
	if err != nil {
		return err
	}
	trace(
		"routing.parameters.bind",
		"expected=routing_parameter_set_id.computed",
		"public=routing_parameter_set_id",
	)
	api.AssertIsEqual(expectedParameterSetID, parameterSetID)
	var precisionBits [routingWordBits]frontend.Variable
	for bit := 0; bit < routingWordBits; bit++ {
		precisionBits[bit] = api.Select(isRegulated, regulatedBits[bit], unregulatedBits[bit])
	}
	trace(
		"routing.route_word",
		"transmission_key_s=owner.transmission_fq",
		"out=owner.route_word",
	)
	routeWord, err := Poseidon377Hash1(api, routeDomain, transmissionKeyS)
	if err != nil {
		return err
	}
	trace(
		"routing.tag.public_range",
		"slot=0",
		"tag=routing_tag",
	)
	publicTagBits := api.ToBinary(tag, routingWordBits)
	err = routingTagForSlot(
		api,
		trace,
		routeWord,
		precisionBits,
		routingNonce,
		0,
		1,
		publicTagBits,
	)
	if err != nil {
		return err
	}
	return nil
}

func routingPermutationWord(
	api frontend.API,
	routingNonce frontend.Variable,
) (frontend.Variable, error) {
	return Poseidon377Hash1(api, permutationDomain, routingNonce)
}

func (c *TransferCircuit) verifyTransferRouting(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
) (frontend.Variable, error) {
	c.traceWiring(
		"routing.precision.select",
		"regulated=regulated_precision",
		"unregulated=unregulated_precision",
		"selector=is_regulated",
	)
	regulatedBits, regulatedMatches := routingPrecisionBits(api, c.RegulatedPrecision)
	unregulatedBits, unregulatedMatches := routingPrecisionBits(api, c.UnregulatedPrecision)
	constrainOrderedRoutingPrecisions(api, regulatedMatches, unregulatedMatches)

	c.traceWiring(
		"routing.parameters.hash",
		"regulated=regulated_precision",
		"unregulated=unregulated_precision",
		"as_of=routing_as_of_height",
		"out=routing_parameter_set_id",
	)
	parameterSetID, err := Poseidon377Hash3(
		api,
		parametersDomain,
		[3]frontend.Variable{
			c.RegulatedPrecision,
			c.UnregulatedPrecision,
			c.RoutingAsOfHeight,
		},
	)
	if err != nil {
		return nil, err
	}
	c.traceWiring(
		"routing.parameters.bind",
		"expected=routing_parameter_set_id.computed",
		"public=routing_parameter_set_id",
	)
	api.AssertIsEqual(parameterSetID, c.RoutingParameterSetID)

	var precisionBits [routingWordBits]frontend.Variable
	for bit := 0; bit < routingWordBits; bit++ {
		precisionBits[bit] = api.Select(c.IsRegulated, regulatedBits[bit], unregulatedBits[bit])
	}

	c.traceWiring(
		"routing.route_word",
		"role=sender",
		"transmission_key_s=sender.transmission_fq",
		"out=sender.route_word",
	)
	senderWord, err := Poseidon377Hash1(api, routeDomain, shared.senderTransmissionFq)
	if err != nil {
		return nil, err
	}
	c.traceWiring(
		"routing.route_word",
		"role=receiver",
		"transmission_key_s=receiver.transmission_fq",
		"out=receiver.route_word",
	)
	receiverWord, err := Poseidon377Hash1(api, routeDomain, statementData.receiverTransmissionFq)
	if err != nil {
		return nil, err
	}
	c.traceWiring(
		"routing.permutation.hash",
		"nonce=compliance.transfer_nonce_root",
		"out=permutation_word",
	)
	permutationWord, err := routingPermutationWord(api, c.Compliance.TransferNonceRoot)
	if err != nil {
		return nil, err
	}
	c.traceWiring(
		"routing.permutation.compose",
		"word=permutation_word",
		"out=permutation_bit",
	)
	swapped := api.ToBinary(permutationWord, 253)[0]
	hasChange := api.Sub(1, api.IsZero(c.ChangeOutput.Note.Amount))
	senderMeaningful := api.Sub(
		api.Add(c.IsRegulated, hasChange),
		api.Mul(c.IsRegulated, hasChange),
	)

	slotWords := [2]frontend.Variable{
		api.Select(swapped, receiverWord, senderWord),
		api.Select(swapped, senderWord, receiverWord),
	}
	slotMeaningful := [2]frontend.Variable{
		api.Select(swapped, 1, senderMeaningful),
		api.Select(swapped, senderMeaningful, 1),
	}
	for slot := range c.RoutingTags {
		c.traceWiring(
			"routing.tag.public_range",
			fmt.Sprintf("slot=%d", slot),
			fmt.Sprintf("tag=routing_tag%d", slot),
		)
		publicTagBits := api.ToBinary(c.RoutingTags[slot], routingWordBits)
		err := routingTagForSlot(
			api,
			c.traceWiring,
			slotWords[slot],
			precisionBits,
			c.Compliance.TransferNonceRoot,
			slot,
			slotMeaningful[slot],
			publicTagBits,
		)
		if err != nil {
			return nil, err
		}
	}
	return swapped, nil
}
