package prototype

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
)

const (
	spendCiphertextFQCount = 5
	stateCommitmentDepth   = 24
)

type SpendCircuit struct {
	ClaimedStatementHash frontend.Variable `gnark:",public"`

	Anchor                  frontend.Variable
	BalanceCommitmentX      frontend.Variable
	BalanceCommitmentY      frontend.Variable
	Nullifier               frontend.Variable
	RKX                     frontend.Variable
	RKY                     frontend.Variable
	AssetAnchor             frontend.Variable
	ComplianceAnchor        frontend.Variable
	EpkX                    frontend.Variable
	EpkY                    frontend.Variable
	C2Core                  frontend.Variable
	ComplianceCiphertext    [spendCiphertextFQCount]frontend.Variable
	TargetTimestamp         frontend.Variable
	DleqC                   frontend.Variable
	DleqS                   frontend.Variable
	SenderLeafHash          frontend.Variable
	ExpectedStateCommitment frontend.Variable

	NoteBlinding     frontend.Variable
	NoteAmount       frontend.Variable
	NoteAssetID      frontend.Variable
	DiversifiedGenX  frontend.Variable
	DiversifiedGenY  frontend.Variable
	TransmissionKeyS frontend.Variable
	TransmissionKeyX frontend.Variable
	TransmissionKeyY frontend.Variable
	ClueKey          frontend.Variable
	Position         frontend.Variable
	StatePath        [stateCommitmentDepth][3]frontend.Variable

	VBlinding           frontend.Variable
	SpendAuthRandomizer frontend.Variable
	AKX                 frontend.Variable
	AKY                 frontend.Variable
	NK                  frontend.Variable
	IVKReduced          frontend.Variable
	IVKQuotientA        frontend.Variable

	IndexedLeafValue          frontend.Variable
	IndexedLeafNextIndex      frontend.Variable
	IndexedLeafNextValue      frontend.Variable
	IndexedLeafDKPubX         frontend.Variable
	IndexedLeafDKPubY         frontend.Variable
	IndexedLeafThreshold      frontend.Variable
	IndexedLeafChannelsHash   frontend.Variable
	IndexedLeafRingPKX        frontend.Variable
	IndexedLeafRingPKY        frontend.Variable
	IndexedLeafRingIDHash     frontend.Variable
	IndexedLeafPolicyIDHash   frontend.Variable
	IndexedLeafPermissionHash frontend.Variable
	IndexedLeafResourceHash   frontend.Variable
	AssetPath                 [complianceQuadTreeDepth][3]frontend.Variable
	AssetPosition             frontend.Variable

	UserDivGenX         frontend.Variable
	UserDivGenY         frontend.Variable
	UserTransX          frontend.Variable
	UserTransY          frontend.Variable
	UserAssetID         frontend.Variable
	UserD               frontend.Variable
	CompliancePath      [complianceQuadTreeDepth][3]frontend.Variable
	CompliancePosition  frontend.Variable
	IsRegulated         frontend.Variable
	IsFlagged           frontend.Variable
	ComplianceEphemeral frontend.Variable
	Salt                frontend.Variable
	TxBlindingNonce     frontend.Variable
}

