package circuits

import (
	"fmt"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/mizufinance/penumbra/tools/gnark/internal/compliance"
	. "github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
	"golang.org/x/crypto/blake2b"
)

type TransferAuthSharedFields struct {
	AK           Point2D
	NK           frontend.Variable
	IVKReduced   frontend.Variable
	IVKQuotientA frontend.Variable
}

type TransferSpendCircuitFields struct {
	Nullifier          frontend.Variable
	RK                 Point2D
	Note               NoteFields
	StateProof         StateCommitmentFields
	AuthRandomizer     frontend.Variable
	IsDummy            frontend.Variable
	DummyNullifierSeed frontend.Variable
	DummySpendAuthKey  frontend.Variable
}

type TransferOutputCircuitFields struct {
	NoteCommitment frontend.Variable
	Note           NoteFields
	Recipient      UserComplianceFields
	IsReceiver     frontend.Variable
}

const (
	TransferCircuitInputs  = 2
	TransferCircuitOutputs = 2
)

type TransferCircuit struct {
	ClaimedStatementHash frontend.Variable `gnark:",public"`

	Anchor                frontend.Variable
	BalanceCommitment     Point2D
	AssetAnchor           frontend.Variable
	ComplianceAnchor      frontend.Variable
	TargetTimestamp       frontend.Variable
	ActionBalanceBlinding frontend.Variable
	IsRegulated           frontend.Variable

	Auth       TransferAuthSharedFields
	Asset      AssetTreeFields
	Sender     UserComplianceFields
	Compliance TransferComplianceFields

	Spends  []TransferSpendCircuitFields
	Outputs []TransferOutputCircuitFields
}

func transferStatementFieldCount() int {
	return TransferStatementBaseFields +
		TransferStatementFieldsPerInput*TransferCircuitInputs +
		TransferStatementFieldsPerOutput*TransferCircuitOutputs
}

func NewTransferCircuit() *TransferCircuit {
	return &TransferCircuit{
		Spends:  make([]TransferSpendCircuitFields, TransferCircuitInputs),
		Outputs: make([]TransferOutputCircuitFields, TransferCircuitOutputs),
	}
}

func (c *TransferCircuit) Define(api frontend.API) error {
	if len(c.Spends) != TransferCircuitInputs || len(c.Outputs) != TransferCircuitOutputs {
		return fmt.Errorf(
			"transfer circuit must be fixed %dx%d, got %dx%d",
			TransferCircuitInputs,
			TransferCircuitOutputs,
			len(c.Spends),
			len(c.Outputs),
		)
	}

	shared, err := c.verifySharedTransferContext(api)
	if err != nil {
		return err
	}
	statementData := c.newTransferStatementData()

	for i := range c.Spends {
		if err := c.verifyTransferSpend(api, &shared, &statementData, &c.Spends[i], i); err != nil {
			return err
		}
	}
	if err := c.verifyTransferOutput(api, &shared, &statementData, &c.Outputs[0], 0); err != nil {
		return err
	}
	if err := c.verifyTransferOutput(api, &shared, &statementData, &c.Outputs[1], 1); err != nil {
		return err
	}
	if err := c.verifyTransferComplianceCiphertexts(api, &shared, &statementData); err != nil {
		return err
	}

	balanceCommitmentFq, err := c.assertTransferNetBalanceCommitment(api, &shared, &statementData)
	if err != nil {
		return err
	}

	fields, err := c.buildTransferStatementFields(api, balanceCommitmentFq, &statementData)
	if err != nil {
		return err
	}
	statementHash, err := TransferStatementHashForShape(
		api,
		TransferCircuitInputs,
		TransferCircuitOutputs,
		fields,
	)
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
	effectiveDKPub           gnarkte.Point
	effectiveRingPK          gnarkte.Point
	senderDivGen             gnarkte.Point
	senderTransmission       gnarkte.Point
	senderDivGenFq           frontend.Variable
	senderTransmissionFq     frontend.Variable
	senderAck                gnarkte.Point
	sharedAssetID            frontend.Variable
}

