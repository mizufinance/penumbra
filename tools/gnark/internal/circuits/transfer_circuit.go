package circuits

import (
	"fmt"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"golang.org/x/crypto/blake2b"
)

type TransferAuthSharedFields struct {
	AK           Point2D
	NK           frontend.Variable
	IVKReduced   frontend.Variable
	IVKQuotientA frontend.Variable
}

type TransferNotePayloadCircuitFields struct {
	Blinding           frontend.Variable
	Amount             frontend.Variable
	RecoveryCommitment frontend.Variable
}

type TransferStatePathCircuitFields struct {
	Position frontend.Variable
	Path     [StateCommitmentDepth][3]frontend.Variable
}

type TransferRequiredSpendNoteCircuitFields struct {
	TransferNotePayloadCircuitFields
	AssetID frontend.Variable
}

type TransferRequiredSpendCircuitFields struct {
	Nullifier       frontend.Variable
	RK              Point2D
	Note            TransferRequiredSpendNoteCircuitFields
	StateProof      TransferStatePathCircuitFields
	AuthRandomizer  frontend.Variable
	HistoryRequired frontend.Variable
}

type TransferOptionalSpendCircuitFields struct {
	Nullifier          frontend.Variable
	RK                 Point2D
	Note               TransferNotePayloadCircuitFields
	StateProof         TransferStatePathCircuitFields
	AuthRandomizer     frontend.Variable
	IsDummy            frontend.Variable
	DummyNullifierSeed frontend.Variable
	HistoryRequired    frontend.Variable
}

type TransferUserCircuitFields struct {
	DivGen        Point2D
	Transmission  Point2D
	Capk          Point2D
	RnkDhPk       Point2D
	RnkCommitment frontend.Variable
	Status        frontend.Variable
	Path          [ComplianceQuadTreeDepth][3]frontend.Variable
	Position      frontend.Variable
}

type TransferReceiverOutputCircuitFields struct {
	NoteCommitment frontend.Variable
	Note           TransferReceiverNoteCircuitFields
	Recipient      TransferUserCircuitFields
	Recovery       RecoveryCapsuleFields
}

type TransferReceiverNoteCircuitFields struct {
	TransferNotePayloadCircuitFields
}

type TransferChangeOutputCircuitFields struct {
	NoteCommitment frontend.Variable
	Note           TransferNotePayloadCircuitFields
	Recovery       RecoveryCapsuleFields
}

type TransferVolumeAccumulatorCircuitFields struct {
	Nullifier         frontend.Variable
	Commitment        frontend.Variable
	DayStart          frontend.Variable
	ProofContext      frontend.Variable
	UseReal           frontend.Variable
	StartsNewDay      frontend.Variable
	TimestampDayIndex frontend.Variable
	TimestampSecond   frontend.Variable
	Subject           frontend.Variable
	PriorVolume       frontend.Variable
	PriorBlinding     frontend.Variable
	PriorCommitment   frontend.Variable
	PriorStateProof   TransferStatePathCircuitFields
	SuccessorVolume   frontend.Variable
	SuccessorBlinding frontend.Variable
}

const (
	TransferCircuitInputs  = 2
	TransferCircuitOutputs = 2
	transferAmountBits     = 128
	transferAmountSumBits  = transferAmountBits + 1
	// BLAKE2b-512(personal="pen.imt.str_hash", input=""), reduced into Fq.
	// This must match compliance::indexed_tree::string_to_fq("") in Rust.
	transferUnregulatedPolicyHashDecimal = "146917404083759493085774782844141783124558702674359650395366636584294839859"
)

type TransferCircuit struct {
	wiringTrace *WiringTranscript

	ClaimedStatementHash  frontend.Variable `gnark:",public"`
	RoutingTags           [2]frontend.Variable
	RoutingParameterSetID frontend.Variable
	RecentPositionFloor   frontend.Variable

	Anchor                frontend.Variable
	AssetAnchor           frontend.Variable
	ComplianceAnchor      frontend.Variable
	TargetTimestamp       frontend.Variable
	ActionBalanceBlinding frontend.Variable
	IsRegulated           frontend.Variable
	RegulatedPrecision    frontend.Variable
	UnregulatedPrecision  frontend.Variable
	RoutingAsOfHeight     frontend.Variable

	Auth       TransferAuthSharedFields
	Asset      AssetTreeFields
	Sender     TransferUserCircuitFields
	Compliance TransferComplianceFields

	RequiredSpend     TransferRequiredSpendCircuitFields
	OptionalSpend     TransferOptionalSpendCircuitFields
	ReceiverOutput    TransferReceiverOutputCircuitFields
	ChangeOutput      TransferChangeOutputCircuitFields
	VolumeAccumulator TransferVolumeAccumulatorCircuitFields
}

func transferStatementFieldCount() int {
	return TransferStatementBaseFields +
		TransferStatementFieldsPerInput*TransferCircuitInputs +
		TransferStatementFieldsPerOutput*TransferCircuitOutputs
}

func NewTransferCircuit() *TransferCircuit {
	return &TransferCircuit{}
}

func (c *TransferCircuit) Define(api frontend.API) error {
	c.bindWiringTrace(api)
	c.bindTransferWitnessSemantics()
	c.traceWiring("assert.boolean", "var=is_regulated")
	api.AssertIsBoolean(c.IsRegulated)

	shared, err := c.verifySharedTransferContext(api)
	if err != nil {
		return err
	}
	statementData := c.newTransferStatementData()

	c.traceWiring("spend.begin", "spend0")
	if err := c.verifyRequiredTransferSpend(
		api,
		&shared,
		&statementData,
		&c.RequiredSpend,
	); err != nil {
		return err
	}
	c.traceWiring("spend.collect", "spend0", "amount->input_amounts", "nullifier->statement.nullifiers_and_rks", "rk_compressed->statement.nullifiers_and_rks")
	c.traceWiring("spend.begin", "spend1")
	if err := c.verifyOptionalTransferSpend(
		api,
		&shared,
		&statementData,
		&c.OptionalSpend,
	); err != nil {
		return err
	}
	c.traceWiring("spend.collect", "spend1", "amount->input_amounts", "nullifier->statement.nullifiers_and_rks", "rk_compressed->statement.nullifiers_and_rks")
	c.traceWiring("output.begin", "output0")
	if err := c.verifyTransferReceiverOutput(
		api,
		&shared,
		&statementData,
		&c.ReceiverOutput,
	); err != nil {
		return err
	}
	c.traceWiring("output.collect", "output0", "amount->output_amounts", "commitment->statement.output_commitments", "ack->receiver_ack")
	c.traceWiring("output.begin", "output1")
	if err := c.verifyTransferChangeOutput(
		api,
		&shared,
		&statementData,
		&c.ChangeOutput,
	); err != nil {
		return err
	}
	routingRolesSwapped, err := c.verifyTransferRouting(api, &shared, &statementData)
	if err != nil {
		return err
	}
	statementData.routingRolesSwapped = routingRolesSwapped
	if err := c.verifyVolumeAccumulator(api, &shared, &statementData); err != nil {
		return err
	}
	c.traceWiring("output.collect", "output1", "amount->output_amounts", "commitment->statement.output_commitments")
	c.traceWiring("compliance.begin", "tiers=sender_core,sender_ext,output_core,output_ext")
	if err := c.verifyTransferComplianceCiphertexts(api, &shared, &statementData); err != nil {
		return err
	}
	c.traceWiring("compliance.collect", "detection_ciphertext->statement", "tier_ciphertexts->statement", "metadata->statement")

	c.traceWiring("decaf.net_balance_commitment", "inputs=input_amounts", "outputs=output_amounts", "asset_id=shared.asset_id", "blinding=action_balance_blinding", "out=balance_commitment.computed")
	balanceCommitmentFq, err := c.computeTransferBalanceCommitmentFq(api, &shared, &statementData)
	if err != nil {
		return err
	}

	c.traceWiring("statement.assemble", "shape=transfer2x2", "fields=transfer_statement_fields")
	fields, err := c.buildTransferStatementFields(balanceCommitmentFq, &statementData)
	if err != nil {
		return err
	}
	for index, field := range fields {
		c.bindSemantic(fmt.Sprintf("statement.field.%03d", index), field)
	}
	c.bindSemantic("statement.fields", fields...)
	c.traceWiring("statement.hash", "family=transfer", "fields=statement_fields", "out=statement_hash")
	statementHash, err := TransferStatementHashForShape(
		api,
		TransferCircuitInputs,
		TransferCircuitOutputs,
		fields,
	)
	if err != nil {
		return err
	}
	c.bindSemantic("statement.hash", statementHash)
	c.traceWiring("assert.eq", "lhs=statement_hash", "rhs=claimed_statement_hash")
	api.AssertIsEqual(statementHash, c.ClaimedStatementHash)
	return nil
}

func statePathVariables(
	path [StateCommitmentDepth][3]frontend.Variable,
) []frontend.Variable {
	variables := make([]frontend.Variable, 0, StateCommitmentDepth*3)
	for i := range path {
		variables = append(variables, path[i][:]...)
	}
	return variables
}

func quadPathVariables(
	path [ComplianceQuadTreeDepth][3]frontend.Variable,
) []frontend.Variable {
	variables := make([]frontend.Variable, 0, ComplianceQuadTreeDepth*3)
	for i := range path {
		variables = append(variables, path[i][:]...)
	}
	return variables
}

func (c *TransferCircuit) bindTransferComplianceMetadata(
	name string,
	metadata TransferComplianceMetadataFields,
) {
	c.bindSemantic(
		name,
		metadata.RingIDHash,
		metadata.PolicyIDHash,
		metadata.ResourceHash,
		metadata.PermissionHash,
		metadata.TargetTimestamp,
		metadata.SenderCoreSalt,
		metadata.SenderExtSalt,
		metadata.OutputCoreSalt,
		metadata.OutputExtSalt,
	)
}