func (c *SpendCircuit) Define(api frontend.API) error {
	noteDivGen := gnarkte.Point{X: c.DiversifiedGenX, Y: c.DiversifiedGenY}
	noteTransmission := gnarkte.Point{X: c.TransmissionKeyX, Y: c.TransmissionKeyY}
	ak := gnarkte.Point{X: c.AKX, Y: c.AKY}
	rkClaimed := gnarkte.Point{X: c.RKX, Y: c.RKY}
	epk := gnarkte.Point{X: c.EpkX, Y: c.EpkY}

	indexedLeaf := indexedLeafInputs{
		Value:          c.IndexedLeafValue,
		NextIndex:      c.IndexedLeafNextIndex,
		NextValue:      c.IndexedLeafNextValue,
		DKPub:          gnarkte.Point{X: c.IndexedLeafDKPubX, Y: c.IndexedLeafDKPubY},
		Threshold:      c.IndexedLeafThreshold,
		ChannelsHash:   c.IndexedLeafChannelsHash,
		RingPK:         gnarkte.Point{X: c.IndexedLeafRingPKX, Y: c.IndexedLeafRingPKY},
		RingIDHash:     c.IndexedLeafRingIDHash,
		PolicyIDHash:   c.IndexedLeafPolicyIDHash,
		PermissionHash: c.IndexedLeafPermissionHash,
		ResourceHash:   c.IndexedLeafResourceHash,
	}
	userDivGen := gnarkte.Point{X: c.UserDivGenX, Y: c.UserDivGenY}
	userTrans := gnarkte.Point{X: c.UserTransX, Y: c.UserTransY}

	commitment, err := NoteCommitment(
		api,
		c.NoteBlinding,
		c.NoteAmount,
		c.NoteAssetID,
		noteDivGen,
		c.TransmissionKeyS,
		c.ClueKey,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(commitment, c.ExpectedStateCommitment)

	nullifier, err := Nullifier(api, c.NK, c.ExpectedStateCommitment, c.Position)
	if err != nil {
		return err
	}
	api.AssertIsEqual(nullifier, c.Nullifier)

	statePath := make([][3]frontend.Variable, len(c.StatePath))
	copy(statePath, c.StatePath[:])
	anchor, err := VerifyStateCommitmentPath(api, c.ExpectedStateCommitment, c.Position, statePath)
	if err != nil {
		return err
	}
	isDummy := api.IsZero(c.NoteAmount)
	isNotDummy := api.Sub(1, isDummy)
	assertEqualIf(api, anchor, c.Anchor, isNotDummy)

	computedRK, err := RandomizedVerificationKey(api, ak, c.SpendAuthRandomizer)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, computedRK, rkClaimed)

	computedTransmission, err := DiversifiedTransmissionKey(
		api,
		c.NK,
		ak,
		noteDivGen,
		c.IVKReduced,
		c.IVKQuotientA,
	)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, computedTransmission, noteTransmission)

	balanceCommitment, err := BalanceCommitment(api, c.NoteAmount, c.NoteAssetID, c.VBlinding)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(
		api,
		balanceCommitment,
		gnarkte.Point{X: c.BalanceCommitmentX, Y: c.BalanceCommitmentY},
	)

	assetLeafCommitment, err := IndexedLeafCommitment(api, indexedLeaf)
	if err != nil {
		return err
	}
	assetRoot, err := VerifyQuadPath(api, assetLeafCommitment, c.AssetPath, c.AssetPosition)
	if err != nil {
		return err
	}
	assertEqualIf(api, assetRoot, c.AssetAnchor, c.IsRegulated)

	userLeafCommitment, err := ComplianceLeafCommitment(api, userDivGen, userTrans, c.UserAssetID, c.UserD)
	if err != nil {
		return err
	}
	complianceRoot, err := VerifyQuadPath(api, userLeafCommitment, c.CompliancePath, c.CompliancePosition)
	if err != nil {
		return err
	}
	assertEqualIf(api, complianceRoot, c.ComplianceAnchor, c.IsRegulated)

	AssertDecafEquivalent(api, userDivGen, noteDivGen)
	AssertDecafEquivalent(api, userTrans, noteTransmission)
	api.AssertIsEqual(c.UserAssetID, c.NoteAssetID)

	VerifyThresholdFlagSimple(api, c.NoteAmount, c.IndexedLeafThreshold, c.IsFlagged)

	ack, err := DeriveACKFromLeafD(api, indexedLeaf.RingPK, c.UserD)
	if err != nil {
		return err
	}
	ssDetection, ssCore, err := DeriveSharedSecretsSpend(
		api,
		c.ComplianceEphemeral,
		ack,
		indexedLeaf.DKPub,
		c.IsFlagged,
		epk,
	)
	if err != nil {
		return err
	}
	if err := VerifyPoseidonEncryptionSpend(
		api,
		c.IsRegulated,
		c.IsFlagged,
		ssDetection,
		ssCore,
		c.C2Core,
		epk,
		c.Salt,
		c.NoteAmount,
		c.NoteAssetID,
		noteDivGen,
		noteTransmission,
		c.ComplianceCiphertext,
	); err != nil {
		return err
	}

	metadataHash, err := ComputeMetadataHash(
		api,
		c.IndexedLeafPolicyIDHash,
		c.IndexedLeafResourceHash,
		c.IndexedLeafPermissionHash,
		1,
		c.TargetTimestamp,
		c.Salt,
	)
	if err != nil {
		return err
	}
	if err := VerifyDLEQ(
		api,
		c.ComplianceEphemeral,
		ack,
		epk,
		metadataHash,
		c.DleqC,
		c.DleqS,
		c.IsRegulated,
	); err != nil {
		return err
	}

	blindedSender, err := BlindSenderLeaf(api, userLeafCommitment, c.TxBlindingNonce)
	if err != nil {
		return err
	}
	api.AssertIsEqual(blindedSender, c.SenderLeafHash)

	balanceCommitmentFq, err := Decaf377CompressToField(api, balanceCommitment)
	if err != nil {
		return err
	}
	rkFq, err := Decaf377CompressToField(api, rkClaimed)
	if err != nil {
		return err
	}
	epkFq, err := Decaf377CompressToField(api, epk)
	if err != nil {
		return err
	}

	fields := make([]frontend.Variable, 0, spendStatementFieldCount)
	fields = append(fields,
		c.Anchor,
		balanceCommitmentFq,
		c.Nullifier,
		rkFq,
		c.AssetAnchor,
		c.ComplianceAnchor,
		epkFq,
		c.C2Core,
	)
	for i := range c.ComplianceCiphertext {
		fields = append(fields, c.ComplianceCiphertext[i])
	}
	fields = append(fields,
		c.TargetTimestamp,
		c.DleqC,
		c.DleqS,
		c.SenderLeafHash,
	)
	computedStatementHash, err := SpendStatementHash(api, fields)
	if err != nil {
		return err
	}
	api.AssertIsEqual(computedStatementHash, c.ClaimedStatementHash)

	return nil
}

