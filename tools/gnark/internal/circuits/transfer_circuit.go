package circuits

import (
	"fmt"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
	. "github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type TransferAuthSharedFields struct {
	AK           Point2D
	NK           frontend.Variable
	IVKReduced   frontend.Variable
	IVKQuotientA frontend.Variable
}

type TransferSpendCircuitFields struct {
	Nullifier      frontend.Variable
	RK             Point2D
	Note           NoteFields
	StateProof     StateCommitmentFields
	AuthRandomizer frontend.Variable
	Enc            SpendEncryptionFields
	Dleq           DLEQFields
}

type TransferOutputDLEQFields struct {
	Core DLEQFields
	Ext  DLEQFields
	Sext DLEQFields
}

type TransferOutputCircuitFields struct {
	NoteCommitment frontend.Variable
	Note           NoteFields
	Recipient      UserComplianceFields
	Enc            OutputEncryptionFields
	Dleq           TransferOutputDLEQFields
}

type TransferCircuit struct {
	nIn  int
	nOut int

	ClaimedStatementHash frontend.Variable `gnark:",public"`

	Anchor                frontend.Variable
	BalanceCommitment     Point2D
	AssetAnchor           frontend.Variable
	ComplianceAnchor      frontend.Variable
	TargetTimestamp       frontend.Variable
	ActionBalanceBlinding frontend.Variable
	IsRegulated           frontend.Variable
	TxBlindingNonce       frontend.Variable

	Auth   TransferAuthSharedFields
	Asset  AssetTreeFields
	Sender UserComplianceFields

	Spends  []TransferSpendCircuitFields
	Outputs []TransferOutputCircuitFields
}

func transferStatementFieldCountForShape(nIn, nOut int) int {
	return 5 + 11*nIn + 24*nOut
}

func NewTransferCircuit(nIn, nOut int) *TransferCircuit {
	return &TransferCircuit{
		nIn:     nIn,
		nOut:    nOut,
		Spends:  make([]TransferSpendCircuitFields, nIn),
		Outputs: make([]TransferOutputCircuitFields, nOut),
	}
}

func (c *TransferCircuit) Define(api frontend.API) error {
	if c.nIn <= 0 || c.nOut <= 0 {
		return fmt.Errorf("transfer circuit shape must be positive, got %dx%d", c.nIn, c.nOut)
	}
	if len(c.Spends) != c.nIn || len(c.Outputs) != c.nOut {
		return fmt.Errorf("transfer circuit shape mismatch: expected %dx%d, got %dx%d", c.nIn, c.nOut, len(c.Spends), len(c.Outputs))
	}

	shared, err := c.verifySharedTransferContext(api)
	if err != nil {
		return err
	}
	statementData := c.newTransferStatementData()

	for i := range c.Spends {
		if err := c.verifyTransferSpend(api, &shared, &statementData, &c.Spends[i]); err != nil {
			return err
		}
	}
	for i := range c.Outputs {
		if err := c.verifyTransferOutput(api, &shared, &statementData, &c.Outputs[i]); err != nil {
			return err
		}
	}

	balanceCommitmentFq, err := c.assertTransferNetBalanceCommitment(api, &shared, &statementData)
	if err != nil {
		return err
	}

	fields := c.buildTransferStatementFields(balanceCommitmentFq, &statementData)
	statementHash, err := TransferStatementHashForShape(api, c.nIn, c.nOut, fields)
	if err != nil {
		return err
	}
	api.AssertIsEqual(statementHash, c.ClaimedStatementHash)
	return nil
}

type transferSharedContext struct {
	claimedBalanceCommitment gnarkte.Point
	ak                       gnarkte.Point
	indexedLeaf              IndexedLeafInputs
	senderDivGen             gnarkte.Point
	senderTransmission       gnarkte.Point
	senderDivGenFq           frontend.Variable
	senderTransmissionFq     frontend.Variable
	senderAck                gnarkte.Point
	sharedAssetID            frontend.Variable
}

type transferStatementData struct {
	inputAmounts          []frontend.Variable
	outputAmounts         []frontend.Variable
	outputCommitments     []frontend.Variable
	spendStatementBlocks  [][]frontend.Variable
	outputStatementBlocks [][]frontend.Variable
	spendDLEQs            []frontend.Variable
	outputDLEQs           []frontend.Variable
	nullifiersAndRKs      []frontend.Variable
}