func (c *TransferCircuit) bindTransferComplianceTier(
	name string,
	epk Point2D,
	c2 frontend.Variable,
	keyConfirmation frontend.Variable,
	ciphertext []frontend.Variable,
) {
	c.bindSemantic(name+".epk", epk.X, epk.Y)
	c.bindSemantic(name+".c2", c2)
	if keyConfirmation != nil {
		c.bindSemantic(name+".key_confirmation", keyConfirmation)
	}
	c.bindSemantic(name+".ciphertext", ciphertext...)
}

func (c *TransferCircuit) bindTransferWitnessSemantics() {
	c.bindSemantic("claimed.statement_hash", c.ClaimedStatementHash)
	c.bindSemantic("anchor", c.Anchor)
	c.bindSemantic("asset_anchor", c.AssetAnchor)
	c.bindSemantic("compliance_anchor", c.ComplianceAnchor)
	c.bindSemantic("target_timestamp", c.TargetTimestamp)
	c.bindSemantic("action.balance_blinding", c.ActionBalanceBlinding)
	c.bindSemantic("is_regulated", c.IsRegulated)
	c.bindSemantic("routing.tags", c.RoutingTags[:]...)
	c.bindSemantic("routing.parameter_set_id", c.RoutingParameterSetID)
	c.bindSemantic("routing.regulated_precision", c.RegulatedPrecision)
	c.bindSemantic("routing.unregulated_precision", c.UnregulatedPrecision)
	c.bindSemantic("routing.as_of_height", c.RoutingAsOfHeight)
	c.bindSemantic("recent_position_floor", c.RecentPositionFloor)
	c.bindSemantic("auth.ak", c.Auth.AK.X, c.Auth.AK.Y)
	c.bindSemantic("auth.nk", c.Auth.NK)
	c.bindSemantic("auth.ivk_reduced", c.Auth.IVKReduced)
	c.bindSemantic("auth.ivk_quotient_a", c.Auth.IVKQuotientA)

	c.bindSemantic("asset.leaf.value", c.Asset.Leaf.Value)
	c.bindSemantic("asset.leaf.next_index", c.Asset.Leaf.NextIndex)
	c.bindSemantic("asset.leaf.next_value", c.Asset.Leaf.NextValue)
	c.bindSemantic("asset.leaf.dk_pub", c.Asset.Leaf.DKPub.X, c.Asset.Leaf.DKPub.Y)
	c.bindSemantic("asset.leaf.daily_volume_limit", c.Asset.Leaf.DailyVolumeLimit)
	c.bindSemantic("asset.leaf.route_policy_hash", c.Asset.Leaf.RoutePolicyHash)
	c.bindSemantic("asset.leaf.ring_pk", c.Asset.Leaf.RingPK.X, c.Asset.Leaf.RingPK.Y)
	c.bindSemantic("asset.leaf.ring_id_hash", c.Asset.Leaf.RingIDHash)
	c.bindSemantic("asset.leaf.policy_id_hash", c.Asset.Leaf.PolicyIDHash)
	c.bindSemantic("asset.leaf.permission_hash", c.Asset.Leaf.PermissionHash)
	c.bindSemantic("asset.leaf.resource_hash", c.Asset.Leaf.ResourceHash)
	c.bindSemantic("asset.path", quadPathVariables(c.Asset.Path)...)
	c.bindSemantic("asset.position", c.Asset.Position)

	c.bindSemantic("sender.div_gen", c.Sender.DivGen.X, c.Sender.DivGen.Y)
	c.bindSemantic(
		"sender.transmission",
		c.Sender.Transmission.X,
		c.Sender.Transmission.Y,
	)
	c.bindSemantic("sender.capk", c.Sender.Capk.X, c.Sender.Capk.Y)
	c.bindSemantic("sender.rnk_commitment", c.Sender.RnkCommitment)
	c.bindSemantic("sender.status", c.Sender.Status)
	c.bindSemantic("sender.path", quadPathVariables(c.Sender.Path)...)
	c.bindSemantic("sender.position", c.Sender.Position)

	c.bindSemantic("spend0.nullifier.claimed", c.RequiredSpend.Nullifier)
	c.bindSemantic("spend0.rk.claimed", c.RequiredSpend.RK.X, c.RequiredSpend.RK.Y)
	c.bindSemantic("spend0.note.blinding", c.RequiredSpend.Note.Blinding)
	c.bindSemantic("spend0.note.amount", c.RequiredSpend.Note.Amount)
	c.bindSemantic("spend0.note.asset_id", c.RequiredSpend.Note.AssetID)
	c.bindSemantic("spend0.state_proof.position", c.RequiredSpend.StateProof.Position)
	c.bindSemantic(
		"spend0.state_proof.path",
		statePathVariables(c.RequiredSpend.StateProof.Path)...,
	)
	c.bindSemantic("spend0.auth_randomizer", c.RequiredSpend.AuthRandomizer)
	c.bindSemantic("spend0.history_required", c.RequiredSpend.HistoryRequired)

	c.bindSemantic("spend1.nullifier.claimed", c.OptionalSpend.Nullifier)
	c.bindSemantic("spend1.rk.claimed", c.OptionalSpend.RK.X, c.OptionalSpend.RK.Y)
	c.bindSemantic("spend1.note.blinding", c.OptionalSpend.Note.Blinding)
	c.bindSemantic("spend1.note.amount", c.OptionalSpend.Note.Amount)
	c.bindSemantic("spend1.state_proof.position", c.OptionalSpend.StateProof.Position)
	c.bindSemantic(
		"spend1.state_proof.path",
		statePathVariables(c.OptionalSpend.StateProof.Path)...,
	)
	c.bindSemantic("spend1.auth_randomizer", c.OptionalSpend.AuthRandomizer)
	c.bindSemantic("spend1.history_required", c.OptionalSpend.HistoryRequired)
	c.bindSemantic("spend1.is_dummy", c.OptionalSpend.IsDummy)
	c.bindSemantic(
		"spend1.dummy_nullifier_seed",
		c.OptionalSpend.DummyNullifierSeed,
	)

	c.bindSemantic("output0.note_commitment.claimed", c.ReceiverOutput.NoteCommitment)
	c.bindSemantic("output0.note.blinding", c.ReceiverOutput.Note.Blinding)
	c.bindSemantic("output0.note.amount", c.ReceiverOutput.Note.Amount)
	c.bindSemantic(
		"output0.recipient.div_gen",
		c.ReceiverOutput.Recipient.DivGen.X,
		c.ReceiverOutput.Recipient.DivGen.Y,
	)
	c.bindSemantic(
		"output0.recipient.transmission",
		c.ReceiverOutput.Recipient.Transmission.X,
		c.ReceiverOutput.Recipient.Transmission.Y,
	)
	c.bindSemantic("output0.recipient.capk", c.ReceiverOutput.Recipient.Capk.X, c.ReceiverOutput.Recipient.Capk.Y)
	c.bindSemantic("output0.recipient.rnk_commitment", c.ReceiverOutput.Recipient.RnkCommitment)
	c.bindSemantic("output0.recipient.status", c.ReceiverOutput.Recipient.Status)
	c.bindSemantic(
		"output0.recipient.path",
		quadPathVariables(c.ReceiverOutput.Recipient.Path)...,
	)
	c.bindSemantic("output0.recipient.position", c.ReceiverOutput.Recipient.Position)

	c.bindSemantic("output1.note_commitment.claimed", c.ChangeOutput.NoteCommitment)
	c.bindSemantic("output1.note.blinding", c.ChangeOutput.Note.Blinding)
	c.bindSemantic("output1.note.amount", c.ChangeOutput.Note.Amount)

	c.bindSemantic("compliance.transfer_nonce_root", c.Compliance.TransferNonceRoot)
	c.bindSemantic(
		"compliance.detection_ciphertext",
		c.Compliance.DetectionCiphertext[:]...,
	)
	c.bindTransferComplianceMetadata(
		"compliance.metadata",
		c.Compliance.Metadata,
	)
	c.bindSemantic("compliance.sender_r_core", c.Compliance.SenderRCore)
	c.bindSemantic("compliance.sender_r_ext", c.Compliance.SenderRExt)
	c.bindSemantic("compliance.output_r_core", c.Compliance.OutputRCore)
	c.bindSemantic("compliance.output_r_ext", c.Compliance.OutputRExt)
	c.bindTransferComplianceTier(
		"compliance.sender_core",
		c.Compliance.SenderCore.Epk,
		c.Compliance.SenderCore.C2,
		c.Compliance.SenderCore.KeyConfirmation,
		c.Compliance.SenderCore.Ciphertext[:],
	)
	c.bindTransferComplianceTier(
		"compliance.sender_ext",
		c.Compliance.SenderExt.Epk,
		c.Compliance.SenderExt.C2,
		nil,
		c.Compliance.SenderExt.Ciphertext[:],
	)
	c.bindTransferComplianceTier(
		"compliance.output_core",
		c.Compliance.OutputCore.Epk,
		c.Compliance.OutputCore.C2,
		c.Compliance.OutputCore.KeyConfirmation,
		c.Compliance.OutputCore.Ciphertext[:],
	)
	c.bindTransferComplianceTier(
		"compliance.output_ext",
		c.Compliance.OutputExt.Epk,
		c.Compliance.OutputExt.C2,
		nil,
		c.Compliance.OutputExt.Ciphertext[:],
	)
}