type transferStatementData struct {
	inputAmounts           []frontend.Variable
	outputAmounts          []frontend.Variable
	outputCommitments      []frontend.Variable
	nullifiersAndRKs       []frontend.Variable
	receiverAmount         frontend.Variable
	receiverDivGenFq       frontend.Variable
	receiverTransmissionFq frontend.Variable
	receiverSlotID         frontend.Variable
	receiverSlotDerivation frontend.Variable
	receiverAck            gnarkte.Point
	senderCoreEPKFq        frontend.Variable
	senderExtEPKFq         frontend.Variable
	outputCoreEPKFq        frontend.Variable
	outputExtEPKFq         frontend.Variable
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
	valueGenerator, err := decafgnark.EncodeToCurve(api, hashedAssetID)
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
			SlotCount:      c.Asset.Leaf.SlotCount,
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

	unregulatedRingPK, unregulatedDKPub, err := UnregulatedComplianceKeys()
	if err != nil {
		return transferSharedContext{}, err
	}
	shared.effectiveRingPK = SelectPoint(api, c.IsRegulated, shared.indexedLeaf.RingPK, unregulatedRingPK)
	shared.effectiveDKPub = SelectPoint(api, c.IsRegulated, shared.indexedLeaf.DKPub, unregulatedDKPub)

	shared.senderDivGenFq, err = decafgnark.CompressToField(api, shared.senderDivGen)
	if err != nil {
		return transferSharedContext{}, err
	}
	shared.senderTransmissionFq, err = decafgnark.CompressToField(api, shared.senderTransmission)
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
		c.Sender.SlotID,
		c.Sender.SlotDerivation,
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

	shared.senderAck, err = DeriveACKFromLeafD(api, shared.effectiveRingPK, c.Sender.D)
	if err != nil {
		return transferSharedContext{}, err
	}

	return shared, nil
}

func (c *TransferCircuit) newTransferStatementData() transferStatementData {
	return transferStatementData{
		inputAmounts:           make([]frontend.Variable, 0, TransferCircuitInputs),
		outputAmounts:          make([]frontend.Variable, 0, TransferCircuitOutputs),
		outputCommitments:      make([]frontend.Variable, 0, TransferCircuitOutputs),
		nullifiersAndRKs:       make([]frontend.Variable, 0, 2*TransferCircuitInputs),
		receiverAmount:         0,
		receiverDivGenFq:       0,
		receiverTransmissionFq: 0,
		receiverSlotID:         0,
		receiverSlotDerivation: 0,
		receiverAck:            gnarkte.Point{X: 0, Y: 0},
		senderCoreEPKFq:        0,
		senderExtEPKFq:         0,
		outputCoreEPKFq:        0,
		outputExtEPKFq:         0,
	}
}

func transferSyntheticDummyNullifierDomain() *big.Int {
	sum := blake2b.Sum512([]byte("penumbra.transfer.synthetic_dummy.nullifier"))
	return LittleEndianBytesToBigInt(sum[:])
}

func syntheticDummyNullifier(
	api frontend.API,
	seed frontend.Variable,
	authRandomizer frontend.Variable,
	slotIndex int,
) (frontend.Variable, error) {
	return Poseidon377Hash3(
		api,
		transferSyntheticDummyNullifierDomain(),
		[3]frontend.Variable{seed, authRandomizer, slotIndex},
	)
}