func NewSpendCircuitAssignmentFromFixture(fixture spendFixture) (*SpendCircuit, error) {
	transmissionKeyS, err := compressedLEHexToBigInt(fixture.Private.TransmissionKeyHex)
	if err != nil {
		return nil, fmt.Errorf("parse transmission key: %w", err)
	}
	ivkReduced, quotientA, err := IncomingViewingKeyReductionNative(fixture)
	if err != nil {
		return nil, fmt.Errorf("compute ivk reduction: %w", err)
	}
	indexedLeaf, err := indexedLeafInputsFromFixture(fixture)
	if err != nil {
		return nil, fmt.Errorf("decode indexed leaf: %w", err)
	}
	assetPath, err := quadPathFromFixture(fixture.Private.AssetPath)
	if err != nil {
		return nil, fmt.Errorf("decode asset path: %w", err)
	}
	compliancePath, err := quadPathFromFixture(fixture.Private.CompliancePath)
	if err != nil {
		return nil, fmt.Errorf("decode compliance path: %w", err)
	}

	var statePath [stateCommitmentDepth][3]frontend.Variable
	for i, siblings := range fixture.Private.StateCommitmentProof.AuthPath {
		for j, sibling := range siblings {
			statePath[i][j] = sibling
		}
	}

	var assetPathVars [complianceQuadTreeDepth][3]frontend.Variable
	var compliancePathVars [complianceQuadTreeDepth][3]frontend.Variable
	for i := 0; i < complianceQuadTreeDepth; i++ {
		for j := 0; j < 3; j++ {
			assetPathVars[i][j] = assetPath[i][j].String()
			compliancePathVars[i][j] = compliancePath[i][j].String()
		}
	}

	assignment := &SpendCircuit{
		ClaimedStatementHash: fixture.ClaimedStatementHash,

		Anchor:                  fixture.Public.Anchor,
		BalanceCommitmentX:      fixture.Public.BalanceCommitmentAffine.X,
		BalanceCommitmentY:      fixture.Public.BalanceCommitmentAffine.Y,
		Nullifier:               fixture.Public.Nullifier,
		RKX:                     fixture.Public.RKAffine.X,
		RKY:                     fixture.Public.RKAffine.Y,
		AssetAnchor:             fixture.Public.AssetAnchor,
		ComplianceAnchor:        fixture.Public.ComplianceAnchor,
		EpkX:                    fixture.Public.EpkAffine.X,
		EpkY:                    fixture.Public.EpkAffine.Y,
		C2Core:                  fixture.Public.C2Core,
		TargetTimestamp:         fixture.Public.TargetTimestamp,
		DleqC:                   fixture.Public.DleqC,
		DleqS:                   fixture.Public.DleqS,
		SenderLeafHash:          fixture.Public.SenderLeafHash,
		ExpectedStateCommitment: fixture.Private.StateCommitmentProof.Commitment,

		NoteBlinding:     fixture.Private.NoteBlinding,
		NoteAmount:       fixture.Private.NoteAmount,
		NoteAssetID:      fixture.Private.NoteAssetID,
		DiversifiedGenX:  fixture.Private.DiversifiedGeneratorAffine.X,
		DiversifiedGenY:  fixture.Private.DiversifiedGeneratorAffine.Y,
		TransmissionKeyS: transmissionKeyS.String(),
		TransmissionKeyX: fixture.Private.TransmissionKeyAffine.X,
		TransmissionKeyY: fixture.Private.TransmissionKeyAffine.Y,
		ClueKey:          fixture.Private.ClueKey,
		Position:         fixture.Private.StateCommitmentProof.Position,
		StatePath:        statePath,

		VBlinding:           fixture.Private.VBlinding,
		SpendAuthRandomizer: fixture.Private.SpendAuthRandomizer,
		AKX:                 fixture.Private.AKAffine.X,
		AKY:                 fixture.Private.AKAffine.Y,
		NK:                  fixture.Private.NK,
		IVKReduced:          ivkReduced.String(),
		IVKQuotientA:        quotientA,

		IndexedLeafValue:          indexedLeaf.Value,
		IndexedLeafNextIndex:      indexedLeaf.NextIndex,
		IndexedLeafNextValue:      indexedLeaf.NextValue,
		IndexedLeafDKPubX:         fixture.Private.AssetIndexedLeafDKPubAffine.X,
		IndexedLeafDKPubY:         fixture.Private.AssetIndexedLeafDKPubAffine.Y,
		IndexedLeafThreshold:      indexedLeaf.Threshold,
		IndexedLeafChannelsHash:   indexedLeaf.ChannelsHash,
		IndexedLeafRingPKX:        fixture.Private.AssetIndexedLeafRingPKAffine.X,
		IndexedLeafRingPKY:        fixture.Private.AssetIndexedLeafRingPKAffine.Y,
		IndexedLeafRingIDHash:     indexedLeaf.RingIDHash,
		IndexedLeafPolicyIDHash:   indexedLeaf.PolicyIDHash,
		IndexedLeafPermissionHash: indexedLeaf.PermissionHash,
		IndexedLeafResourceHash:   indexedLeaf.ResourceHash,
		AssetPath:                 assetPathVars,
		AssetPosition:             fixture.Private.AssetPosition,

		UserDivGenX:         fixture.Private.UserDiversifiedGeneratorAffine.X,
		UserDivGenY:         fixture.Private.UserDiversifiedGeneratorAffine.Y,
		UserTransX:          fixture.Private.UserTransmissionKeyAffine.X,
		UserTransY:          fixture.Private.UserTransmissionKeyAffine.Y,
		UserAssetID:         fixture.Private.NoteAssetID,
		UserD:               fixture.Private.UserDDecimal,
		CompliancePath:      compliancePathVars,
		CompliancePosition:  fixture.Private.CompliancePosition,
		IsRegulated:         boolToField(fixture.Private.IsRegulated),
		IsFlagged:           boolToField(fixture.Private.IsFlagged),
		ComplianceEphemeral: fixture.Private.ComplianceEphemeralSecret,
		Salt:                fixture.Private.Salt,
		TxBlindingNonce:     fixture.Private.TxBlindingNonce,
	}
	for i := range assignment.ComplianceCiphertext {
		assignment.ComplianceCiphertext[i] = fixture.Public.ComplianceCiphertext[i]
	}
	return assignment, nil
}