type transferSharedContext struct {
	ak                      gnarkte.Point
	indexedLeaf             IndexedLeafInputs
	effectiveDKPub          gnarkte.Point
	effectiveRingPK         gnarkte.Point
	effectiveRingIDHash     frontend.Variable
	effectivePolicyHash     frontend.Variable
	effectiveResourceHash   frontend.Variable
	effectivePermissionHash frontend.Variable
	senderDivGen            gnarkte.Point
	senderTransmission      gnarkte.Point
	senderDivGenFq          frontend.Variable
	senderTransmissionFq    frontend.Variable
	senderAck               gnarkte.Point
	effectiveNK             frontend.Variable
	sharedAssetID           frontend.Variable
}

type transferStatementData struct {
	inputAmounts           []frontend.Variable
	outputAmounts          []frontend.Variable
	outputCommitments      []frontend.Variable
	nullifiersAndRKs       []frontend.Variable
	receiverAmount         frontend.Variable
	receiverDivGenFq       frontend.Variable
	receiverTransmissionFq frontend.Variable
	receiverAck            gnarkte.Point
	routingRolesSwapped    frontend.Variable
	senderCoreEPKFq        frontend.Variable
	senderExtEPKFq         frontend.Variable
	outputCoreEPKFq        frontend.Variable
	outputExtEPKFq         frontend.Variable
	isFlagged              frontend.Variable
}

// computeTransferAmountBalancePoint evaluates
// [(input0+input1)-(output0+output1)] valueGenerator. Each amount retains its
// independent 128-bit range check; two 129-bit ladders suffice because each
// side is the sum of exactly two uint128 values.
func computeTransferAmountBalancePoint(
	api frontend.API,
	curve gnarkte.Curve,
	valueGenerator gnarkte.Point,
	inputAmounts []frontend.Variable,
	outputAmounts []frontend.Variable,
) (gnarkte.Point, error) {
	if len(inputAmounts) != TransferCircuitInputs {
		return gnarkte.Point{}, fmt.Errorf(
			"transfer balance requires %d inputs, got %d",
			TransferCircuitInputs,
			len(inputAmounts),
		)
	}
	if len(outputAmounts) != TransferCircuitOutputs {
		return gnarkte.Point{}, fmt.Errorf(
			"transfer balance requires %d outputs, got %d",
			TransferCircuitOutputs,
			len(outputAmounts),
		)
	}

	sumAmounts := func(amounts []frontend.Variable) frontend.Variable {
		sum := frontend.Variable(0)
		for _, amount := range amounts {
			api.ToBinary(amount, transferAmountBits)
			sum = api.Add(sum, amount)
		}
		return sum
	}
	inputSum := sumAmounts(inputAmounts)
	outputSum := sumAmounts(outputAmounts)
	inputPoint := ScalarMulWindow2LEBits(
		api,
		curve,
		valueGenerator,
		api.ToBinary(inputSum, transferAmountSumBits),
	)
	outputPoint := ScalarMulWindow2LEBits(
		api,
		curve,
		valueGenerator,
		api.ToBinary(outputSum, transferAmountSumBits),
	)
	return curve.Add(inputPoint, curve.Neg(outputPoint)), nil
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

	valuePoint, err := computeTransferAmountBalancePoint(
		api,
		curve,
		valueGenerator,
		inputAmounts,
		outputAmounts,
	)
	if err != nil {
		return gnarkte.Point{}, err
	}
	blindingPoint := ScalarMulLE(
		api,
		curve,
		valueBlindingGenerator,
		blinding,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	)
	return curve.Add(valuePoint, blindingPoint), nil
}

// computeConservationNetBalanceCommitment is NB-1: the net-balance
// commitment for shapes that are conservation-exact by construction
// (NoteReshape families and shielded withdrawal, whose outputs include the
// public outbound amount). Sigma(inputAmounts) = Sigma(outputAmounts) is
// asserted directly as one linear row instead of building the balance from
// per-amount value-generator ladders (computeTransferNetBalanceCommitment),
// and the commitment collapses to the blinding ladder alone - the correct
// value when conservation holds, since the value terms cancel exactly.
//
// The per-amount api.ToBinary(amount, 128) calls are the ZK-ASSUME-AMOUNT-RANGE
// enforcement carried over from the deleted ladders (load-bearing: without it
// amounts could overflow the scalar field and falsify the Sigma(in)=Sigma(out)
// assert); pinned by TestAmountRangeBoundIs128Bits and the parity test below.
// Transfer keeps computeTransferNetBalanceCommitment: its balance commitment
// represents the action's net value instead of an internally zero balance.
func computeConservationNetBalanceCommitment(
	api frontend.API,
	inputAmounts []frontend.Variable,
	outputAmounts []frontend.Variable,
	blinding frontend.Variable,
) (gnarkte.Point, error) {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	valueBlindingGenerator := gnarkte.Point{
		X: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorX),
		Y: MustBigInt(vectors.Decaf377CompanionCurve.ValueBlindingGeneratorY),
	}

	sumIn := frontend.Variable(0)
	for _, amount := range inputAmounts {
		api.ToBinary(amount, 128)
		sumIn = api.Add(sumIn, amount)
	}
	sumOut := frontend.Variable(0)
	for _, amount := range outputAmounts {
		api.ToBinary(amount, 128)
		sumOut = api.Add(sumOut, amount)
	}
	api.AssertIsEqual(sumIn, sumOut)

	return ScalarMulLE(
		api,
		curve,
		valueBlindingGenerator,
		blinding,
		MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen(),
	), nil
}

