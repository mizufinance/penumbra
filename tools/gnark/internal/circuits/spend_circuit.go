package circuits

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type SpendCircuit struct {
	ClaimedStatementHash frontend.Variable `gnark:",public"`

	Anchor            frontend.Variable
	BalanceCommitment Point2D
	Nullifier         frontend.Variable
	RK                Point2D
	AssetAnchor       frontend.Variable
	ComplianceAnchor  frontend.Variable
	TargetTimestamp   frontend.Variable
	SenderLeafHash    frontend.Variable

	StateProof StateCommitmentFields
	Note       NoteFields
	Auth       SpendAuthFields
	Asset      AssetTreeFields
	User       UserComplianceFields
	Enc        SpendEncryptionFields
	Dleq       DLEQFields
}

func (c *SpendCircuit) Define(api frontend.API) error {
	noteDivGen := gnarkte.Point{X: c.Note.DivGen.X, Y: c.Note.DivGen.Y}
	noteTransmission := gnarkte.Point{X: c.Note.Transmission.X, Y: c.Note.Transmission.Y}
	ak := gnarkte.Point{X: c.Auth.AK.X, Y: c.Auth.AK.Y}
	rkClaimed := gnarkte.Point{X: c.RK.X, Y: c.RK.Y}
	epk := gnarkte.Point{X: c.Enc.Epk.X, Y: c.Enc.Epk.Y}

	indexedLeaf := IndexedLeafInputs{
		Value:          c.Asset.Leaf.Value,
		NextIndex:      c.Asset.Leaf.NextIndex,
		NextValue:      c.Asset.Leaf.NextValue,
		DKPub:          gnarkte.Point{X: c.Asset.Leaf.DKPub.X, Y: c.Asset.Leaf.DKPub.Y},
		Threshold:      c.Asset.Leaf.Threshold,
		ChannelsHash:   c.Asset.Leaf.ChannelsHash,
		RingPK:         gnarkte.Point{X: c.Asset.Leaf.RingPK.X, Y: c.Asset.Leaf.RingPK.Y},
		RingIDHash:     c.Asset.Leaf.RingIDHash,
		PolicyIDHash:   c.Asset.Leaf.PolicyIDHash,
		PermissionHash: c.Asset.Leaf.PermissionHash,
		ResourceHash:   c.Asset.Leaf.ResourceHash,
	}
	userDivGen := gnarkte.Point{X: c.User.DivGen.X, Y: c.User.DivGen.Y}
	userTrans := gnarkte.Point{X: c.User.Transmission.X, Y: c.User.Transmission.Y}

	noteDivGenFq, err := Decaf377CompressToField(api, noteDivGen)
	if err != nil {
		return err
	}
	noteTransmissionFq, err := Decaf377CompressToField(api, noteTransmission)
	if err != nil {
		return err
	}
	epkFq, err := Decaf377CompressToField(api, epk)
	if err != nil {
		return err
	}

	commitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		c.Note.Blinding,
		c.Note.Amount,
		c.Note.AssetID,
		noteDivGenFq,
		c.Note.TransmissionKeyS,
		c.Note.ClueKey,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(commitment, c.StateProof.Commitment)

	nullifier, err := Nullifier(api, c.Auth.NK, c.StateProof.Commitment, c.StateProof.Position)
	if err != nil {
		return err
	}
	api.AssertIsEqual(nullifier, c.Nullifier)

	statePath := make([][3]frontend.Variable, len(c.StateProof.Path))
	copy(statePath, c.StateProof.Path[:])
	anchor, err := VerifyStateCommitmentPath(api, c.StateProof.Commitment, c.StateProof.Position, statePath)
	if err != nil {
		return err
	}
	isDummy := api.IsZero(c.Note.Amount)
	isNotDummy := api.Sub(1, isDummy)
	AssertEqualIf(api, anchor, c.Anchor, isNotDummy)

	computedRK, err := RandomizedVerificationKey(api, ak, c.Auth.Randomizer)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, computedRK, rkClaimed)

	computedTransmission, err := DiversifiedTransmissionKey(
		api,
		c.Auth.NK,
		ak,
		noteDivGen,
		c.Auth.IVKReduced,
		c.Auth.IVKQuotientA,
	)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, computedTransmission, noteTransmission)

	balanceCommitment, err := BalanceCommitment(api, c.Note.Amount, c.Note.AssetID, c.Auth.VBlinding)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(
		api,
		balanceCommitment,
		gnarkte.Point{X: c.BalanceCommitment.X, Y: c.BalanceCommitment.Y},
	)

	assetLeafCommitment, err := IndexedLeafCommitment(api, indexedLeaf)
	if err != nil {
		return err
	}
	assetRoot, err := VerifyQuadPath(api, assetLeafCommitment, c.Asset.Path, c.Asset.Position)
	if err != nil {
		return err
	}
	AssertEqualIf(api, assetRoot, c.AssetAnchor, c.Enc.IsRegulated)

	AssertDecafEquivalent(api, userDivGen, noteDivGen)
	AssertDecafEquivalent(api, userTrans, noteTransmission)
	api.AssertIsEqual(c.User.AssetID, c.Note.AssetID)

	userLeafCommitment, err := ComplianceLeafCommitmentFromCompressed(
		api,
		noteDivGenFq,
		noteTransmissionFq,
		c.User.AssetID,
		c.User.D,
	)
	if err != nil {
		return err
	}
	complianceRoot, err := VerifyQuadPath(api, userLeafCommitment, c.User.Path, c.User.Position)
	if err != nil {
		return err
	}
	AssertEqualIf(api, complianceRoot, c.ComplianceAnchor, c.Enc.IsRegulated)

	VerifyThresholdFlagSimple(api, c.Note.Amount, c.Asset.Leaf.Threshold, c.Enc.IsFlagged)

	ack, err := DeriveACKFromLeafD(api, indexedLeaf.RingPK, c.User.D)
	if err != nil {
		return err
	}
	ssDetection, ssCoreUser, ssCore, err := DeriveSharedSecretsSpend(
		api,
		c.Enc.ComplianceEphemeral,
		ack,
		indexedLeaf.DKPub,
		c.Enc.IsFlagged,
		epk,
	)
	if err != nil {
		return err
	}
	if err := VerifyPoseidonEncryptionSpend(
		api,
		c.Enc.IsRegulated,
		c.Enc.IsFlagged,
		ssDetection,
		ssCore,
		c.Enc.C2Core,
		epkFq,
		c.Enc.Salt,
		c.Note.Amount,
		c.Note.AssetID,
		noteDivGenFq,
		noteTransmissionFq,
		c.Enc.ComplianceCiphertext,
	); err != nil {
		return err
	}

	metadataHash, err := ComputeMetadataHash(
		api,
		c.Asset.Leaf.PolicyIDHash,
		c.Asset.Leaf.ResourceHash,
		c.Asset.Leaf.PermissionHash,
		1,
		c.TargetTimestamp,
		c.Enc.Salt,
	)
	if err != nil {
		return err
	}
	if err := VerifyDLEQ(
		api,
		c.Enc.ComplianceEphemeral,
		ack,
		ssCoreUser,
		epk,
		metadataHash,
		c.Dleq.C,
		c.Dleq.S,
		c.Enc.IsRegulated,
	); err != nil {
		return err
	}

	blindedSender, err := BlindSenderLeaf(api, userLeafCommitment, c.Enc.TxBlindingNonce)
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

	fields := make([]frontend.Variable, 0, SpendStatementFieldCount)
	fields = append(fields,
		c.Anchor,
		balanceCommitmentFq,
		c.Nullifier,
		rkFq,
		c.AssetAnchor,
		c.ComplianceAnchor,
		epkFq,
		c.Enc.C2Core,
	)
	for i := range c.Enc.ComplianceCiphertext {
		fields = append(fields, c.Enc.ComplianceCiphertext[i])
	}
	fields = append(fields,
		c.TargetTimestamp,
		c.Dleq.C,
		c.Dleq.S,
		c.SenderLeafHash,
	)
	computedStatementHash, err := SpendStatementHash(api, fields)
	if err != nil {
		return err
	}
	api.AssertIsEqual(computedStatementHash, c.ClaimedStatementHash)

	return nil
}