func boolToField(value bool) int {
	if value {
		return 1
	}
	return 0
}

func mutateFieldByOne(value frontend.Variable) string {
	var asBig *big.Int
	switch v := value.(type) {
	case string:
		asBig = mustBigInt(v)
	case *big.Int:
		asBig = new(big.Int).Set(v)
	default:
		panic(fmt.Sprintf("unsupported mutation type %T", value))
	}
	asBig.Add(asBig, big.NewInt(1))
	asBig.Mod(asBig, ScalarField())
	return asBig.String()
}

func pointAffineBinaryToStrings(point pointAffineBinary) pointAffineFixture {
	return pointAffineFixture{
		X: littleEndianBytesToBigInt(point.X[:]).String(),
		Y: littleEndianBytesToBigInt(point.Y[:]).String(),
	}
}

func incomingViewingKeyReductionFromBinary(nk [32]byte, akCompressed [32]byte) (*big.Int, uint64, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, 0, err
	}
	ivkModQ, err := Poseidon377Hash2Native(
		mustBigInt(vectors.Poseidon377.IVKDomain),
		[2]*big.Int{
			littleEndianBytesToBigInt(nk[:]),
			littleEndianBytesToBigInt(akCompressed[:]),
		},
	)
	if err != nil {
		return nil, 0, err
	}
	rModulus := mustBigInt(vectors.Decaf377CompanionCurve.Order)
	ivkModR := new(big.Int).Mod(new(big.Int).Set(ivkModQ), rModulus)
	quotient := new(big.Int).Sub(ivkModQ, ivkModR)
	quotient.Div(quotient, rModulus)
	return ivkModR, quotient.Uint64(), nil
}