func (c *TransferCircuit) verifySharedTransferContext(api frontend.API) (transferSharedContext, error) {
	c.traceWiring(
		"shared.bind",
		"shared.ak=auth.ak",
		"shared.asset_id=spend0.note.asset_id",
		"sender.div_gen=sender.div_gen",
		"sender.transmission=sender.transmission",
	)
	shared := transferSharedContext{
		ak: gnarkte.Point{X: c.Auth.AK.X, Y: c.Auth.AK.Y},
		indexedLeaf: IndexedLeafInputs{
			Value:            c.Asset.Leaf.Value,
			NextIndex:        c.Asset.Leaf.NextIndex,
			NextValue:        c.Asset.Leaf.NextValue,
			DKPub:            gnarkte.Point{X: c.Asset.Leaf.DKPub.X, Y: c.Asset.Leaf.DKPub.Y},
			DailyVolumeLimit: c.Asset.Leaf.DailyVolumeLimit,
			RoutePolicyHash:  c.Asset.Leaf.RoutePolicyHash,
			RingPK:           gnarkte.Point{X: c.Asset.Leaf.RingPK.X, Y: c.Asset.Leaf.RingPK.Y},
			RingIDHash:       c.Asset.Leaf.RingIDHash,
			PolicyIDHash:     c.Asset.Leaf.PolicyIDHash,
			PermissionHash:   c.Asset.Leaf.PermissionHash,
			ResourceHash:     c.Asset.Leaf.ResourceHash,
		},
		senderDivGen:       gnarkte.Point{X: c.Sender.DivGen.X, Y: c.Sender.DivGen.Y},
		senderTransmission: gnarkte.Point{X: c.Sender.Transmission.X, Y: c.Sender.Transmission.Y},
		sharedAssetID:      c.RequiredSpend.Note.AssetID,
	}
	c.bindSemantic("shared.asset_id", shared.sharedAssetID)
	c.traceWiring(
		"assert.decaf_non_identity",
		"point=auth.ak",
		"coordinate=x",
	)
	AssertDecafNonIdentity(api, shared.ak)
	c.traceWiring(
		"assert.decaf_non_identity",
		"point=sender.div_gen",
		"coordinate=x",
	)
	AssertDecafNonIdentity(api, shared.senderDivGen)
	c.traceWiring("assert.ne", "lhs=shared.asset_id", "rhs=0")
	api.AssertIsDifferent(shared.sharedAssetID, 0)

	unregulatedRingPK, unregulatedDKPub, err := UnregulatedComplianceKeys()
	if err != nil {
		return transferSharedContext{}, err
	}
	c.traceWiring("select.point", "cond=is_regulated", "if_true=asset.leaf.ring_pk", "if_false=unregulated.ring_pk", "out=effective.ring_pk")
	shared.effectiveRingPK = SelectPoint(api, c.IsRegulated, shared.indexedLeaf.RingPK, unregulatedRingPK)
	c.bindSemantic(
		"effective.ring_pk",
		shared.effectiveRingPK.X,
		shared.effectiveRingPK.Y,
	)
	c.traceWiring("select.point", "cond=is_regulated", "if_true=asset.leaf.dk_pub", "if_false=unregulated.dk_pub", "out=effective.dk_pub")
	shared.effectiveDKPub = SelectPoint(api, c.IsRegulated, shared.indexedLeaf.DKPub, unregulatedDKPub)
	c.bindSemantic(
		"effective.dk_pub",
		shared.effectiveDKPub.X,
		shared.effectiveDKPub.Y,
	)
	emptyPolicyHash := MustBigInt(transferUnregulatedPolicyHashDecimal)
	selectPolicyHash := func(name string, regulated frontend.Variable) frontend.Variable {
		c.traceWiring(
			"select.field",
			"cond=is_regulated",
			"if_true=asset.leaf."+name,
			"if_false=unregulated.policy_hash",
			"out=effective."+name,
		)
		effective := api.Select(c.IsRegulated, regulated, emptyPolicyHash)
		c.bindSemantic("effective."+name, effective)
		return effective
	}
	shared.effectiveRingIDHash = selectPolicyHash("ring_id_hash", shared.indexedLeaf.RingIDHash)
	shared.effectivePolicyHash = selectPolicyHash("policy_id_hash", shared.indexedLeaf.PolicyIDHash)
	shared.effectiveResourceHash = selectPolicyHash("resource_hash", shared.indexedLeaf.ResourceHash)
	shared.effectivePermissionHash = selectPolicyHash("permission_hash", shared.indexedLeaf.PermissionHash)

	c.traceWiring("decaf.compress_to_field", "in=sender.div_gen", "out=sender.div_gen_fq")
	shared.senderDivGenFq, err = decafgnark.CompressToField(api, shared.senderDivGen)
	if err != nil {
		return transferSharedContext{}, err
	}
	c.bindSemantic("sender.div_gen_fq", shared.senderDivGenFq)
	c.traceWiring("decaf.compress_to_field", "in=sender.transmission", "out=sender.transmission_fq")
	shared.senderTransmissionFq, err = decafgnark.CompressToField(api, shared.senderTransmission)
	if err != nil {
		return transferSharedContext{}, err
	}
	c.bindSemantic("sender.transmission_fq", shared.senderTransmissionFq)
	c.traceWiring("assert.ne", "lhs=auth.ivk_reduced", "rhs=0")
	AssertIncomingViewingKeyNonzero(api, c.Auth.IVKReduced)
	c.traceWiring(
		"decaf.diversified_transmission_key",
		"nk=auth.nk",
		"ak=shared.ak",
		"div_gen=sender.div_gen",
		"ivk_reduced=auth.ivk_reduced",
		"ivk_quotient_a=auth.ivk_quotient_a",
		"out=sender.transmission.computed",
	)
	computedSenderTransmission, ivkBits, err := diversifiedTransmissionKeyAndBitsAfterIvkNonzero(
		api,
		c.Auth.NK,
		shared.ak,
		shared.senderDivGen,
		c.Auth.IVKReduced,
		c.Auth.IVKQuotientA,
	)
	if err != nil {
		return transferSharedContext{}, err
	}
	c.bindSemantic(
		"sender.transmission.computed",
		computedSenderTransmission.X,
		computedSenderTransmission.Y,
	)
	c.traceWiring(
		"decaf.assert_equivalent",
		"lhs=sender.transmission.computed",
		"rhs=sender.transmission",
	)
	decafgnark.AssertEquivalent(api, computedSenderTransmission, shared.senderTransmission)
	c.traceWiring(
		"assert.decaf_non_identity",
		"point=sender.transmission",
		"coordinate=x",
	)
	AssertDecafNonIdentity(api, shared.senderTransmission)

	if err := c.verifyTransferAssetRegistry(api, &shared); err != nil {
		return transferSharedContext{}, err
	}

	c.traceWiring("gadget.compliance_leaf", "div_gen_fq=sender.div_gen_fq", "transmission_fq=sender.transmission_fq", "asset_id=shared.asset_id", "capk=sender.capk", "rnk_dh_pk=sender.rnk_dh_pk", "rnk_commitment=sender.rnk_commitment", "status=sender.status", "out=sender.leaf_commitment")
	senderLeafCommitment, err := ComplianceLeafCommitmentFromCompressed(
		api,
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
		shared.sharedAssetID,
		gnarkte.Point{X: c.Sender.Capk.X, Y: c.Sender.Capk.Y},
		gnarkte.Point{X: c.Sender.RnkDhPk.X, Y: c.Sender.RnkDhPk.Y},
		c.Sender.RnkCommitment,
		c.Sender.Status,
	)
	if err != nil {
		return transferSharedContext{}, err
	}
	c.bindSemantic("sender.leaf_commitment", senderLeafCommitment)
	derivedRNK, err := RegulatedNullifierKey(
		api,
		ivkBits,
		gnarkte.Point{X: c.Sender.RnkDhPk.X, Y: c.Sender.RnkDhPk.Y},
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
		shared.sharedAssetID,
		shared.effectiveRingPK,
	)
	if err != nil {
		return transferSharedContext{}, err
	}
	shared.effectiveNK = api.Select(c.IsRegulated, derivedRNK, c.Auth.NK)
	rnkCommitment, err := ComplianceNullifierKeyCommitment(api, derivedRNK)
	if err != nil {
		return transferSharedContext{}, err
	}
	AssertEqualIf(api, rnkCommitment, c.Sender.RnkCommitment, c.IsRegulated)
	c.traceWiring("gadget.compliance_path", "leaf=sender.leaf_commitment", "path=sender.path", "position=sender.position", "out=sender.compliance_root")
	senderComplianceRoot, err := VerifyQuadPath(api, senderLeafCommitment, c.Sender.Path, c.Sender.Position)
	if err != nil {
		return transferSharedContext{}, err
	}
	c.bindSemantic("sender.compliance_root", senderComplianceRoot)
	c.traceWiring("assert.eq_if", "lhs=sender.compliance_root", "rhs=compliance_anchor", "cond=is_regulated")
	AssertEqualIf(api, senderComplianceRoot, c.ComplianceAnchor, c.IsRegulated)
	c.traceWiring("gadget.active_lifecycle", "lifecycle=sender.status", "cond=is_regulated")
	AssertActiveComplianceLifecycle(api, c.Sender.Status, c.IsRegulated)

	c.traceWiring("bind.capk", "capk=sender.capk", "out=sender.ack")
	shared.senderAck = gnarkte.Point{X: c.Sender.Capk.X, Y: c.Sender.Capk.Y}
	c.bindSemantic("sender.ack", shared.senderAck.X, shared.senderAck.Y)

	return shared, nil
}

func (c *TransferCircuit) verifyTransferAssetRegistry(
	api frontend.API,
	shared *transferSharedContext,
) error {
	vectors, err := LoadPrototypeVectors()
	if err != nil {
		return err
	}

	// Keep the transcript at primitive boundaries. VerifyAssetRegistryIMT is a
	// useful runtime helper, but treating its heterogeneous hash/path/gap rows
	// as one gadget would let one broad label stand in for several relations.
	c.traceWiring("decaf.compress_to_field", "in=asset.leaf.dk_pub", "out=asset.leaf.dk_pub_fq")
	dkPubFq, err := decafgnark.CompressToField(api, shared.indexedLeaf.DKPub)
	if err != nil {
		return err
	}
	c.bindSemantic("asset.leaf.dk_pub_fq", dkPubFq)
	c.traceWiring(
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
			shared.indexedLeaf.DailyVolumeLimit,
			shared.indexedLeaf.RoutePolicyHash,
		},
	)
	if err != nil {
		return err
	}
	c.bindSemantic("asset.leaf.params_hash", paramsHash)

	c.traceWiring("decaf.compress_to_field", "in=asset.leaf.ring_pk", "out=asset.leaf.ring_pk_fq")
	ringPKFq, err := decafgnark.CompressToField(api, shared.indexedLeaf.RingPK)
	if err != nil {
		return err
	}
	c.bindSemantic("asset.leaf.ring_pk_fq", ringPKFq)
	c.traceWiring(
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
			shared.indexedLeaf.RingIDHash,
			shared.indexedLeaf.PolicyIDHash,
			shared.indexedLeaf.PermissionHash,
			shared.indexedLeaf.ResourceHash,
		},
	)
	if err != nil {
		return err
	}
	c.bindSemantic("asset.leaf.ring_hash", ringHash)

	c.traceWiring(
		"gadget.asset_registry_leaf_hash",
		"value=asset.leaf.value",
		"next_index=asset.leaf.next_index",
		"next_value=asset.leaf.next_value",
		"params_hash=asset.leaf.params_hash",
		"ring_hash=asset.leaf.ring_hash",
		"out=asset.leaf.commitment",
	)
	assetLeafCommitment, err := Poseidon377Hash5(
		api,
		MustBigInt(vectors.Poseidon377.IMTLeafDomain),
		[5]frontend.Variable{
			shared.indexedLeaf.Value,
			shared.indexedLeaf.NextIndex,
			shared.indexedLeaf.NextValue,
			paramsHash,
			ringHash,
		},
	)
	if err != nil {
		return err
	}
	c.bindSemantic("asset.leaf.commitment", assetLeafCommitment)

	c.traceWiring(
		"gadget.asset_registry_path",
		"leaf=asset.leaf.commitment",
		"path=asset.path",
		"position=asset.position",
		"out=asset.root.computed",
	)
	assetRoot, err := VerifyQuadPath(
		api,
		assetLeafCommitment,
		c.Asset.Path,
		c.Asset.Position,
	)
	if err != nil {
		return err
	}
	c.bindSemantic("asset.root.computed", assetRoot)
	c.traceWiring("assert.eq", "lhs=asset.root.computed", "rhs=asset_anchor")
	api.AssertIsEqual(assetRoot, c.AssetAnchor)

	c.traceWiring(
		"gadget.asset_registry_gap",
		"asset_id=shared.asset_id",
		"is_regulated=is_regulated",
		"value=asset.leaf.value",
		"next_value=asset.leaf.next_value",
		"out=asset.gap_valid",
	)
	validProof := AssetRegistryGap(
		api,
		shared.sharedAssetID,
		c.IsRegulated,
		shared.indexedLeaf.Value,
		shared.indexedLeaf.NextValue,
	)
	c.bindSemantic("asset.gap_valid", validProof)
	c.traceWiring("assert.eq", "lhs=asset.gap_valid", "rhs=1")
	api.AssertIsEqual(validProof, 1)
	return nil
}

func (c *TransferCircuit) newTransferStatementData() transferStatementData {
	return transferStatementData{
		inputAmounts:           make([]frontend.Variable, 0, TransferCircuitInputs),
		outputAmounts:          make([]frontend.Variable, 0, TransferCircuitOutputs),
		outputCommitments:      make([]frontend.Variable, 0, 2*TransferCircuitOutputs),
		nullifiersAndRKs:       make([]frontend.Variable, 0, 2*TransferCircuitInputs),
		receiverAmount:         0,
		receiverDivGenFq:       0,
		receiverTransmissionFq: 0,
		receiverAck:            gnarkte.Point{X: 0, Y: 0},
		routingRolesSwapped:    0,
		senderCoreEPKFq:        0,
		senderExtEPKFq:         0,
		outputCoreEPKFq:        0,
		outputExtEPKFq:         0,
		isFlagged:              0,
	}
}