func computeTransferNetBalanceCommitment(
	api frontend.API,
	inputAmounts []frontend.Variable,
	outputAmounts []frontend.Variable,
	assetID frontend.Variable,
	blinding frontend.Variable,
) (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	hashedAssetID, err := Poseidon377Hash1(api, MustBigInt(vectors.Poseidon377.ValueGeneratorDomain), assetID)
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	valueGenerator, err := Decaf377EncodeToCurve(api, hashedAssetID)
	if err != nil {
		return gnarkte.Point{}, err
	}
	valueBlindingGenerator := gnarkte.Point{
		X: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorX),
		Y: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorY),
	}

	sum := ScalarMulLE(api, curve, valueGenerator, 0, 128)
	for _, amount := range inputAmounts {
		sum = curve.Add(sum, ScalarMulLE(api, curve, valueGenerator, amount, 128))
	}
	for _, amount := range outputAmounts {
		sum = curve.Add(sum, curve.Neg(ScalarMulLE(api, curve, valueGenerator, amount, 128)))
	}
	blindingPoint := ScalarMulLE(
		api,
		curve,
		valueBlindingGenerator,
		blinding,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	return curve.Add(sum, blindingPoint), nil
}

func (c *TransferCircuit) verifySharedTransferContext(api frontend.API) (transferSharedContext, error) {
	shared := transferSharedContext{
		claimedBalanceCommitment: gnarkte.Point{X: c.BalanceCommitment.X, Y: c.BalanceCommitment.Y},
		ak:                       gnarkte.Point{X: c.Auth.AK.X, Y: c.Auth.AK.Y},
		indexedLeaf: IndexedLeafInputs{
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
		},
		senderDivGen:       gnarkte.Point{X: c.Sender.DivGen.X, Y: c.Sender.DivGen.Y},
		senderTransmission: gnarkte.Point{X: c.Sender.Transmission.X, Y: c.Sender.Transmission.Y},
		sharedAssetID:      c.Spends[0].Note.AssetID,
	}

	var err error
	shared.senderDivGenFq, err = Decaf377CompressToField(api, shared.senderDivGen)
	if err != nil {
		return transferSharedContext{}, err
	}
	shared.senderTransmissionFq, err = Decaf377CompressToField(api, shared.senderTransmission)
	if err != nil {
		return transferSharedContext{}, err
	}

	assetLeafCommitment, err := IndexedLeafCommitment(api, shared.indexedLeaf)
	if err != nil {
		return transferSharedContext{}, err
	}
	assetRoot, err := VerifyQuadPath(api, assetLeafCommitment, c.Asset.Path, c.Asset.Position)
	if err != nil {
		return transferSharedContext{}, err
	}
	AssertEqualIf(api, assetRoot, c.AssetAnchor, c.IsRegulated)

	senderLeafCommitment, err := ComplianceLeafCommitmentFromCompressed(
		api,
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
		c.Sender.AssetID,
		c.Sender.D,
	)
	if err != nil {
		return transferSharedContext{}, err
	}
	senderComplianceRoot, err := VerifyQuadPath(api, senderLeafCommitment, c.Sender.Path, c.Sender.Position)
	if err != nil {
		return transferSharedContext{}, err
	}
	AssertEqualIf(api, senderComplianceRoot, c.ComplianceAnchor, c.IsRegulated)

	shared.senderAck, err = DeriveACKFromLeafD(api, shared.indexedLeaf.RingPK, c.Sender.D)
	if err != nil {
		return transferSharedContext{}, err
	}

	return shared, nil
}

func (c *TransferCircuit) newTransferStatementData() transferStatementData {
	return transferStatementData{
		inputAmounts:          make([]frontend.Variable, 0, c.nIn),
		outputAmounts:         make([]frontend.Variable, 0, c.nOut),
		outputCommitments:     make([]frontend.Variable, 0, c.nOut),
		spendStatementBlocks:  make([][]frontend.Variable, 0, c.nIn),
		outputStatementBlocks: make([][]frontend.Variable, 0, c.nOut),
		spendDLEQs:            make([]frontend.Variable, 0, 2*c.nIn),
		outputDLEQs:           make([]frontend.Variable, 0, 6*c.nOut),
		nullifiersAndRKs:      make([]frontend.Variable, 0, 2*c.nIn),
	}
}