func indexedLeafInputsFromBinary(witness *spendWitnessV1Binary) indexedLeafInputs {
	return indexedLeafInputs{
		Value:          littleEndianBytesToBigInt(witness.AssetIndexedLeaf.Value[:]).String(),
		NextIndex:      witness.AssetIndexedLeaf.NextIndex,
		NextValue:      littleEndianBytesToBigInt(witness.AssetIndexedLeaf.NextValue[:]).String(),
		DKPub:          pointAffineToNative(pointAffineBinaryToStrings(witness.AssetIndexedLeafDKPub)),
		Threshold:      littleEndianBytesToBigInt(witness.AssetIndexedLeaf.Threshold[:]).String(),
		ChannelsHash:   littleEndianBytesToBigInt(witness.AssetIndexedLeaf.ChannelsHash[:]).String(),
		RingPK:         pointAffineToNative(pointAffineBinaryToStrings(witness.AssetIndexedLeafRingPK)),
		RingIDHash:     littleEndianBytesToBigInt(witness.AssetIndexedLeaf.RingIDHash[:]).String(),
		PolicyIDHash:   littleEndianBytesToBigInt(witness.AssetIndexedLeaf.PolicyIDHash[:]).String(),
		PermissionHash: littleEndianBytesToBigInt(witness.AssetIndexedLeaf.PermissionHash[:]).String(),
		ResourceHash:   littleEndianBytesToBigInt(witness.AssetIndexedLeaf.ResourceHash[:]).String(),
	}
}