func syntheticDummyVerificationKey(
	api frontend.API,
	spendAuthKey frontend.Variable,
	authRandomizer frontend.Variable,
) (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	generator := gnarkte.Point{
		X: MustBigInt(vectors.Decaf377CompanionCurve.GeneratorX),
		Y: MustBigInt(vectors.Decaf377CompanionCurve.GeneratorY),
	}
	dummyAK := ScalarMulLE(
		api,
		curve,
		generator,
		spendAuthKey,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	return RandomizedVerificationKey(api, dummyAK, authRandomizer)
}

func (c *TransferCircuit) verifyTransferSpend(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
	spend *TransferSpendCircuitFields,
	index int,
) error {
	spentDivGen := gnarkte.Point{X: spend.Note.DivGen.X, Y: spend.Note.DivGen.Y}
	spentTransmission := gnarkte.Point{X: spend.Note.Transmission.X, Y: spend.Note.Transmission.Y}
	rkClaimed := gnarkte.Point{X: spend.RK.X, Y: spend.RK.Y}
	api.AssertIsBoolean(spend.IsDummy)
	isNotDummy := api.Sub(1, spend.IsDummy)

	spentDivGenFq, err := decafgnark.CompressToField(api, spentDivGen)
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
	AssertEqualIf(api, spentCommitment, spend.StateProof.Commitment, isNotDummy)

	realNullifier, err := Nullifier(api, c.Auth.NK, spend.StateProof.Commitment, spend.StateProof.Position)
	if err != nil {
		return err
	}

	statePath := make([][3]frontend.Variable, len(spend.StateProof.Path))
	copy(statePath, spend.StateProof.Path[:])
	anchor, err := VerifyStateCommitmentPath(api, spend.StateProof.Commitment, spend.StateProof.Position, statePath)
	if err != nil {
		return err
	}
	syntheticNullifier, err := syntheticDummyNullifier(api, spend.DummyNullifierSeed, spend.AuthRandomizer, index)
	if err != nil {
		return err
	}
	api.AssertIsEqual(
		spend.Nullifier,
		api.Add(
			api.Mul(isNotDummy, realNullifier),
			api.Mul(spend.IsDummy, syntheticNullifier),
		),
	)
	AssertEqualIf(api, anchor, c.Anchor, isNotDummy)

	computedRK, err := RandomizedVerificationKey(api, shared.ak, spend.AuthRandomizer)
	if err != nil {
		return err
	}
	dummyRK, err := syntheticDummyVerificationKey(api, spend.DummySpendAuthKey, spend.AuthRandomizer)
	if err != nil {
		return err
	}
	decafgnark.AssertEquivalentIf(api, computedRK, rkClaimed, isNotDummy)
	decafgnark.AssertEquivalentIf(api, dummyRK, rkClaimed, spend.IsDummy)

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
	decafgnark.AssertEquivalentIf(api, computedSpentTransmission, spentTransmission, isNotDummy)
	AssertEqualIf(api, spend.Note.Amount, 0, spend.IsDummy)

	api.AssertIsEqual(spend.Note.AssetID, shared.sharedAssetID)
	api.AssertIsEqual(c.Sender.AssetID, spend.Note.AssetID)
	decafgnark.AssertEquivalentIf(api, shared.senderDivGen, spentDivGen, 1)
	decafgnark.AssertEquivalentIf(api, shared.senderTransmission, spentTransmission, 1)

	statementData.inputAmounts = append(statementData.inputAmounts, spend.Note.Amount)
	statementData.nullifiersAndRKs = append(statementData.nullifiersAndRKs, spend.Nullifier)
	rkFq, err := decafgnark.CompressToField(api, rkClaimed)
	if err != nil {
		return err
	}
	statementData.nullifiersAndRKs = append(statementData.nullifiersAndRKs, rkFq)
	return nil
}

func (c *TransferCircuit) verifyTransferOutput(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
	output *TransferOutputCircuitFields,
	index int,
) error {
	createdDivGen := gnarkte.Point{X: output.Note.DivGen.X, Y: output.Note.DivGen.Y}
	createdTransmission := gnarkte.Point{X: output.Note.Transmission.X, Y: output.Note.Transmission.Y}
	recipientDivGen := gnarkte.Point{X: output.Recipient.DivGen.X, Y: output.Recipient.DivGen.Y}
	recipientTransmission := gnarkte.Point{X: output.Recipient.Transmission.X, Y: output.Recipient.Transmission.Y}

	expectedReceiver := 0
	if index == 0 {
		expectedReceiver = 1
	}
	api.AssertIsEqual(output.IsReceiver, expectedReceiver)
	isDummy := api.IsZero(output.Note.Amount)

	createdDivGenFq, err := decafgnark.CompressToField(api, createdDivGen)
	if err != nil {
		return err
	}
	createdTransmissionFq, err := decafgnark.CompressToField(api, createdTransmission)
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
	decafgnark.AssertEquivalentIf(api, recipientDivGen, createdDivGen, 1)
	decafgnark.AssertEquivalentIf(api, recipientTransmission, createdTransmission, 1)

	recipientLeafCommitment, err := ComplianceLeafCommitmentFromCompressed(
		api,
		createdDivGenFq,
		createdTransmissionFq,
		output.Recipient.AssetID,
		output.Recipient.SlotID,
		output.Recipient.SlotDerivation,
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

	statementData.outputAmounts = append(statementData.outputAmounts, output.Note.Amount)
	statementData.outputCommitments = append(statementData.outputCommitments, output.NoteCommitment)

	if index == 0 {
		api.AssertIsEqual(isDummy, 0)
		recipientAck, err := DeriveACKFromLeafD(api, shared.effectiveRingPK, output.Recipient.D)
		if err != nil {
			return err
		}
		statementData.receiverAmount = output.Note.Amount
		statementData.receiverDivGenFq = createdDivGenFq
		statementData.receiverTransmissionFq = createdTransmissionFq
		statementData.receiverSlotID = output.Recipient.SlotID
		statementData.receiverSlotDerivation = output.Recipient.SlotDerivation
		statementData.receiverAck = recipientAck
		return nil
	}

	api.AssertIsEqual(output.Recipient.AssetID, c.Sender.AssetID)
	api.AssertIsEqual(output.Recipient.D, c.Sender.D)
	decafgnark.AssertEquivalentIf(api, recipientDivGen, shared.senderDivGen, 1)
	decafgnark.AssertEquivalentIf(api, recipientTransmission, shared.senderTransmission, 1)
	return nil
}

func (c *TransferCircuit) verifyTransferComplianceCiphertexts(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
) error {
	isFlagged := ThresholdFlag(api, statementData.receiverAmount, shared.indexedLeaf.Threshold)

	senderCoreEPK := gnarkte.Point{X: c.Compliance.SenderCore.Epk.X, Y: c.Compliance.SenderCore.Epk.Y}
	senderExtEPK := gnarkte.Point{X: c.Compliance.SenderExt.Epk.X, Y: c.Compliance.SenderExt.Epk.Y}
	outputCoreEPK := gnarkte.Point{X: c.Compliance.OutputCore.Epk.X, Y: c.Compliance.OutputCore.Epk.Y}
	outputExtEPK := gnarkte.Point{X: c.Compliance.OutputExt.Epk.X, Y: c.Compliance.OutputExt.Epk.Y}

	senderCoreEPKFq, err := decafgnark.CompressToField(api, senderCoreEPK)
	if err != nil {
		return err
	}
	senderExtEPKFq, err := decafgnark.CompressToField(api, senderExtEPK)
	if err != nil {
		return err
	}
	outputCoreEPKFq, err := decafgnark.CompressToField(api, outputCoreEPK)
	if err != nil {
		return err
	}
	outputExtEPKFq, err := decafgnark.CompressToField(api, outputExtEPK)
	if err != nil {
		return err
	}
	statementData.senderCoreEPKFq = senderCoreEPKFq
	statementData.senderExtEPKFq = senderExtEPKFq
	statementData.outputCoreEPKFq = outputCoreEPKFq
	statementData.outputExtEPKFq = outputExtEPKFq

	saltLabels := [5]*big.Int{
		TransferDetectionSaltLabel,
		TransferSenderCoreSaltLabel,
		TransferSenderExtSaltLabel,
		TransferOutputCoreSaltLabel,
		TransferOutputExtSaltLabel,
	}
	salts := [5]frontend.Variable{}
	for i, label := range saltLabels {
		salts[i], err = DeriveTransferSalt(api, c.Compliance.TransferNonceRoot, label)
		if err != nil {
			return err
		}
	}

	ssDetection, _, senderCoreShared, err := DeriveSharedSecretsSpend(
		api,
		c.Compliance.SenderRCore,
		shared.senderAck,
		shared.effectiveDKPub,
		isFlagged,
		senderCoreEPK,
	)
	if err != nil {
		return err
	}
	_, _, senderExtShared, err := DeriveSharedSecretsSpend(
		api,
		c.Compliance.SenderRExt,
		shared.senderAck,
		shared.effectiveDKPub,
		isFlagged,
		senderExtEPK,
	)
	if err != nil {
		return err
	}
	_, _, outputCoreShared, err := DeriveSharedSecretsSpend(
		api,
		c.Compliance.OutputRCore,
		statementData.receiverAck,
		shared.effectiveDKPub,
		isFlagged,
		outputCoreEPK,
	)
	if err != nil {
		return err
	}
	_, _, outputExtShared, err := DeriveSharedSecretsSpend(
		api,
		c.Compliance.OutputRExt,
		statementData.receiverAck,
		shared.effectiveDKPub,
		isFlagged,
		outputExtEPK,
	)
	if err != nil {
		return err
	}

	if err := VerifyPoseidonEncryptionTransferDetection(
		api,
		c.IsRegulated,
		isFlagged,
		ssDetection,
		statementData.senderCoreEPKFq,
		salts[0],
		shared.sharedAssetID,
		c.Sender.SlotID,
		statementData.receiverSlotID,
		c.Compliance.DetectionCiphertext,
	); err != nil {
		return err
	}
	if err := VerifyPoseidonEncryptionTransferAmount(
		api,
		c.IsRegulated,
		senderCoreShared,
		c.Compliance.SenderCore.C2,
		statementData.receiverAmount,
		c.Compliance.SenderCore.Ciphertext,
	); err != nil {
		return err
	}
	if err := VerifyPoseidonEncryptionTransferAddress(
		api,
		c.IsRegulated,
		senderExtShared,
		c.Compliance.SenderExt.C2,
		statementData.receiverDivGenFq,
		statementData.receiverTransmissionFq,
		c.Compliance.SenderExt.Ciphertext,
	); err != nil {
		return err
	}
	if err := VerifyPoseidonEncryptionTransferAmount(
		api,
		c.IsRegulated,
		outputCoreShared,
		c.Compliance.OutputCore.C2,
		statementData.receiverAmount,
		c.Compliance.OutputCore.Ciphertext,
	); err != nil {
		return err
	}
	if err := VerifyPoseidonEncryptionTransferAddress(
		api,
		c.IsRegulated,
		outputExtShared,
		c.Compliance.OutputExt.C2,
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
		c.Compliance.OutputExt.Ciphertext,
	); err != nil {
		return err
	}

	decafgnark.AssertEquivalent(api, gnarkte.Point{X: c.Compliance.SenderCore.Epk.X, Y: c.Compliance.SenderCore.Epk.Y}, gnarkte.Point{X: c.Compliance.SenderCore.Proof.EncCmt.X, Y: c.Compliance.SenderCore.Proof.EncCmt.Y})
	decafgnark.AssertEquivalent(api, gnarkte.Point{X: c.Compliance.SenderExt.Epk.X, Y: c.Compliance.SenderExt.Epk.Y}, gnarkte.Point{X: c.Compliance.SenderExt.Proof.EncCmt.X, Y: c.Compliance.SenderExt.Proof.EncCmt.Y})
	decafgnark.AssertEquivalent(api, gnarkte.Point{X: c.Compliance.OutputCore.Epk.X, Y: c.Compliance.OutputCore.Epk.Y}, gnarkte.Point{X: c.Compliance.OutputCore.Proof.EncCmt.X, Y: c.Compliance.OutputCore.Proof.EncCmt.Y})
	decafgnark.AssertEquivalent(api, gnarkte.Point{X: c.Compliance.OutputExt.Epk.X, Y: c.Compliance.OutputExt.Epk.Y}, gnarkte.Point{X: c.Compliance.OutputExt.Proof.EncCmt.X, Y: c.Compliance.OutputExt.Proof.EncCmt.Y})
	decafgnark.AssertEquivalent(api, shared.senderAck, gnarkte.Point{X: c.Compliance.SenderCore.Proof.DerivedPK.X, Y: c.Compliance.SenderCore.Proof.DerivedPK.Y})
	decafgnark.AssertEquivalent(api, shared.senderAck, gnarkte.Point{X: c.Compliance.SenderExt.Proof.DerivedPK.X, Y: c.Compliance.SenderExt.Proof.DerivedPK.Y})
	decafgnark.AssertEquivalent(api, statementData.receiverAck, gnarkte.Point{X: c.Compliance.OutputCore.Proof.DerivedPK.X, Y: c.Compliance.OutputCore.Proof.DerivedPK.Y})
	decafgnark.AssertEquivalent(api, statementData.receiverAck, gnarkte.Point{X: c.Compliance.OutputExt.Proof.DerivedPK.X, Y: c.Compliance.OutputExt.Proof.DerivedPK.Y})

	verifyProofStatement := func(
		proof TransferComplianceProofFields,
		expectedSubjectDerivation frontend.Variable,
		expectedTier frontend.Variable,
		expectedSalt frontend.Variable,
	) (frontend.Variable, error) {
		api.AssertIsEqual(proof.Statement.SubjectDerivation, expectedSubjectDerivation)
		api.AssertIsEqual(proof.Statement.RingIDHash, shared.indexedLeaf.RingIDHash)
		api.AssertIsEqual(proof.Statement.PolicyIDHash, shared.indexedLeaf.PolicyIDHash)
		api.AssertIsEqual(proof.Statement.ResourceHash, shared.indexedLeaf.ResourceHash)
		api.AssertIsEqual(proof.Statement.PermissionHash, shared.indexedLeaf.PermissionHash)
		api.AssertIsEqual(proof.Statement.Tier, expectedTier)
		api.AssertIsEqual(proof.Statement.TargetTimestamp, c.TargetTimestamp)
		api.AssertIsEqual(proof.Statement.Salt, expectedSalt)
		return ComputeMetadataHash(
			api,
			proof.Statement.PolicyIDHash,
			proof.Statement.ResourceHash,
			proof.Statement.PermissionHash,
			proof.Statement.Tier,
			proof.Statement.TargetTimestamp,
			proof.Statement.Salt,
		)
	}

	senderCoreMetadataHash, err := verifyProofStatement(
		c.Compliance.SenderCore.Proof,
		c.Sender.SlotDerivation,
		1,
		salts[1],
	)
	if err != nil {
		return err
	}
	senderExtMetadataHash, err := verifyProofStatement(
		c.Compliance.SenderExt.Proof,
		c.Sender.SlotDerivation,
		2,
		salts[2],
	)
	if err != nil {
		return err
	}
	outputCoreMetadataHash, err := verifyProofStatement(
		c.Compliance.OutputCore.Proof,
		statementData.receiverSlotDerivation,
		3,
		salts[3],
	)
	if err != nil {
		return err
	}
	outputExtMetadataHash, err := verifyProofStatement(
		c.Compliance.OutputExt.Proof,
		statementData.receiverSlotDerivation,
		4,
		salts[4],
	)
	if err != nil {
		return err
	}

	if err := VerifyDLEQ(
		api,
		c.Compliance.SenderRCore,
		gnarkte.Point{X: c.Compliance.SenderCore.Proof.DerivedPK.X, Y: c.Compliance.SenderCore.Proof.DerivedPK.Y},
		gnarkte.Point{X: c.Compliance.SenderCore.Proof.SharedPoint.X, Y: c.Compliance.SenderCore.Proof.SharedPoint.Y},
		gnarkte.Point{X: c.Compliance.SenderCore.Proof.EncCmt.X, Y: c.Compliance.SenderCore.Proof.EncCmt.Y},
		senderCoreMetadataHash,
		c.Compliance.SenderCore.Proof.Challenge,
		c.Compliance.SenderCore.Proof.Response,
		c.IsRegulated,
	); err != nil {
		return err
	}
	if err := VerifyDLEQ(
		api,
		c.Compliance.SenderRExt,
		gnarkte.Point{X: c.Compliance.SenderExt.Proof.DerivedPK.X, Y: c.Compliance.SenderExt.Proof.DerivedPK.Y},
		gnarkte.Point{X: c.Compliance.SenderExt.Proof.SharedPoint.X, Y: c.Compliance.SenderExt.Proof.SharedPoint.Y},
		gnarkte.Point{X: c.Compliance.SenderExt.Proof.EncCmt.X, Y: c.Compliance.SenderExt.Proof.EncCmt.Y},
		senderExtMetadataHash,
		c.Compliance.SenderExt.Proof.Challenge,
		c.Compliance.SenderExt.Proof.Response,
		c.IsRegulated,
	); err != nil {
		return err
	}
	if err := VerifyDLEQ(
		api,
		c.Compliance.OutputRCore,
		gnarkte.Point{X: c.Compliance.OutputCore.Proof.DerivedPK.X, Y: c.Compliance.OutputCore.Proof.DerivedPK.Y},
		gnarkte.Point{X: c.Compliance.OutputCore.Proof.SharedPoint.X, Y: c.Compliance.OutputCore.Proof.SharedPoint.Y},
		gnarkte.Point{X: c.Compliance.OutputCore.Proof.EncCmt.X, Y: c.Compliance.OutputCore.Proof.EncCmt.Y},
		outputCoreMetadataHash,
		c.Compliance.OutputCore.Proof.Challenge,
		c.Compliance.OutputCore.Proof.Response,
		c.IsRegulated,
	); err != nil {
		return err
	}
	if err := VerifyDLEQ(
		api,
		c.Compliance.OutputRExt,
		gnarkte.Point{X: c.Compliance.OutputExt.Proof.DerivedPK.X, Y: c.Compliance.OutputExt.Proof.DerivedPK.Y},
		gnarkte.Point{X: c.Compliance.OutputExt.Proof.SharedPoint.X, Y: c.Compliance.OutputExt.Proof.SharedPoint.Y},
		gnarkte.Point{X: c.Compliance.OutputExt.Proof.EncCmt.X, Y: c.Compliance.OutputExt.Proof.EncCmt.Y},
		outputExtMetadataHash,
		c.Compliance.OutputExt.Proof.Challenge,
		c.Compliance.OutputExt.Proof.Response,
		c.IsRegulated,
	); err != nil {
		return err
	}

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
	decafgnark.AssertEquivalent(api, netBalanceCommitment, shared.claimedBalanceCommitment)

	balanceCommitmentFq, err := decafgnark.CompressToField(api, netBalanceCommitment)
	if err != nil {
		return nil, err
	}
	return balanceCommitmentFq, nil
}

func (c *TransferCircuit) buildTransferStatementFields(
	api frontend.API,
	balanceCommitmentFq frontend.Variable,
	statementData *transferStatementData,
) ([]frontend.Variable, error) {
	fields := make([]frontend.Variable, 0, transferStatementFieldCount())
	fields = append(fields, c.Anchor)
	fields = append(fields, statementData.outputCommitments...)
	fields = append(fields, balanceCommitmentFq)
	fields = append(fields, statementData.nullifiersAndRKs...)
	fields = append(fields, c.AssetAnchor, c.ComplianceAnchor)
	fields = append(fields, c.Compliance.DetectionCiphertext[:]...)

	appendCoreTier := func(epkFq frontend.Variable, tier TransferComplianceCoreFields) {
		fields = append(fields, epkFq, tier.C2)
		fields = append(fields, tier.Ciphertext[:]...)
	}
	appendExtTier := func(epkFq frontend.Variable, tier TransferComplianceExtFields) {
		fields = append(fields, epkFq, tier.C2)
		fields = append(fields, tier.Ciphertext[:]...)
	}
	appendProofTier := func(proof TransferComplianceProofFields) error {
		derivedPKFq, err := decafgnark.CompressToField(
			api,
			gnarkte.Point{X: proof.DerivedPK.X, Y: proof.DerivedPK.Y},
		)
		if err != nil {
			return err
		}
		encCmtFq, err := decafgnark.CompressToField(
			api,
			gnarkte.Point{X: proof.EncCmt.X, Y: proof.EncCmt.Y},
		)
		if err != nil {
			return err
		}
		sharedPointFq, err := decafgnark.CompressToField(
			api,
			gnarkte.Point{X: proof.SharedPoint.X, Y: proof.SharedPoint.Y},
		)
		if err != nil {
			return err
		}
		fields = append(
			fields,
			proof.Statement.SubjectDerivation,
			proof.Statement.RingIDHash,
			proof.Statement.PolicyIDHash,
			proof.Statement.ResourceHash,
			proof.Statement.PermissionHash,
			proof.Statement.Tier,
			proof.Statement.TargetTimestamp,
			proof.Statement.Salt,
			derivedPKFq,
			encCmtFq,
			sharedPointFq,
			proof.Challenge,
			proof.Response,
		)
		return nil
	}

	appendCoreTier(statementData.senderCoreEPKFq, c.Compliance.SenderCore)
	appendExtTier(statementData.senderExtEPKFq, c.Compliance.SenderExt)
	appendCoreTier(statementData.outputCoreEPKFq, c.Compliance.OutputCore)
	appendExtTier(statementData.outputExtEPKFq, c.Compliance.OutputExt)
	fields = append(fields, c.TargetTimestamp)
	if err := appendProofTier(c.Compliance.SenderCore.Proof); err != nil {
		return nil, err
	}
	if err := appendProofTier(c.Compliance.SenderExt.Proof); err != nil {
		return nil, err
	}
	if err := appendProofTier(c.Compliance.OutputCore.Proof); err != nil {
		return nil, err
	}
	if err := appendProofTier(c.Compliance.OutputExt.Proof); err != nil {
		return nil, err
	}

	expected := transferStatementFieldCount()
	if len(fields) != expected {
		return nil, fmt.Errorf("expected %d transfer statement fields, got %d", expected, len(fields))
	}
	return fields, nil
}