func (c *TransferCircuit) verifyTransferSpend(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
	spend *TransferSpendCircuitFields,
) error {
	spentDivGen := gnarkte.Point{X: spend.Note.DivGen.X, Y: spend.Note.DivGen.Y}
	spentTransmission := gnarkte.Point{X: spend.Note.Transmission.X, Y: spend.Note.Transmission.Y}
	rkClaimed := gnarkte.Point{X: spend.RK.X, Y: spend.RK.Y}
	spendEPK := gnarkte.Point{X: spend.Enc.Epk.X, Y: spend.Enc.Epk.Y}

	spentDivGenFq, err := Decaf377CompressToField(api, spentDivGen)
	if err != nil {
		return err
	}
	spentTransmissionFq, err := Decaf377CompressToField(api, spentTransmission)
	if err != nil {
		return err
	}
	spendEPKFq, err := Decaf377CompressToField(api, spendEPK)
	if err != nil {
		return err
	}

	spentCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		spend.Note.Blinding,
		spend.Note.Amount,
		spend.Note.AssetID,
		spentDivGenFq,
		spend.Note.TransmissionKeyS,
		spend.Note.ClueKey,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(spentCommitment, spend.StateProof.Commitment)

	nullifier, err := Nullifier(api, c.Auth.NK, spend.StateProof.Commitment, spend.StateProof.Position)
	if err != nil {
		return err
	}
	api.AssertIsEqual(nullifier, spend.Nullifier)

	statePath := make([][3]frontend.Variable, len(spend.StateProof.Path))
	copy(statePath, spend.StateProof.Path[:])
	anchor, err := VerifyStateCommitmentPath(api, spend.StateProof.Commitment, spend.StateProof.Position, statePath)
	if err != nil {
		return err
	}
	isDummy := api.IsZero(spend.Note.Amount)
	isNotDummy := api.Sub(1, isDummy)
	AssertEqualIf(api, anchor, c.Anchor, isNotDummy)

	computedRK, err := RandomizedVerificationKey(api, shared.ak, spend.AuthRandomizer)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, computedRK, rkClaimed)

	computedSpentTransmission, err := DiversifiedTransmissionKey(
		api,
		c.Auth.NK,
		shared.ak,
		spentDivGen,
		c.Auth.IVKReduced,
		c.Auth.IVKQuotientA,
	)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, computedSpentTransmission, spentTransmission)

	api.AssertIsEqual(spend.Note.AssetID, shared.sharedAssetID)
	api.AssertIsEqual(c.Sender.AssetID, spend.Note.AssetID)
	api.AssertIsEqual(spend.Enc.IsRegulated, c.IsRegulated)

	AssertDecafEquivalent(api, shared.senderDivGen, spentDivGen)
	AssertDecafEquivalent(api, shared.senderTransmission, spentTransmission)

	VerifyThresholdFlagSimple(api, spend.Note.Amount, c.Asset.Leaf.Threshold, spend.Enc.IsFlagged)

	spendSSDetection, spendSSCoreUser, spendSSCore, err := DeriveSharedSecretsSpend(
		api,
		spend.Enc.ComplianceEphemeral,
		shared.senderAck,
		shared.indexedLeaf.DKPub,
		spend.Enc.IsFlagged,
		spendEPK,
	)
	if err != nil {
		return err
	}
	if err := VerifyPoseidonEncryptionSpend(
		api,
		spend.Enc.IsRegulated,
		spend.Enc.IsFlagged,
		spendSSDetection,
		spendSSCore,
		spend.Enc.C2Core,
		spendEPKFq,
		spend.Enc.Salt,
		spend.Note.Amount,
		spend.Note.AssetID,
		spentDivGenFq,
		spentTransmissionFq,
		spend.Enc.ComplianceCiphertext,
	); err != nil {
		return err
	}

	spendMetadataHash, err := ComputeMetadataHash(
		api,
		c.Asset.Leaf.PolicyIDHash,
		c.Asset.Leaf.ResourceHash,
		c.Asset.Leaf.PermissionHash,
		1,
		c.TargetTimestamp,
		spend.Enc.Salt,
	)
	if err != nil {
		return err
	}
	if err := VerifyDLEQ(
		api,
		spend.Enc.ComplianceEphemeral,
		shared.senderAck,
		spendSSCoreUser,
		spendEPK,
		spendMetadataHash,
		spend.Dleq.C,
		spend.Dleq.S,
		spend.Enc.IsRegulated,
	); err != nil {
		return err
	}

	statementData.inputAmounts = append(statementData.inputAmounts, spend.Note.Amount)
	statementData.nullifiersAndRKs = append(statementData.nullifiersAndRKs, spend.Nullifier)
	rkFq, err := Decaf377CompressToField(api, rkClaimed)
	if err != nil {
		return err
	}
	statementData.nullifiersAndRKs = append(statementData.nullifiersAndRKs, rkFq)

	spendStatement := make([]frontend.Variable, 0, 2+SpendCiphertextFQCount)
	spendStatement = append(spendStatement, spendEPKFq, spend.Enc.C2Core)
	for j := range spend.Enc.ComplianceCiphertext {
		spendStatement = append(spendStatement, spend.Enc.ComplianceCiphertext[j])
	}
	statementData.spendStatementBlocks = append(statementData.spendStatementBlocks, spendStatement)
	statementData.spendDLEQs = append(statementData.spendDLEQs, spend.Dleq.C, spend.Dleq.S)
	return nil
}

