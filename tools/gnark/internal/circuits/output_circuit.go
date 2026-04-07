package circuits

import (
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type OutputCircuit struct {
	ClaimedStatementHash frontend.Variable `gnark:",public"`

	ClaimedNoteCommitment frontend.Variable
	BalanceCommitment     Point2D
	AssetAnchor           frontend.Variable
	ComplianceAnchor      frontend.Variable
	TargetTimestamp       frontend.Variable
	CounterpartyLeafHash  frontend.Variable

	Note            NoteFields
	BalanceBlinding frontend.Variable
	Asset           AssetTreeFields
	User            UserComplianceFields
	Counterparty    UserComplianceFields
	Enc             OutputEncryptionFields
	Dleq1           DLEQFields
	Dleq2           DLEQFields
	Dleq3           DLEQFields
}

func (c *OutputCircuit) Define(api frontend.API) error {
	noteDivGen := gnarkte.Point{X: c.Note.DivGen.X, Y: c.Note.DivGen.Y}
	noteTransmission := gnarkte.Point{X: c.Note.Transmission.X, Y: c.Note.Transmission.Y}
	claimedBalanceCommitment := gnarkte.Point{X: c.BalanceCommitment.X, Y: c.BalanceCommitment.Y}
	epk1 := gnarkte.Point{X: c.Enc.Epk1.X, Y: c.Enc.Epk1.Y}
	epk2 := gnarkte.Point{X: c.Enc.Epk2.X, Y: c.Enc.Epk2.Y}
	epk3 := gnarkte.Point{X: c.Enc.Epk3.X, Y: c.Enc.Epk3.Y}
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
	userTransmission := gnarkte.Point{X: c.User.Transmission.X, Y: c.User.Transmission.Y}
	counterpartyDivGen := gnarkte.Point{X: c.Counterparty.DivGen.X, Y: c.Counterparty.DivGen.Y}
	counterpartyTransmission := gnarkte.Point{X: c.Counterparty.Transmission.X, Y: c.Counterparty.Transmission.Y}

	noteDivGenFq, err := Decaf377CompressToField(api, noteDivGen)
	if err != nil {
		return err
	}
	noteTransmissionFq, err := Decaf377CompressToField(api, noteTransmission)
	if err != nil {
		return err
	}
	counterpartyDivGenFq, err := Decaf377CompressToField(api, counterpartyDivGen)
	if err != nil {
		return err
	}
	counterpartyTransmissionFq, err := Decaf377CompressToField(api, counterpartyTransmission)
	if err != nil {
		return err
	}
	balanceCommitmentFq, err := Decaf377CompressToField(api, claimedBalanceCommitment)
	if err != nil {
		return err
	}
	epk1Fq, err := Decaf377CompressToField(api, epk1)
	if err != nil {
		return err
	}
	epk2Fq, err := Decaf377CompressToField(api, epk2)
	if err != nil {
		return err
	}
	epk3Fq, err := Decaf377CompressToField(api, epk3)
	if err != nil {
		return err
	}

	noteCommitment, err := NoteCommitmentWithCompressedDivGen(
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
	api.AssertIsEqual(noteCommitment, c.ClaimedNoteCommitment)

	if err := AssertNegativeBalanceCommitment(
		api,
		claimedBalanceCommitment,
		c.Note.Amount,
		c.Note.AssetID,
		c.BalanceBlinding,
	); err != nil {
		return err
	}

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
	AssertDecafEquivalent(api, userTransmission, noteTransmission)
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

	ackReceiver, err := DeriveACKFromLeafD(api, indexedLeaf.RingPK, c.User.D)
	if err != nil {
		return err
	}
	ackSender, err := DeriveACKFromLeafD(api, indexedLeaf.RingPK, c.Counterparty.D)
	if err != nil {
		return err
	}

	ssDetection, ssCoreUser, ssExtUser, ssSextUser, ssCore, ssExt, ssSext, err := DeriveSharedSecretsOutput(
		api,
		c.Enc.ComplianceEphemeral,
		c.Enc.R2,
		c.Enc.R3,
		ackReceiver,
		ackSender,
		indexedLeaf.DKPub,
		c.Enc.IsFlagged,
		epk1,
		epk2,
		epk3,
	)
	if err != nil {
		return err
	}
	if err := VerifyPoseidonEncryptionOutput(
		api,
		c.Enc.IsRegulated,
		c.Enc.IsFlagged,
		ssDetection,
		ssCore,
		ssExt,
		ssSext,
		c.Enc.C2Core,
		c.Enc.C2Ext,
		c.Enc.C2Sext,
		epk1Fq,
		c.Enc.Salt,
		c.Note.Amount,
		c.Note.AssetID,
		noteDivGenFq,
		noteTransmissionFq,
		counterpartyDivGenFq,
		counterpartyTransmissionFq,
		c.Enc.ComplianceCiphertext,
	); err != nil {
		return err
	}

	metadataHashCore, err := ComputeMetadataHash(
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
	metadataHashExt, err := ComputeMetadataHash(
		api,
		c.Asset.Leaf.PolicyIDHash,
		c.Asset.Leaf.ResourceHash,
		c.Asset.Leaf.PermissionHash,
		2,
		c.TargetTimestamp,
		c.Enc.Salt,
	)
	if err != nil {
		return err
	}
	metadataHashSext, err := ComputeMetadataHash(
		api,
		c.Asset.Leaf.PolicyIDHash,
		c.Asset.Leaf.ResourceHash,
		c.Asset.Leaf.PermissionHash,
		3,
		c.TargetTimestamp,
		c.Enc.Salt,
	)
	if err != nil {
		return err
	}
	if err := VerifyDLEQ(api, c.Enc.ComplianceEphemeral, ackReceiver, ssCoreUser, epk1, metadataHashCore, c.Dleq1.C, c.Dleq1.S, c.Enc.IsRegulated); err != nil {
		return err
	}
	if err := VerifyDLEQ(api, c.Enc.R2, ackReceiver, ssExtUser, epk2, metadataHashExt, c.Dleq2.C, c.Dleq2.S, c.Enc.IsRegulated); err != nil {
		return err
	}
	if err := VerifyDLEQ(api, c.Enc.R3, ackSender, ssSextUser, epk3, metadataHashSext, c.Dleq3.C, c.Dleq3.S, c.Enc.IsRegulated); err != nil {
		return err
	}

	counterpartyLeafCommitment, err := ComplianceLeafCommitmentFromCompressed(
		api,
		counterpartyDivGenFq,
		counterpartyTransmissionFq,
		c.Counterparty.AssetID,
		c.Counterparty.D,
	)
	if err != nil {
		return err
	}
	blindedCounterparty, err := BlindSenderLeaf(api, counterpartyLeafCommitment, c.Enc.TxBlindingNonce)
	if err != nil {
		return err
	}
	api.AssertIsEqual(blindedCounterparty, c.CounterpartyLeafHash)

	statementHash, err := OutputStatementHashFromParts(
		api,
		c.ClaimedNoteCommitment,
		balanceCommitmentFq,
		epk1Fq,
		epk2Fq,
		epk3Fq,
		c.AssetAnchor,
		c.ComplianceAnchor,
		c.Enc.C2Core,
		c.Enc.C2Ext,
		c.Enc.C2Sext,
		c.Enc.ComplianceCiphertext,
		c.TargetTimestamp,
		c.Dleq1.C,
		c.Dleq1.S,
		c.Dleq2.C,
		c.Dleq2.S,
		c.Dleq3.C,
		c.Dleq3.S,
		c.CounterpartyLeafHash,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(statementHash, c.ClaimedStatementHash)
	return nil
}