func NewSpendCircuitAssignmentFromFixture(fixture SpendFixture) (*SpendCircuit, error) {
	transmissionKeyS, err := CompressedLEHexToBigInt(fixture.Private.TransmissionKeyHex)
	if err != nil {
		return nil, fmt.Errorf("parse transmission key: %w", err)
	}
	ivkReduced, quotientA, err := IncomingViewingKeyReductionNative(fixture)
	if err != nil {
		return nil, fmt.Errorf("compute ivk reduction: %w", err)
	}
	indexedLeaf, err := IndexedLeafInputsFromFixture(fixture)
	if err != nil {
		return nil, fmt.Errorf("decode indexed leaf: %w", err)
	}
	assetPath, err := QuadPathFromFixture(fixture.Private.AssetPath)
	if err != nil {
		return nil, fmt.Errorf("decode asset path: %w", err)
	}
	compliancePath, err := QuadPathFromFixture(fixture.Private.CompliancePath)
	if err != nil {
		return nil, fmt.Errorf("decode compliance path: %w", err)
	}

	if len(fixture.Private.StateCommitmentProof.AuthPath) != StateCommitmentDepth {
		return nil, fmt.Errorf(
			"decode state commitment auth path: got %d layers, expected %d",
			len(fixture.Private.StateCommitmentProof.AuthPath),
			StateCommitmentDepth,
		)
	}
	var statePath [StateCommitmentDepth][3]frontend.Variable
	for i, siblings := range fixture.Private.StateCommitmentProof.AuthPath {
		for j, sibling := range siblings {
			statePath[i][j] = sibling
		}
	}

	var assetPathVars [ComplianceQuadTreeDepth][3]frontend.Variable
	var compliancePathVars [ComplianceQuadTreeDepth][3]frontend.Variable
	for i := 0; i < ComplianceQuadTreeDepth; i++ {
		for j := 0; j < 3; j++ {
			assetPathVars[i][j] = assetPath[i][j].String()
			compliancePathVars[i][j] = compliancePath[i][j].String()
		}
	}

	assignment := &SpendCircuit{
		ClaimedStatementHash: fixture.ClaimedStatementHash,

		Anchor:            fixture.Public.Anchor,
		BalanceCommitment: Point2D{X: fixture.Public.BalanceCommitmentAffine.X, Y: fixture.Public.BalanceCommitmentAffine.Y},
		Nullifier:         fixture.Public.Nullifier,
		RK:                Point2D{X: fixture.Public.RKAffine.X, Y: fixture.Public.RKAffine.Y},
		AssetAnchor:       fixture.Public.AssetAnchor,
		ComplianceAnchor:  fixture.Public.ComplianceAnchor,
		TargetTimestamp:   fixture.Public.TargetTimestamp,
		SenderLeafHash:    fixture.Public.SenderLeafHash,

		StateProof: StateCommitmentFields{
			Commitment: fixture.Private.StateCommitmentProof.Commitment,
			Position:   fixture.Private.StateCommitmentProof.Position,
			Path:       statePath,
		},
		Note: NoteFields{
			Blinding:         fixture.Private.NoteBlinding,
			Amount:           fixture.Private.NoteAmount,
			AssetID:          fixture.Private.NoteAssetID,
			DivGen:           Point2D{X: fixture.Private.DiversifiedGeneratorAffine.X, Y: fixture.Private.DiversifiedGeneratorAffine.Y},
			TransmissionKeyS: transmissionKeyS.String(),
			Transmission:     Point2D{X: fixture.Private.TransmissionKeyAffine.X, Y: fixture.Private.TransmissionKeyAffine.Y},
			ClueKey:          fixture.Private.ClueKey,
		},
		Auth: SpendAuthFields{
			VBlinding:    fixture.Private.VBlinding,
			Randomizer:   fixture.Private.SpendAuthRandomizer,
			AK:           Point2D{X: fixture.Private.AKAffine.X, Y: fixture.Private.AKAffine.Y},
			NK:           fixture.Private.NK,
			IVKReduced:   ivkReduced.String(),
			IVKQuotientA: quotientA,
		},
		Asset: AssetTreeFields{
			Leaf: IndexedLeafFields{
				Value:          indexedLeaf.Value,
				NextIndex:      indexedLeaf.NextIndex,
				NextValue:      indexedLeaf.NextValue,
				DKPub:          Point2D{X: fixture.Private.AssetIndexedLeafDKPubAffine.X, Y: fixture.Private.AssetIndexedLeafDKPubAffine.Y},
				Threshold:      indexedLeaf.Threshold,
				ChannelsHash:   indexedLeaf.ChannelsHash,
				RingPK:         Point2D{X: fixture.Private.AssetIndexedLeafRingPKAffine.X, Y: fixture.Private.AssetIndexedLeafRingPKAffine.Y},
				RingIDHash:     indexedLeaf.RingIDHash,
				PolicyIDHash:   indexedLeaf.PolicyIDHash,
				PermissionHash: indexedLeaf.PermissionHash,
				ResourceHash:   indexedLeaf.ResourceHash,
			},
			Path:     assetPathVars,
			Position: fixture.Private.AssetPosition,
		},
		User: UserComplianceFields{
			DivGen:       Point2D{X: fixture.Private.UserDiversifiedGeneratorAffine.X, Y: fixture.Private.UserDiversifiedGeneratorAffine.Y},
			Transmission: Point2D{X: fixture.Private.UserTransmissionKeyAffine.X, Y: fixture.Private.UserTransmissionKeyAffine.Y},
			AssetID:      fixture.Private.NoteAssetID,
			D:            fixture.Private.UserDDecimal,
			Path:         compliancePathVars,
			Position:     fixture.Private.CompliancePosition,
		},
		Enc: SpendEncryptionFields{
			Epk:                 Point2D{X: fixture.Public.EpkAffine.X, Y: fixture.Public.EpkAffine.Y},
			C2Core:              fixture.Public.C2Core,
			IsRegulated:         BoolToField(fixture.Private.IsRegulated),
			IsFlagged:           BoolToField(fixture.Private.IsFlagged),
			ComplianceEphemeral: fixture.Private.ComplianceEphemeralSecret,
			Salt:                fixture.Private.Salt,
			TxBlindingNonce:     fixture.Private.TxBlindingNonce,
		},
		Dleq: DLEQFields{
			C: fixture.Public.DleqC,
			S: fixture.Public.DleqS,
		},
	}
	if len(fixture.Public.ComplianceCiphertext) != len(assignment.Enc.ComplianceCiphertext) {
		return nil, fmt.Errorf(
			"unexpected spend compliance ciphertext length: got %d, want %d",
			len(fixture.Public.ComplianceCiphertext),
			len(assignment.Enc.ComplianceCiphertext),
		)
	}
	for i := range assignment.Enc.ComplianceCiphertext {
		assignment.Enc.ComplianceCiphertext[i] = fixture.Public.ComplianceCiphertext[i]
	}
	return assignment, nil
}

func MutateFieldByOne(value frontend.Variable) string {
	var asBig *big.Int
	switch v := value.(type) {
	case string:
		asBig = MustBigInt(v)
	case *big.Int:
		asBig = new(big.Int).Set(v)
	default:
		panic(fmt.Sprintf("unsupported mutation type %T", value))
	}
	asBig.Add(asBig, big.NewInt(1))
	asBig.Mod(asBig, ScalarField())
	return asBig.String()
}