func (c *TransferCircuit) verifyTransferOutput(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
	output *TransferOutputCircuitFields,
) error {
	createdDivGen := gnarkte.Point{X: output.Note.DivGen.X, Y: output.Note.DivGen.Y}
	createdTransmission := gnarkte.Point{X: output.Note.Transmission.X, Y: output.Note.Transmission.Y}
	recipientDivGen := gnarkte.Point{X: output.Recipient.DivGen.X, Y: output.Recipient.DivGen.Y}
	recipientTransmission := gnarkte.Point{X: output.Recipient.Transmission.X, Y: output.Recipient.Transmission.Y}
	outputEPK1 := gnarkte.Point{X: output.Enc.Epk1.X, Y: output.Enc.Epk1.Y}
	outputEPK2 := gnarkte.Point{X: output.Enc.Epk2.X, Y: output.Enc.Epk2.Y}
	outputEPK3 := gnarkte.Point{X: output.Enc.Epk3.X, Y: output.Enc.Epk3.Y}

	createdDivGenFq, err := Decaf377CompressToField(api, createdDivGen)
	if err != nil {
		return err
	}
	createdTransmissionFq, err := Decaf377CompressToField(api, createdTransmission)
	if err != nil {
		return err
	}
	outputEPK1Fq, err := Decaf377CompressToField(api, outputEPK1)
	if err != nil {
		return err
	}
	outputEPK2Fq, err := Decaf377CompressToField(api, outputEPK2)
	if err != nil {
		return err
	}
	outputEPK3Fq, err := Decaf377CompressToField(api, outputEPK3)
	if err != nil {
		return err
	}

	createdCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		output.Note.Blinding,
		output.Note.Amount,
		output.Note.AssetID,
		createdDivGenFq,
		output.Note.TransmissionKeyS,
		output.Note.ClueKey,
	)
	if err != nil {
		return err
	}
	api.AssertIsEqual(createdCommitment, output.NoteCommitment)

	api.AssertIsEqual(output.Note.AssetID, shared.sharedAssetID)
	api.AssertIsEqual(output.Recipient.AssetID, output.Note.AssetID)
	api.AssertIsEqual(output.Enc.IsRegulated, c.IsRegulated)

	AssertDecafEquivalent(api, recipientDivGen, createdDivGen)
	AssertDecafEquivalent(api, recipientTransmission, createdTransmission)

	recipientLeafCommitment, err := ComplianceLeafCommitmentFromCompressed(
		api,
		createdDivGenFq,
		createdTransmissionFq,
		output.Recipient.AssetID,
		output.Recipient.D,
	)
	if err != nil {
		return err
	}
	recipientComplianceRoot, err := VerifyQuadPath(api, recipientLeafCommitment, output.Recipient.Path, output.Recipient.Position)
	if err != nil {
		return err
	}
	AssertEqualIf(api, recipientComplianceRoot, c.ComplianceAnchor, c.IsRegulated)

	VerifyThresholdFlagSimple(api, output.Note.Amount, c.Asset.Leaf.Threshold, output.Enc.IsFlagged)

	recipientAck, err := DeriveACKFromLeafD(api, shared.indexedLeaf.RingPK, output.Recipient.D)
	if err != nil {
		return err
	}

	outputSSDetection, outputSSCoreUser, outputSSExtUser, outputSSSextUser, outputSSCore, outputSSExt, outputSSSext, err := DeriveSharedSecretsOutput(
		api,
		output.Enc.ComplianceEphemeral,
		output.Enc.R2,
		output.Enc.R3,
		recipientAck,
		shared.senderAck,
		shared.indexedLeaf.DKPub,
		output.Enc.IsFlagged,
		outputEPK1,
		outputEPK2,
		outputEPK3,
	)
	if err != nil {
		return err
	}
	if err := VerifyPoseidonEncryptionOutput(
		api,
		output.Enc.IsRegulated,
		output.Enc.IsFlagged,
		outputSSDetection,
		outputSSCore,
		outputSSExt,
		outputSSSext,
		output.Enc.C2Core,
		output.Enc.C2Ext,
		output.Enc.C2Sext,
		outputEPK1Fq,
		output.Enc.Salt,
		output.Note.Amount,
		output.Note.AssetID,
		createdDivGenFq,
		createdTransmissionFq,
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
		output.Enc.ComplianceCiphertext,
	); err != nil {
		return err
	}

	outputMetadataCore, err := ComputeMetadataHash(
		api,
		c.Asset.Leaf.PolicyIDHash,
		c.Asset.Leaf.ResourceHash,
		c.Asset.Leaf.PermissionHash,
		1,
		c.TargetTimestamp,
		output.Enc.Salt,
	)
	if err != nil {
		return err
	}
	outputMetadataExt, err := ComputeMetadataHash(
		api,
		c.Asset.Leaf.PolicyIDHash,
		c.Asset.Leaf.ResourceHash,
		c.Asset.Leaf.PermissionHash,
		2,
		c.TargetTimestamp,
		output.Enc.Salt,
	)
	if err != nil {
		return err
	}
	outputMetadataSext, err := ComputeMetadataHash(
		api,
		c.Asset.Leaf.PolicyIDHash,
		c.Asset.Leaf.ResourceHash,
		c.Asset.Leaf.PermissionHash,
		3,
		c.TargetTimestamp,
		output.Enc.Salt,
	)
	if err != nil {
		return err
	}
	if err := VerifyDLEQ(api, output.Enc.ComplianceEphemeral, recipientAck, outputSSCoreUser, outputEPK1, outputMetadataCore, output.Dleq.Core.C, output.Dleq.Core.S, output.Enc.IsRegulated); err != nil {
		return err
	}
	if err := VerifyDLEQ(api, output.Enc.R2, recipientAck, outputSSExtUser, outputEPK2, outputMetadataExt, output.Dleq.Ext.C, output.Dleq.Ext.S, output.Enc.IsRegulated); err != nil {
		return err
	}
	if err := VerifyDLEQ(api, output.Enc.R3, shared.senderAck, outputSSSextUser, outputEPK3, outputMetadataSext, output.Dleq.Sext.C, output.Dleq.Sext.S, output.Enc.IsRegulated); err != nil {
		return err
	}

	statementData.outputAmounts = append(statementData.outputAmounts, output.Note.Amount)
	statementData.outputCommitments = append(statementData.outputCommitments, output.NoteCommitment)

	outputStatement := make([]frontend.Variable, 0, 6+len(output.Enc.ComplianceCiphertext))
	outputStatement = append(
		outputStatement,
		outputEPK1Fq,
		outputEPK2Fq,
		outputEPK3Fq,
		output.Enc.C2Core,
		output.Enc.C2Ext,
		output.Enc.C2Sext,
	)
	for j := range output.Enc.ComplianceCiphertext {
		outputStatement = append(outputStatement, output.Enc.ComplianceCiphertext[j])
	}
	statementData.outputStatementBlocks = append(statementData.outputStatementBlocks, outputStatement)
	statementData.outputDLEQs = append(
		statementData.outputDLEQs,
		output.Dleq.Core.C,
		output.Dleq.Core.S,
		output.Dleq.Ext.C,
		output.Dleq.Ext.S,
		output.Dleq.Sext.C,
		output.Dleq.Sext.S,
	)
	return nil
}