func volumeAccumulatorDomain(label string) *big.Int {
	sum := blake2b.Sum512([]byte(label))
	return LittleEndianBytesToBigInt(sum[:])
}

func (c *TransferCircuit) verifyVolumeAccumulator(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
) error {
	receiverSameDivGen := api.IsZero(api.Sub(shared.senderDivGenFq, statementData.receiverDivGenFq))
	receiverSameTransmission := api.IsZero(api.Sub(shared.senderTransmissionFq, statementData.receiverTransmissionFq))
	isSelf := api.Mul(receiverSameDivGen, receiverSameTransmission)
	isExternal := api.Sub(1, isSelf)
	isFee := api.Sub(c.VolumeAccumulator.ProofContext, 1)
	// Fee funding may only reshape value back to the sender. Transaction location
	// alone must not exempt an arbitrary external transfer from compliance.
	api.AssertIsEqual(api.Mul(isFee, isExternal), 0)
	isFlagged, err := verifyVolumeAccumulatorTransition(
		api,
		&c.VolumeAccumulator,
		c.TargetTimestamp,
		api.Mul(c.IsRegulated, isExternal),
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
		shared.sharedAssetID,
		statementData.receiverAmount,
		shared.indexedLeaf.DailyVolumeLimit,
		c.Anchor,
		c.Auth.NK,
		c.Compliance.TransferNonceRoot,
	)
	if err != nil {
		return err
	}
	statementData.isFlagged = isFlagged
	return nil
}

func verifyVolumeAccumulatorTransition(
	api frontend.API,
	v *TransferVolumeAccumulatorCircuitFields,
	targetTimestamp frontend.Variable,
	eligibleOrdinary frontend.Variable,
	senderDivGenFq frontend.Variable,
	senderTransmissionFq frontend.Variable,
	assetID frontend.Variable,
	outboundAmount frontend.Variable,
	dailyVolumeLimit frontend.Variable,
	anchor frontend.Variable,
	nk frontend.Variable,
	paddingSeed frontend.Variable,
) (frontend.Variable, error) {
	api.AssertIsBoolean(v.UseReal)
	api.AssertIsBoolean(v.StartsNewDay)
	api.AssertIsBoolean(eligibleOrdinary)
	api.AssertIsEqual(api.Mul(api.Sub(v.ProofContext, 1), api.Sub(v.ProofContext, 2)), 0)
	isFee := api.Sub(v.ProofContext, 1)
	isOrdinary := api.Sub(1, isFee)
	api.AssertIsBoolean(isFee)
	api.AssertIsBoolean(isOrdinary)

	api.ToBinary(targetTimestamp, 64)
	api.ToBinary(v.TimestampDayIndex, 48)
	api.ToBinary(v.TimestampSecond, 17)
	api.AssertIsEqual(targetTimestamp, api.Add(api.Mul(v.TimestampDayIndex, 86400), v.TimestampSecond))
	api.AssertIsEqual(FieldLessThan(api, v.TimestampSecond, 86400), 1)
	selectedDayStart := api.Mul(v.TimestampDayIndex, 86400)
	api.AssertIsEqual(v.DayStart, api.Mul(isOrdinary, selectedDayStart))

	eligible := api.Mul(isOrdinary, eligibleOrdinary)
	api.AssertIsEqual(api.Mul(v.UseReal, api.Sub(1, eligible)), 0)
	isFlagged := api.Mul(eligible, api.Sub(1, v.UseReal))
	api.AssertIsBoolean(isFlagged)

	subject, err := Poseidon377Hash3(
		api,
		volumeAccumulatorDomain("shieldd.volume.subject"),
		[3]frontend.Variable{senderDivGenFq, senderTransmissionFq, assetID},
	)
	if err != nil {
		return nil, err
	}
	api.AssertIsEqual(api.Mul(v.UseReal, api.Sub(v.Subject, subject)), 0)

	api.ToBinary(v.PriorVolume, transferAmountBits)
	api.ToBinary(v.SuccessorVolume, transferAmountBits)
	api.ToBinary(dailyVolumeLimit, transferAmountBits)
	candidate := api.Add(v.PriorVolume, outboundAmount)
	api.ToBinary(candidate, transferAmountBits)
	api.AssertIsEqual(api.Mul(v.UseReal, api.Sub(v.SuccessorVolume, candidate)), 0)
	withinLimit := api.Sub(1, FieldLessThan(api, dailyVolumeLimit, candidate))
	api.AssertIsEqual(api.Mul(v.UseReal, api.Sub(1, withinLimit)), 0)
	api.AssertIsEqual(api.Mul(v.UseReal, v.StartsNewDay, v.PriorVolume), 0)

	priorCommitment, err := Poseidon377Hash4(
		api,
		volumeAccumulatorDomain("shieldd.volume.state"),
		[4]frontend.Variable{v.Subject, v.DayStart, v.PriorVolume, v.PriorBlinding},
	)
	if err != nil {
		return nil, err
	}
	continuation := api.Mul(v.UseReal, api.Sub(1, v.StartsNewDay))
	api.AssertIsEqual(api.Mul(continuation, api.Sub(v.PriorCommitment, priorCommitment)), 0)
	priorPath := make([][3]frontend.Variable, len(v.PriorStateProof.Path))
	copy(priorPath, v.PriorStateProof.Path[:])
	priorAnchor, err := VerifyStateCommitmentPath(api, priorCommitment, v.PriorStateProof.Position, priorPath)
	if err != nil {
		return nil, err
	}
	api.AssertIsEqual(api.Mul(continuation, api.Sub(priorAnchor, anchor)), 0)

	originNullifier, err := Poseidon377Hash3(
		api,
		volumeAccumulatorDomain("shieldd.volume.origin_nullifier"),
		[3]frontend.Variable{nk, v.Subject, v.DayStart},
	)
	if err != nil {
		return nil, err
	}
	continuationNullifier, err := Nullifier(api, nk, priorCommitment, v.PriorStateProof.Position)
	if err != nil {
		return nil, err
	}
	realNullifier := api.Select(v.StartsNewDay, originNullifier, continuationNullifier)
	realCommitment, err := Poseidon377Hash4(
		api,
		volumeAccumulatorDomain("shieldd.volume.state"),
		[4]frontend.Variable{v.Subject, v.DayStart, v.SuccessorVolume, v.SuccessorBlinding},
	)
	if err != nil {
		return nil, err
	}
	paddingCommitment, err := Poseidon377Hash3(
		api,
		volumeAccumulatorDomain("shieldd.volume.padding_commitment"),
		[3]frontend.Variable{nk, paddingSeed, v.DayStart},
	)
	if err != nil {
		return nil, err
	}
	paddingNullifier, err := Poseidon377Hash3(
		api,
		volumeAccumulatorDomain("shieldd.volume.padding_nullifier"),
		[3]frontend.Variable{nk, paddingSeed, v.DayStart},
	)
	if err != nil {
		return nil, err
	}
	ordinaryNullifier := api.Select(v.UseReal, realNullifier, paddingNullifier)
	ordinaryCommitment := api.Select(v.UseReal, realCommitment, paddingCommitment)
	api.AssertIsEqual(v.Nullifier, api.Mul(isOrdinary, ordinaryNullifier))
	api.AssertIsEqual(v.Commitment, api.Mul(isOrdinary, ordinaryCommitment))
	return isFlagged, nil
}