func quadPathVarsFromBinary(path merklePathBinary) [complianceQuadTreeDepth][3]frontend.Variable {
	var out [complianceQuadTreeDepth][3]frontend.Variable
	for i := 0; i < len(path.Layers) && i < complianceQuadTreeDepth; i++ {
		for j := 0; j < len(path.Layers[i]) && j < 3; j++ {
			out[i][j] = littleEndianBytesToBigInt(path.Layers[i][j][:]).String()
		}
	}
	return out
}

func statePathVarsFromBinary(path [][3][32]byte) [stateCommitmentDepth][3]frontend.Variable {
	var out [stateCommitmentDepth][3]frontend.Variable
	for i := 0; i < len(path) && i < stateCommitmentDepth; i++ {
		for j := 0; j < 3; j++ {
			out[i][j] = littleEndianBytesToBigInt(path[i][j][:]).String()
		}
	}
	return out
}

func NewSpendCircuitAssignmentFromWitnessV1(payload []byte) (*SpendCircuit, error) {
	witness, err := decodeSpendWitnessV1(payload)
	if err != nil {
		return nil, fmt.Errorf("decode SpendWitnessV1: %w", err)
	}
	indexedLeaf := indexedLeafInputsFromBinary(witness)
	ivkReduced, quotientA, err := incomingViewingKeyReductionFromBinary(witness.NK, witness.AK)
	if err != nil {
		return nil, fmt.Errorf("compute ivk reduction from binary witness: %w", err)
	}

	assignment := &SpendCircuit{
		ClaimedStatementHash: littleEndianBytesToBigInt(witness.ClaimedStatementHash[:]).String(),

		Anchor:                  littleEndianBytesToBigInt(witness.Anchor[:]).String(),
		BalanceCommitmentX:      littleEndianBytesToBigInt(witness.BalanceCommitmentAffine.X[:]).String(),
		BalanceCommitmentY:      littleEndianBytesToBigInt(witness.BalanceCommitmentAffine.Y[:]).String(),
		Nullifier:               littleEndianBytesToBigInt(witness.Nullifier[:]).String(),
		RKX:                     littleEndianBytesToBigInt(witness.RKAffine.X[:]).String(),
		RKY:                     littleEndianBytesToBigInt(witness.RKAffine.Y[:]).String(),
		AssetAnchor:             littleEndianBytesToBigInt(witness.AssetAnchor[:]).String(),
		ComplianceAnchor:        littleEndianBytesToBigInt(witness.ComplianceAnchor[:]).String(),
		EpkX:                    littleEndianBytesToBigInt(witness.EpkAffine.X[:]).String(),
		EpkY:                    littleEndianBytesToBigInt(witness.EpkAffine.Y[:]).String(),
		C2Core:                  littleEndianBytesToBigInt(witness.C2Core[:]).String(),
		TargetTimestamp:         littleEndianBytesToBigInt(witness.TargetTimestamp[:]).String(),
		DleqC:                   littleEndianBytesToBigInt(witness.DleqC[:]).String(),
		DleqS:                   littleEndianBytesToBigInt(witness.DleqS[:]).String(),
		SenderLeafHash:          littleEndianBytesToBigInt(witness.SenderLeafHash[:]).String(),
		ExpectedStateCommitment: littleEndianBytesToBigInt(witness.StateCommitmentCommitment[:]).String(),

		NoteBlinding:     littleEndianBytesToBigInt(witness.NoteBlinding[:]).String(),
		NoteAmount:       littleEndianBytesToBigInt(witness.NoteAmount[:]).String(),
		NoteAssetID:      littleEndianBytesToBigInt(witness.NoteAssetID[:]).String(),
		DiversifiedGenX:  littleEndianBytesToBigInt(witness.DiversifiedGeneratorAffine.X[:]).String(),
		DiversifiedGenY:  littleEndianBytesToBigInt(witness.DiversifiedGeneratorAffine.Y[:]).String(),
		TransmissionKeyS: littleEndianBytesToBigInt(witness.TransmissionKey[:]).String(),
		TransmissionKeyX: littleEndianBytesToBigInt(witness.TransmissionKeyAffine.X[:]).String(),
		TransmissionKeyY: littleEndianBytesToBigInt(witness.TransmissionKeyAffine.Y[:]).String(),
		ClueKey:          littleEndianBytesToBigInt(witness.ClueKey[:]).String(),
		Position:         witness.StateCommitmentPosition,
		StatePath:        statePathVarsFromBinary(witness.StateCommitmentAuthPath),

		VBlinding:           littleEndianBytesToBigInt(witness.VBlinding[:]).String(),
		SpendAuthRandomizer: littleEndianBytesToBigInt(witness.SpendAuthRandomizer[:]).String(),
		AKX:                 littleEndianBytesToBigInt(witness.AKAffine.X[:]).String(),
		AKY:                 littleEndianBytesToBigInt(witness.AKAffine.Y[:]).String(),
		NK:                  littleEndianBytesToBigInt(witness.NK[:]).String(),
		IVKReduced:          ivkReduced.String(),
		IVKQuotientA:        quotientA,

		IndexedLeafValue:          indexedLeaf.Value,
		IndexedLeafNextIndex:      indexedLeaf.NextIndex,
		IndexedLeafNextValue:      indexedLeaf.NextValue,
		IndexedLeafDKPubX:         littleEndianBytesToBigInt(witness.AssetIndexedLeafDKPub.X[:]).String(),
		IndexedLeafDKPubY:         littleEndianBytesToBigInt(witness.AssetIndexedLeafDKPub.Y[:]).String(),
		IndexedLeafThreshold:      indexedLeaf.Threshold,
		IndexedLeafChannelsHash:   indexedLeaf.ChannelsHash,
		IndexedLeafRingPKX:        littleEndianBytesToBigInt(witness.AssetIndexedLeafRingPK.X[:]).String(),
		IndexedLeafRingPKY:        littleEndianBytesToBigInt(witness.AssetIndexedLeafRingPK.Y[:]).String(),
		IndexedLeafRingIDHash:     indexedLeaf.RingIDHash,
		IndexedLeafPolicyIDHash:   indexedLeaf.PolicyIDHash,
		IndexedLeafPermissionHash: indexedLeaf.PermissionHash,
		IndexedLeafResourceHash:   indexedLeaf.ResourceHash,
		AssetPath:                 quadPathVarsFromBinary(witness.AssetPath),
		AssetPosition:             witness.AssetPosition,

		UserDivGenX:         littleEndianBytesToBigInt(witness.UserDiversifiedGenerator.X[:]).String(),
		UserDivGenY:         littleEndianBytesToBigInt(witness.UserDiversifiedGenerator.Y[:]).String(),
		UserTransX:          littleEndianBytesToBigInt(witness.UserTransmissionKey.X[:]).String(),
		UserTransY:          littleEndianBytesToBigInt(witness.UserTransmissionKey.Y[:]).String(),
		UserAssetID:         littleEndianBytesToBigInt(witness.UserAssetID[:]).String(),
		UserD:               littleEndianBytesToBigInt(witness.UserD[:]).String(),
		CompliancePath:      quadPathVarsFromBinary(witness.CompliancePath),
		CompliancePosition:  witness.CompliancePosition,
		IsRegulated:         boolToField(witness.IsRegulated),
		IsFlagged:           boolToField(witness.IsFlagged),
		ComplianceEphemeral: littleEndianBytesToBigInt(witness.ComplianceEphemeralSecret[:]).String(),
		Salt:                littleEndianBytesToBigInt(witness.Salt[:]).String(),
		TxBlindingNonce:     littleEndianBytesToBigInt(witness.TxBlindingNonce[:]).String(),
	}

	for i := range assignment.ComplianceCiphertext {
		assignment.ComplianceCiphertext[i] = littleEndianBytesToBigInt(witness.ComplianceCiphertext[i][:]).String()
	}
	return assignment, nil
}