func (c *TransferCircuit) assertTransferNetBalanceCommitment(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
) (frontend.Variable, error) {
	netBalanceCommitment, err := computeTransferNetBalanceCommitment(
		api,
		statementData.inputAmounts,
		statementData.outputAmounts,
		shared.sharedAssetID,
		c.ActionBalanceBlinding,
	)
	if err != nil {
		return nil, err
	}
	AssertDecafEquivalent(api, netBalanceCommitment, shared.claimedBalanceCommitment)

	balanceCommitmentFq, err := Decaf377CompressToField(api, netBalanceCommitment)
	if err != nil {
		return nil, err
	}
	return balanceCommitmentFq, nil
}

func (c *TransferCircuit) buildTransferStatementFields(
	balanceCommitmentFq frontend.Variable,
	statementData *transferStatementData,
) []frontend.Variable {
	fields := make([]frontend.Variable, 0, transferStatementFieldCountForShape(c.nIn, c.nOut))
	fields = append(fields, c.Anchor)
	fields = append(fields, statementData.outputCommitments...)
	fields = append(fields, balanceCommitmentFq)
	fields = append(fields, statementData.nullifiersAndRKs...)
	fields = append(fields, c.AssetAnchor, c.ComplianceAnchor)
	for _, block := range statementData.spendStatementBlocks {
		fields = append(fields, block...)
	}
	for _, block := range statementData.outputStatementBlocks {
		fields = append(fields, block...)
	}
	fields = append(fields, c.TargetTimestamp)
	fields = append(fields, statementData.spendDLEQs...)
	fields = append(fields, statementData.outputDLEQs...)
	return fields
}