func transferSyntheticDummyNullifierDomain() *big.Int {
	sum := blake2b.Sum512([]byte("shieldd.transfer.synthetic_dummy.nullifier"))
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

func (c *TransferCircuit) verifyRequiredTransferSpend(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
	spend *TransferRequiredSpendCircuitFields,
) error {
	return c.verifyTransferSpend(
		api,
		shared,
		statementData,
		transferSpendView{
			Nullifier:      spend.Nullifier,
			RK:             spend.RK,
			Note:           spend.Note.TransferNotePayloadCircuitFields,
			StateProof:     spend.StateProof,
			AuthRandomizer: spend.AuthRandomizer,
		},
		0,
		nil,
	)
}

func (c *TransferCircuit) verifyOptionalTransferSpend(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
	optional *TransferOptionalSpendCircuitFields,
) error {
	return c.verifyTransferSpend(
		api,
		shared,
		statementData,
		transferSpendView{
			Nullifier:      optional.Nullifier,
			RK:             optional.RK,
			Note:           optional.Note,
			StateProof:     optional.StateProof,
			AuthRandomizer: optional.AuthRandomizer,
		},
		1,
		optional,
	)
}

type transferSpendView struct {
	Nullifier      frontend.Variable
	RK             Point2D
	Note           TransferNotePayloadCircuitFields
	StateProof     TransferStatePathCircuitFields
	AuthRandomizer frontend.Variable
}

func (c *TransferCircuit) verifyTransferSpend(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
	spend transferSpendView,
	index int,
	optional *TransferOptionalSpendCircuitFields,
) error {
	rkClaimed := gnarkte.Point{X: spend.RK.X, Y: spend.RK.Y}
	name := fmt.Sprintf("spend%d", index)
	var isNotDummy frontend.Variable
	if optional != nil {
		c.traceWiring("assert.boolean", "var="+name+".is_dummy")
		api.AssertIsBoolean(optional.IsDummy)
		isNotDummy = api.Sub(1, optional.IsDummy)
		c.bindSemantic(name+".is_not_dummy", isNotDummy)
	}

	c.bindSemantic(
		name+".note_commitment.inputs",
		spend.Note.Blinding,
		spend.Note.Amount,
		shared.sharedAssetID,
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
	)
	c.traceWiring("gadget.note_commitment", "blinding="+name+".note.blinding", "amount="+name+".note.amount", "asset_id=shared.asset_id", "div_gen_fq=sender.div_gen_fq", "transmission_key_s=sender.transmission_fq", "out="+name+".note.commitment.computed")
	spentCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		spend.Note.Blinding,
		spend.Note.Amount,
		shared.sharedAssetID,
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
		spend.Note.RecoveryCommitment,
	)
	if err != nil {
		return err
	}
	c.bindSemantic(name+".note.commitment.computed", spentCommitment)

	c.traceWiring("gadget.nullifier", "nk=auth.nk", "commitment="+name+".note.commitment.computed", "position="+name+".state_proof.position", "out="+name+".nullifier.real")
	realNullifier, err := Nullifier(api, shared.effectiveNK, spentCommitment, spend.StateProof.Position)
	if err != nil {
		return err
	}
	c.bindSemantic(name+".nullifier.real", realNullifier)

	statePath := make([][3]frontend.Variable, len(spend.StateProof.Path))
	copy(statePath, spend.StateProof.Path[:])
	c.traceWiring("gadget.state_commitment_path", "commitment="+name+".note.commitment.computed", "position="+name+".state_proof.position", "path="+name+".state_proof.path", "out="+name+".anchor.computed")
	anchor, err := VerifyStateCommitmentPath(api, spentCommitment, spend.StateProof.Position, statePath)
	if err != nil {
		return err
	}
	c.bindSemantic(name+".anchor.computed", anchor)
	if optional == nil {
		c.bindSemantic(name+".nullifier.selected", realNullifier)
		c.traceWiring("assert.eq", "lhs="+name+".nullifier", "rhs="+name+".nullifier.real")
		api.AssertIsEqual(spend.Nullifier, realNullifier)
		c.traceWiring("assert.eq", "lhs="+name+".anchor.computed", "rhs=anchor")
		api.AssertIsEqual(anchor, c.Anchor)
	} else {
		c.traceWiring("gadget.synthetic_dummy_nullifier", "seed="+name+".dummy_nullifier_seed", "randomizer="+name+".auth_randomizer", "slot="+name, "out="+name+".nullifier.synthetic")
		syntheticNullifier, err := syntheticDummyNullifier(
			api,
			optional.DummyNullifierSeed,
			spend.AuthRandomizer,
			index,
		)
		if err != nil {
			return err
		}
		c.bindSemantic(name+".nullifier.synthetic", syntheticNullifier)
		c.traceWiring("dummy.mux", "is_dummy="+name+".is_dummy", "real="+name+".nullifier.real", "synthetic="+name+".nullifier.synthetic", "out="+name+".nullifier.selected")
		selectedNullifier := api.Add(
			api.Mul(isNotDummy, realNullifier),
			api.Mul(optional.IsDummy, syntheticNullifier),
		)
		c.bindSemantic(name+".nullifier.selected", selectedNullifier)
		c.traceWiring("assert.eq", "lhs="+name+".nullifier.claimed", "rhs="+name+".nullifier.selected")
		api.AssertIsEqual(
			spend.Nullifier,
			selectedNullifier,
		)
		c.traceWiring("assert.eq_if", "lhs="+name+".anchor.computed", "rhs=anchor", "cond="+name+".is_not_dummy")
		AssertEqualIf(api, anchor, c.Anchor, isNotDummy)
	}

	c.traceWiring("decaf.randomized_verification_key", "ak=shared.ak", "randomizer="+name+".auth_randomizer", "out="+name+".rk.computed")
	computedRK, err := RandomizedVerificationKey(api, shared.ak, spend.AuthRandomizer)
	if err != nil {
		return err
	}
	c.bindSemantic(name+".rk.computed", computedRK.X, computedRK.Y)
	if optional == nil {
		c.traceWiring("decaf.assert_equivalent", "lhs="+name+".rk.computed", "rhs="+name+".rk.claimed")
		decafgnark.AssertEquivalent(api, computedRK, rkClaimed)
	} else {
		c.traceWiring("decaf.assert_equivalent_if", "lhs="+name+".rk.computed", "rhs="+name+".rk.claimed", "cond="+name+".is_not_dummy")
		decafgnark.AssertEquivalentIf(api, computedRK, rkClaimed, isNotDummy)
		c.traceWiring("assert.eq_if", "lhs="+name+".note.amount", "rhs=0", "cond="+name+".is_dummy")
		AssertEqualIf(api, spend.Note.Amount, 0, optional.IsDummy)
	}

	statementData.inputAmounts = append(statementData.inputAmounts, spend.Note.Amount)
	claimedHistoryRequired := c.RequiredSpend.HistoryRequired
	isDummy := frontend.Variable(0)
	isDummyLabel := "0"
	if optional != nil {
		claimedHistoryRequired = optional.HistoryRequired
		isDummy = optional.IsDummy
		isDummyLabel = name + ".is_dummy"
	}
	c.traceWiring(
		"history.classify",
		"position="+name+".state_proof.position",
		"floor=recent_position_floor",
		"is_dummy="+isDummyLabel,
		"out="+name+".history_required",
	)
	computedHistoryRequired := historyRequired(
		api,
		spend.StateProof.Position,
		c.RecentPositionFloor,
		isDummy,
	)
	api.AssertIsEqual(claimedHistoryRequired, computedHistoryRequired)
	statementData.nullifiersAndRKs = append(statementData.nullifiersAndRKs, spend.Nullifier)
	c.traceWiring("decaf.compress_to_field", "in="+name+".rk.claimed", "out="+name+".rk.compressed")
	rkFq, err := decafgnark.CompressToField(api, rkClaimed)
	if err != nil {
		return err
	}
	c.bindSemantic(name+".rk.compressed", rkFq)
	statementData.nullifiersAndRKs = append(statementData.nullifiersAndRKs, rkFq)
	statementData.nullifiersAndRKs = append(statementData.nullifiersAndRKs, claimedHistoryRequired)
	return nil
}

func (c *TransferCircuit) verifyTransferReceiverOutput(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
	output *TransferReceiverOutputCircuitFields,
) error {
	recipientDivGen := gnarkte.Point{X: output.Recipient.DivGen.X, Y: output.Recipient.DivGen.Y}
	recipientTransmission := gnarkte.Point{X: output.Recipient.Transmission.X, Y: output.Recipient.Transmission.Y}
	c.traceWiring(
		"assert.decaf_non_identity",
		"point=output0.recipient.div_gen",
		"coordinate=x",
	)
	AssertDecafNonIdentity(api, recipientDivGen)
	c.traceWiring(
		"assert.decaf_non_identity",
		"point=output0.recipient.transmission",
		"coordinate=x",
	)
	AssertDecafNonIdentity(api, recipientTransmission)
	c.traceWiring("gadget.is_zero", "in=output0.note.amount", "out=output0.is_dummy")
	receiverIsDummy := api.IsZero(output.Note.Amount)
	c.bindSemantic("output0.is_dummy", receiverIsDummy)

	c.traceWiring("decaf.compress_to_field", "in=output0.recipient.div_gen", "out=output0.recipient.div_gen_fq")
	recipientDivGenFq, err := decafgnark.CompressToField(api, recipientDivGen)
	if err != nil {
		return err
	}
	c.bindSemantic("output0.recipient.div_gen_fq", recipientDivGenFq)
	c.traceWiring("decaf.compress_to_field", "in=output0.recipient.transmission", "out=output0.recipient.transmission_fq")
	recipientTransmissionFq, err := decafgnark.CompressToField(api, recipientTransmission)
	if err != nil {
		return err
	}
	c.bindSemantic(
		"output0.recipient.transmission_fq",
		recipientTransmissionFq,
	)

	c.bindSemantic(
		"output0.note_commitment.inputs",
		output.Note.Blinding,
		output.Note.Amount,
		shared.sharedAssetID,
		recipientDivGenFq,
		recipientTransmissionFq,
	)
	c.traceWiring("gadget.note_commitment", "blinding=output0.note.blinding", "amount=output0.note.amount", "asset_id=shared.asset_id", "div_gen_fq=output0.recipient.div_gen_fq", "transmission_key_s=output0.recipient.transmission_fq", "out=output0.note.commitment.computed")
	createdCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		output.Note.Blinding,
		output.Note.Amount,
		shared.sharedAssetID,
		recipientDivGenFq,
		recipientTransmissionFq,
		output.Note.RecoveryCommitment,
	)
	if err != nil {
		return err
	}
	c.bindSemantic("output0.note.commitment.computed", createdCommitment)
	c.traceWiring("assert.eq", "lhs=output0.note.commitment.computed", "rhs=output0.note_commitment")
	api.AssertIsEqual(createdCommitment, output.NoteCommitment)

	c.traceWiring("gadget.compliance_leaf", "div_gen_fq=output0.recipient.div_gen_fq", "transmission_fq=output0.recipient.transmission_fq", "asset_id=shared.asset_id", "capk=output0.recipient.capk", "rnk_dh_pk=output0.recipient.rnk_dh_pk", "rnk_commitment=output0.recipient.rnk_commitment", "status=output0.recipient.status", "out=output0.recipient.leaf_commitment")
	recipientLeafCommitment, err := ComplianceLeafCommitmentFromCompressed(
		api,
		recipientDivGenFq,
		recipientTransmissionFq,
		shared.sharedAssetID,
		gnarkte.Point{X: output.Recipient.Capk.X, Y: output.Recipient.Capk.Y},
		gnarkte.Point{X: output.Recipient.RnkDhPk.X, Y: output.Recipient.RnkDhPk.Y},
		output.Recipient.RnkCommitment,
		output.Recipient.Status,
	)
	if err != nil {
		return err
	}
	c.bindSemantic("output0.recipient.leaf_commitment", recipientLeafCommitment)
	c.traceWiring("gadget.compliance_path", "leaf=output0.recipient.leaf_commitment", "path=output0.recipient.path", "position=output0.recipient.position", "out=output0.recipient.compliance_root")
	recipientComplianceRoot, err := VerifyQuadPath(api, recipientLeafCommitment, output.Recipient.Path, output.Recipient.Position)
	if err != nil {
		return err
	}
	c.bindSemantic(
		"output0.recipient.compliance_root",
		recipientComplianceRoot,
	)
	c.traceWiring("assert.eq_if", "lhs=output0.recipient.compliance_root", "rhs=compliance_anchor", "cond=is_regulated")
	AssertEqualIf(api, recipientComplianceRoot, c.ComplianceAnchor, c.IsRegulated)
	c.traceWiring("gadget.active_lifecycle", "lifecycle=output0.recipient.status", "cond=is_regulated")
	AssertActiveComplianceLifecycle(api, output.Recipient.Status, c.IsRegulated)
	if err := VerifyRecoveryCapsule(
		api,
		gnarkte.Point{X: output.Recipient.Capk.X, Y: output.Recipient.Capk.Y},
		output.Note.Amount,
		output.Note.Blinding,
		output.Recovery,
	); err != nil {
		return err
	}
	api.AssertIsEqual(output.Recovery.Commitment, output.Note.RecoveryCommitment)

	statementData.outputAmounts = append(statementData.outputAmounts, output.Note.Amount)
	statementData.outputCommitments = append(
		statementData.outputCommitments,
		output.NoteCommitment,
		output.Note.RecoveryCommitment,
	)
	c.traceWiring("assert.eq", "lhs=output0.is_dummy", "rhs=0")
	api.AssertIsEqual(receiverIsDummy, 0)
	c.traceWiring("bind.capk", "capk=output0.recipient.capk", "out=receiver.ack")
	recipientAck := gnarkte.Point{X: output.Recipient.Capk.X, Y: output.Recipient.Capk.Y}
	c.bindSemantic("receiver.ack", recipientAck.X, recipientAck.Y)
	statementData.receiverAmount = output.Note.Amount
	statementData.receiverDivGenFq = recipientDivGenFq
	statementData.receiverTransmissionFq = recipientTransmissionFq
	statementData.receiverAck = recipientAck
	c.bindSemantic("receiver.amount", statementData.receiverAmount)
	c.bindSemantic("receiver.div_gen_fq", statementData.receiverDivGenFq)
	c.bindSemantic(
		"receiver.transmission_fq",
		statementData.receiverTransmissionFq,
	)
	return nil
}

func (c *TransferCircuit) verifyTransferChangeOutput(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
	output *TransferChangeOutputCircuitFields,
) error {
	c.bindSemantic(
		"output1.note_commitment.inputs",
		output.Note.Blinding,
		output.Note.Amount,
		shared.sharedAssetID,
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
	)
	c.traceWiring("gadget.note_commitment", "blinding=output1.note.blinding", "amount=output1.note.amount", "asset_id=shared.asset_id", "div_gen_fq=sender.div_gen_fq", "transmission_key_s=sender.transmission_fq", "out=output1.note.commitment.computed")
	createdCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		output.Note.Blinding,
		output.Note.Amount,
		shared.sharedAssetID,
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
		output.Note.RecoveryCommitment,
	)
	if err != nil {
		return err
	}
	c.bindSemantic("output1.note.commitment.computed", createdCommitment)
	c.traceWiring("assert.eq", "lhs=output1.note.commitment.computed", "rhs=output1.note_commitment")
	api.AssertIsEqual(createdCommitment, output.NoteCommitment)
	if err := VerifyRecoveryCapsule(
		api,
		shared.senderAck,
		output.Note.Amount,
		output.Note.Blinding,
		output.Recovery,
	); err != nil {
		return err
	}
	api.AssertIsEqual(output.Recovery.Commitment, output.Note.RecoveryCommitment)
	statementData.outputAmounts = append(statementData.outputAmounts, output.Note.Amount)
	statementData.outputCommitments = append(
		statementData.outputCommitments,
		output.NoteCommitment,
		output.Note.RecoveryCommitment,
	)
	return nil
}

func (c *TransferCircuit) verifyTransferComplianceCiphertexts(
	api frontend.API,
	shared *transferSharedContext,
	statementData *transferStatementData,
) error {
	c.traceWiring("daily_volume_limit.flag", "regulated=is_regulated", "amount=receiver.amount", "daily_volume_limit=asset.leaf.daily_volume_limit", "out=is_flagged")
	isFlagged := statementData.isFlagged
	c.bindSemantic("is_flagged", isFlagged)

	senderCoreEPK := gnarkte.Point{X: c.Compliance.SenderCore.Epk.X, Y: c.Compliance.SenderCore.Epk.Y}
	senderExtEPK := gnarkte.Point{X: c.Compliance.SenderExt.Epk.X, Y: c.Compliance.SenderExt.Epk.Y}
	outputCoreEPK := gnarkte.Point{X: c.Compliance.OutputCore.Epk.X, Y: c.Compliance.OutputCore.Epk.Y}
	outputExtEPK := gnarkte.Point{X: c.Compliance.OutputExt.Epk.X, Y: c.Compliance.OutputExt.Epk.Y}

	c.traceWiring("assert.decaf_non_identity", "point=compliance.sender_core.epk", "coordinate=x")
	AssertDecafNonIdentity(api, senderCoreEPK)
	c.traceWiring("assert.decaf_non_identity", "point=compliance.sender_ext.epk", "coordinate=x")
	AssertDecafNonIdentity(api, senderExtEPK)
	c.traceWiring("assert.decaf_non_identity", "point=compliance.output_core.epk", "coordinate=x")
	AssertDecafNonIdentity(api, outputCoreEPK)
	c.traceWiring("assert.decaf_non_identity", "point=compliance.output_ext.epk", "coordinate=x")
	AssertDecafNonIdentity(api, outputExtEPK)

	c.traceWiring("decaf.compress_to_field", "in=compliance.sender_core.epk", "out=compliance.sender_core.epk_fq")
	senderCoreEPKFq, err := decafgnark.CompressToField(api, senderCoreEPK)
	if err != nil {
		return err
	}
	c.bindSemantic("compliance.sender_core.epk_fq", senderCoreEPKFq)
	c.traceWiring("decaf.compress_to_field", "in=compliance.sender_ext.epk", "out=compliance.sender_ext.epk_fq")
	senderExtEPKFq, err := decafgnark.CompressToField(api, senderExtEPK)
	if err != nil {
		return err
	}
	c.bindSemantic("compliance.sender_ext.epk_fq", senderExtEPKFq)
	c.traceWiring("decaf.compress_to_field", "in=compliance.output_core.epk", "out=compliance.output_core.epk_fq")
	outputCoreEPKFq, err := decafgnark.CompressToField(api, outputCoreEPK)
	if err != nil {
		return err
	}
	c.bindSemantic("compliance.output_core.epk_fq", outputCoreEPKFq)
	c.traceWiring("decaf.compress_to_field", "in=compliance.output_ext.epk", "out=compliance.output_ext.epk_fq")
	outputExtEPKFq, err := decafgnark.CompressToField(api, outputExtEPK)
	if err != nil {
		return err
	}
	c.bindSemantic("compliance.output_ext.epk_fq", outputExtEPKFq)
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
		c.traceWiring("gadget.transfer_salt", fmt.Sprintf("label=salt%d", i), "nonce_root=compliance.transfer_nonce_root", fmt.Sprintf("out=salt%d", i))
		salts[i], err = DeriveTransferSalt(api, c.Compliance.TransferNonceRoot, label)
		if err != nil {
			return err
		}
		c.bindSemantic(fmt.Sprintf("salt%d", i), salts[i])
	}

	c.traceWiring("decaf.shared_secret", "tier=sender_core", "esk=compliance.sender_r_core", "ack=sender.ack", "dk_pub=effective.dk_pub", "flag=is_flagged", "epk=compliance.sender_core.epk", "issuer=sender_core.shared.issuer", "user=sender_core.shared.user", "selected=sender_core.shared.selected")
	senderCoreIssuer, senderCoreUser, senderCoreSelected, err := DeriveSharedSecretsSpend(
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
	c.bindSemantic("sender_core.shared.issuer", senderCoreIssuer.X, senderCoreIssuer.Y)
	c.bindSemantic("sender_core.shared.user", senderCoreUser.X, senderCoreUser.Y)
	c.bindSemantic("sender_core.shared.selected", senderCoreSelected.X, senderCoreSelected.Y)
	c.traceWiring("decaf.shared_secret", "tier=sender_ext", "esk=compliance.sender_r_ext", "ack=sender.ack", "dk_pub=effective.dk_pub", "flag=is_flagged", "epk=compliance.sender_ext.epk", "issuer=sender_ext.shared.issuer", "user=sender_ext.shared.user", "selected=sender_ext.shared.selected")
	senderExtIssuer, senderExtUser, senderExtSelected, err := DeriveSharedSecretsSpend(
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
	c.bindSemantic("sender_ext.shared.issuer", senderExtIssuer.X, senderExtIssuer.Y)
	c.bindSemantic("sender_ext.shared.user", senderExtUser.X, senderExtUser.Y)
	c.bindSemantic("sender_ext.shared.selected", senderExtSelected.X, senderExtSelected.Y)
	c.traceWiring("decaf.shared_secret", "tier=output_core", "esk=compliance.output_r_core", "ack=receiver.ack", "dk_pub=effective.dk_pub", "flag=is_flagged", "epk=compliance.output_core.epk", "issuer=output_core.shared.issuer", "user=output_core.shared.user", "selected=output_core.shared.selected")
	outputCoreIssuer, outputCoreUser, outputCoreSelected, err := DeriveSharedSecretsSpend(
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
	c.bindSemantic("output_core.shared.issuer", outputCoreIssuer.X, outputCoreIssuer.Y)
	c.bindSemantic("output_core.shared.user", outputCoreUser.X, outputCoreUser.Y)
	c.bindSemantic("output_core.shared.selected", outputCoreSelected.X, outputCoreSelected.Y)
	c.traceWiring("decaf.shared_secret", "tier=output_ext", "esk=compliance.output_r_ext", "ack=receiver.ack", "dk_pub=effective.dk_pub", "flag=is_flagged", "epk=compliance.output_ext.epk", "issuer=output_ext.shared.issuer", "user=output_ext.shared.user", "selected=output_ext.shared.selected")
	outputExtIssuer, outputExtUser, outputExtSelected, err := DeriveSharedSecretsSpend(
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
	c.bindSemantic("output_ext.shared.issuer", outputExtIssuer.X, outputExtIssuer.Y)
	c.bindSemantic("output_ext.shared.user", outputExtUser.X, outputExtUser.Y)
	c.bindSemantic("output_ext.shared.selected", outputExtSelected.X, outputExtSelected.Y)

	c.traceWiring("gadget.poseidon_encryption.detection", "flag=is_flagged", "ss=sender_core.shared.issuer", "epk_fq=compliance.sender_core.epk_fq", "salt=salt0", "asset_id=shared.asset_id", "reserved=0", "out=compliance.detection_ciphertext")
	if err := VerifyPoseidonEncryptionTransferDetection(
		api,
		isFlagged,
		senderCoreIssuer,
		statementData.senderCoreEPKFq,
		salts[0],
		shared.sharedAssetID,
		c.Compliance.DetectionCiphertext,
	); err != nil {
		return err
	}
	c.traceWiring("gadget.poseidon_encryption.amount", "tier=sender_core", "ss=sender_core.shared.selected", "c2=compliance.sender_core.c2", "epk_fq=compliance.sender_core.epk_fq", "salt=salt1", "key_confirmation=compliance.sender_core.key_confirmation", "amount=receiver.amount", "out=compliance.sender_core.ciphertext")
	if err := VerifyPoseidonEncryptionTransferAmount(
		api,
		senderCoreSelected,
		c.Compliance.SenderCore.C2,
		statementData.senderCoreEPKFq,
		salts[1],
		c.Compliance.SenderCore.KeyConfirmation,
		statementData.receiverAmount,
		c.Compliance.SenderCore.Ciphertext,
	); err != nil {
		return err
	}
	c.traceWiring("gadget.poseidon_encryption.address", "tier=sender_ext", "ss=sender_ext.shared.selected", "c2=compliance.sender_ext.c2", "div_gen_fq=receiver.div_gen_fq", "transmission_fq=receiver.transmission_fq", "out=compliance.sender_ext.ciphertext")
	if err := VerifyPoseidonEncryptionTransferAddress(
		api,
		senderExtSelected,
		c.Compliance.SenderExt.C2,
		statementData.receiverDivGenFq,
		statementData.receiverTransmissionFq,
		c.Compliance.SenderExt.Ciphertext,
	); err != nil {
		return err
	}
	c.traceWiring("gadget.poseidon_encryption.amount", "tier=output_core", "ss=output_core.shared.selected", "c2=compliance.output_core.c2", "epk_fq=compliance.output_core.epk_fq", "salt=salt3", "key_confirmation=compliance.output_core.key_confirmation", "amount=receiver.amount", "out=compliance.output_core.ciphertext")
	if err := VerifyPoseidonEncryptionTransferAmount(
		api,
		outputCoreSelected,
		c.Compliance.OutputCore.C2,
		statementData.outputCoreEPKFq,
		salts[3],
		c.Compliance.OutputCore.KeyConfirmation,
		statementData.receiverAmount,
		c.Compliance.OutputCore.Ciphertext,
	); err != nil {
		return err
	}
	c.traceWiring("gadget.poseidon_encryption.address", "tier=output_ext", "ss=output_ext.shared.selected", "c2=compliance.output_ext.c2", "div_gen_fq=sender.div_gen_fq", "transmission_fq=sender.transmission_fq", "out=compliance.output_ext.ciphertext")
	if err := VerifyPoseidonEncryptionTransferAddress(
		api,
		outputExtSelected,
		c.Compliance.OutputExt.C2,
		shared.senderDivGenFq,
		shared.senderTransmissionFq,
		c.Compliance.OutputExt.Ciphertext,
	); err != nil {
		return err
	}

	metadata := c.Compliance.Metadata
	c.traceWiring("compliance.metadata.begin", "metadata=compliance.metadata")
	c.traceWiring("assert.eq", "lhs=compliance.metadata.ring_id_hash", "rhs=effective.ring_id_hash")
	api.AssertIsEqual(metadata.RingIDHash, shared.effectiveRingIDHash)
	c.traceWiring("assert.eq", "lhs=compliance.metadata.policy_id_hash", "rhs=effective.policy_id_hash")
	api.AssertIsEqual(metadata.PolicyIDHash, shared.effectivePolicyHash)
	c.traceWiring("assert.eq", "lhs=compliance.metadata.resource_hash", "rhs=effective.resource_hash")
	api.AssertIsEqual(metadata.ResourceHash, shared.effectiveResourceHash)
	c.traceWiring("assert.eq", "lhs=compliance.metadata.permission_hash", "rhs=effective.permission_hash")
	api.AssertIsEqual(metadata.PermissionHash, shared.effectivePermissionHash)
	c.traceWiring("assert.eq", "lhs=compliance.metadata.target_timestamp", "rhs=target_timestamp")
	api.AssertIsEqual(metadata.TargetTimestamp, c.TargetTimestamp)
	c.traceWiring("assert.eq", "lhs=compliance.metadata.sender_core_salt", "rhs=salt1")
	api.AssertIsEqual(metadata.SenderCoreSalt, salts[1])
	c.traceWiring("assert.eq", "lhs=compliance.metadata.sender_ext_salt", "rhs=salt2")
	api.AssertIsEqual(metadata.SenderExtSalt, salts[2])
	c.traceWiring("assert.eq", "lhs=compliance.metadata.output_core_salt", "rhs=salt3")
	api.AssertIsEqual(metadata.OutputCoreSalt, salts[3])
	c.traceWiring("assert.eq", "lhs=compliance.metadata.output_ext_salt", "rhs=salt4")
	api.AssertIsEqual(metadata.OutputExtSalt, salts[4])

	return nil
}

func (c *TransferCircuit) computeTransferBalanceCommitmentFq(
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
	c.bindSemantic(
		"balance.inputs",
		statementData.inputAmounts...,
	)
	c.bindSemantic(
		"balance.outputs",
		statementData.outputAmounts...,
	)
	c.bindSemantic(
		"balance_commitment.computed",
		netBalanceCommitment.X,
		netBalanceCommitment.Y,
	)

	c.traceWiring("decaf.compress_to_field", "in=balance_commitment.computed", "out=balance_commitment.fq")
	balanceCommitmentFq, err := decafgnark.CompressToField(api, netBalanceCommitment)
	if err != nil {
		return nil, err
	}
	c.bindSemantic("balance_commitment.fq", balanceCommitmentFq)
	return balanceCommitmentFq, nil
}

func (c *TransferCircuit) buildTransferStatementFields(
	balanceCommitmentFq frontend.Variable,
	statementData *transferStatementData,
) ([]frontend.Variable, error) {
	fields := make([]frontend.Variable, 0, transferStatementFieldCount())
	fields = append(fields, c.Anchor)
	fields = append(fields, statementData.outputCommitments...)
	fields = append(fields, balanceCommitmentFq)
	fields = append(fields, c.RoutingTags[:]...)
	fields = append(fields, c.RoutingParameterSetID)
	fields = append(fields, c.RecentPositionFloor)
	fields = append(fields,
		c.VolumeAccumulator.Nullifier,
		c.VolumeAccumulator.Commitment,
		c.VolumeAccumulator.DayStart,
		c.VolumeAccumulator.ProofContext,
	)
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
	appendCoreTier(statementData.senderCoreEPKFq, c.Compliance.SenderCore)
	appendExtTier(statementData.senderExtEPKFq, c.Compliance.SenderExt)
	appendCoreTier(statementData.outputCoreEPKFq, c.Compliance.OutputCore)
	appendExtTier(statementData.outputExtEPKFq, c.Compliance.OutputExt)
	fields = append(fields, c.TargetTimestamp)
	fields = append(
		fields,
		c.Compliance.SenderCore.KeyConfirmation,
		c.Compliance.OutputCore.KeyConfirmation,
		c.Compliance.Metadata.RingIDHash,
		c.Compliance.Metadata.PolicyIDHash,
		c.Compliance.Metadata.ResourceHash,
		c.Compliance.Metadata.PermissionHash,
		c.Compliance.Metadata.SenderCoreSalt,
		c.Compliance.Metadata.SenderExtSalt,
		c.Compliance.Metadata.OutputCoreSalt,
		c.Compliance.Metadata.OutputExtSalt,
	)

	expected := transferStatementFieldCount()
	if len(fields) != expected {
		return nil, fmt.Errorf("expected %d transfer statement fields, got %d", expected, len(fields))
	}
	return fields, nil
}
