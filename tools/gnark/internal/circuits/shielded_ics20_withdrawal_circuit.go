package circuits

import (
	"fmt"
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/mizufinance/penumbra/tools/gnark/internal/compliance"
	. "github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
	"golang.org/x/crypto/blake2b"
)

type ShieldedIcs20WithdrawalChangeCircuitFields struct {
	NoteCommitment frontend.Variable
	Note           NoteFields
}

type ShieldedIcs20WithdrawalCircuit struct {
	nIn int

	ClaimedStatementHash frontend.Variable `gnark:",public"`

	Anchor                 frontend.Variable
	BalanceCommitment      Point2D
	AssetAnchor            frontend.Variable
	ComplianceAnchor       frontend.Variable
	TargetTimestamp        frontend.Variable
	OutboundAssetID        frontend.Variable
	OutboundAmount         frontend.Variable
	WithdrawalEffectHashLo frontend.Variable
	WithdrawalEffectHashHi frontend.Variable
	ActionBalanceBlinding  frontend.Variable
	IsRegulated            frontend.Variable

	Auth   TransferAuthSharedFields
	Asset  AssetTreeFields
	Sender UserComplianceFields

	Spends       []TransferSpendCircuitFields
	ChangeOutput ShieldedIcs20WithdrawalChangeCircuitFields
}

func NewShieldedIcs20WithdrawalCircuit(nIn int) *ShieldedIcs20WithdrawalCircuit {
	return &ShieldedIcs20WithdrawalCircuit{
		nIn:    nIn,
		Spends: make([]TransferSpendCircuitFields, nIn),
	}
}

func (c *ShieldedIcs20WithdrawalCircuit) Define(api frontend.API) error {
	if c.nIn <= 0 {
		return fmt.Errorf("shielded ICS-20 withdrawal circuit shape must be positive, got %d", c.nIn)
	}
	if len(c.Spends) != c.nIn {
		return fmt.Errorf("shielded ICS-20 withdrawal circuit shape mismatch: expected %d spends, got %d", c.nIn, len(c.Spends))
	}

	shared, err := c.verifySharedContext(api)
	if err != nil {
		return err
	}

	inputAmounts := make([]frontend.Variable, 0, c.nIn)
	nullifiersAndRKs := make([]frontend.Variable, 0, 2*c.nIn)
	for i := range c.Spends {
		amount, nullifier, rkCompressed, err := c.verifySpend(api, &shared, &c.Spends[i], i)
		if err != nil {
			return err
		}
		inputAmounts = append(inputAmounts, amount)
		nullifiersAndRKs = append(nullifiersAndRKs, nullifier, rkCompressed)
	}

	changeAmount, changeCommitment, err := c.verifyChangeOutput(api, &shared, &c.ChangeOutput)
	if err != nil {
		return err
	}
	balanceCommitmentPoint, err := computeTransferNetBalanceCommitment(
		api,
		inputAmounts,
		[]frontend.Variable{changeAmount, c.OutboundAmount},
		shared.sharedAssetID,
		c.ActionBalanceBlinding,
	)
	if err != nil {
		return err
	}
	AssertDecafEquivalent(api, balanceCommitmentPoint, shared.claimedBalanceCommitment)
	balanceCommitmentFq, err := Decaf377CompressToField(api, balanceCommitmentPoint)
	if err != nil {
		return err
	}

	fields := make([]frontend.Variable, 0, ShieldedIcs20WithdrawalStatementFieldCount(c.nIn))
	fields = append(fields, c.Anchor, changeCommitment, balanceCommitmentFq)
	fields = append(fields, nullifiersAndRKs...)
	fields = append(
		fields,
		c.AssetAnchor,
		c.ComplianceAnchor,
		c.TargetTimestamp,
		c.OutboundAssetID,
		c.OutboundAmount,
		c.WithdrawalEffectHashLo,
		c.WithdrawalEffectHashHi,
	)

	statementHash, err := ShieldedIcs20WithdrawalStatementHashForShape(api, c.nIn, fields)
	if err != nil {
		return err
	}
	api.AssertIsEqual(statementHash, c.ClaimedStatementHash)
	return nil
}

type shieldedIcs20WithdrawalSharedContext struct {
	claimedBalanceCommitment gnarkte.Point
	ak                       gnarkte.Point
	indexedLeaf              IndexedLeafInputs
	senderDivGen             gnarkte.Point
	senderTransmission       gnarkte.Point
	sharedAssetID            frontend.Variable
}

func (c *ShieldedIcs20WithdrawalCircuit) verifySharedContext(
	api frontend.API,
) (shieldedIcs20WithdrawalSharedContext, error) {
	shared := shieldedIcs20WithdrawalSharedContext{
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

	senderDivGenFq, err := Decaf377CompressToField(api, shared.senderDivGen)
	if err != nil {
		return shieldedIcs20WithdrawalSharedContext{}, err
	}
	senderTransmissionFq, err := Decaf377CompressToField(api, shared.senderTransmission)
	if err != nil {
		return shieldedIcs20WithdrawalSharedContext{}, err
	}

	assetLeafCommitment, err := IndexedLeafCommitment(api, shared.indexedLeaf)
	if err != nil {
		return shieldedIcs20WithdrawalSharedContext{}, err
	}
	assetRoot, err := VerifyQuadPath(api, assetLeafCommitment, c.Asset.Path, c.Asset.Position)
	if err != nil {
		return shieldedIcs20WithdrawalSharedContext{}, err
	}
	AssertEqualIf(api, assetRoot, c.AssetAnchor, c.IsRegulated)

	senderLeafCommitment, err := ComplianceLeafCommitmentFromCompressed(
		api,
		senderDivGenFq,
		senderTransmissionFq,
		c.Sender.AssetID,
		c.Sender.D,
	)
	if err != nil {
		return shieldedIcs20WithdrawalSharedContext{}, err
	}
	senderComplianceRoot, err := VerifyQuadPath(api, senderLeafCommitment, c.Sender.Path, c.Sender.Position)
	if err != nil {
		return shieldedIcs20WithdrawalSharedContext{}, err
	}
	AssertEqualIf(api, senderComplianceRoot, c.ComplianceAnchor, c.IsRegulated)

	api.AssertIsEqual(shared.sharedAssetID, c.OutboundAssetID)
	api.AssertIsEqual(c.Sender.AssetID, c.OutboundAssetID)

	return shared, nil
}

func shieldedIcs20WithdrawalSyntheticDummyNullifierDomain() *big.Int {
	sum := blake2b.Sum512([]byte("penumbra.shielded_ics20_withdrawal.synthetic_dummy.nullifier"))
	return LittleEndianBytesToBigInt(sum[:])
}

func shieldedIcs20WithdrawalSyntheticDummyNullifier(
	api frontend.API,
	seed frontend.Variable,
	authRandomizer frontend.Variable,
	slotIndex int,
) (frontend.Variable, error) {
	return Poseidon377Hash3(
		api,
		shieldedIcs20WithdrawalSyntheticDummyNullifierDomain(),
		[3]frontend.Variable{seed, authRandomizer, slotIndex},
	)
}

func shieldedIcs20WithdrawalSyntheticDummyVerificationKey(
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

func (c *ShieldedIcs20WithdrawalCircuit) verifySpend(
	api frontend.API,
	shared *shieldedIcs20WithdrawalSharedContext,
	spend *TransferSpendCircuitFields,
	index int,
) (frontend.Variable, frontend.Variable, frontend.Variable, error) {
	spentDivGen := gnarkte.Point{X: spend.Note.DivGen.X, Y: spend.Note.DivGen.Y}
	spentTransmission := gnarkte.Point{X: spend.Note.Transmission.X, Y: spend.Note.Transmission.Y}
	rkClaimed := gnarkte.Point{X: spend.RK.X, Y: spend.RK.Y}
	api.AssertIsBoolean(spend.IsDummy)
	isNotDummy := api.Sub(1, spend.IsDummy)

	spentDivGenFq, err := Decaf377CompressToField(api, spentDivGen)
	if err != nil {
		return nil, nil, nil, err
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
		return nil, nil, nil, err
	}
	AssertEqualIf(api, spentCommitment, spend.StateProof.Commitment, isNotDummy)

	realNullifier, err := Nullifier(api, c.Auth.NK, spend.StateProof.Commitment, spend.StateProof.Position)
	if err != nil {
		return nil, nil, nil, err
	}
	statePath := make([][3]frontend.Variable, len(spend.StateProof.Path))
	copy(statePath, spend.StateProof.Path[:])
	anchor, err := VerifyStateCommitmentPath(api, spend.StateProof.Commitment, spend.StateProof.Position, statePath)
	if err != nil {
		return nil, nil, nil, err
	}
	syntheticNullifier, err := shieldedIcs20WithdrawalSyntheticDummyNullifier(
		api,
		spend.DummyNullifierSeed,
		spend.AuthRandomizer,
		index,
	)
	if err != nil {
		return nil, nil, nil, err
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
		return nil, nil, nil, err
	}
	dummyRK, err := shieldedIcs20WithdrawalSyntheticDummyVerificationKey(
		api,
		spend.DummySpendAuthKey,
		spend.AuthRandomizer,
	)
	if err != nil {
		return nil, nil, nil, err
	}
	AssertDecafEquivalentIf(api, computedRK, rkClaimed, isNotDummy)
	AssertDecafEquivalentIf(api, dummyRK, rkClaimed, spend.IsDummy)

	computedTransmission, err := DiversifiedTransmissionKey(
		api,
		c.Auth.NK,
		shared.ak,
		spentDivGen,
		c.Auth.IVKReduced,
		c.Auth.IVKQuotientA,
	)
	if err != nil {
		return nil, nil, nil, err
	}
	AssertDecafEquivalentIf(api, computedTransmission, spentTransmission, isNotDummy)
	AssertEqualIf(api, spend.Note.Amount, 0, spend.IsDummy)

	api.AssertIsEqual(spend.Note.AssetID, shared.sharedAssetID)
	api.AssertIsEqual(c.Sender.AssetID, spend.Note.AssetID)
	AssertDecafEquivalentIf(api, shared.senderDivGen, spentDivGen, 1)
	AssertDecafEquivalentIf(api, shared.senderTransmission, spentTransmission, 1)

	rkFq, err := Decaf377CompressToField(api, rkClaimed)
	if err != nil {
		return nil, nil, nil, err
	}
	return spend.Note.Amount, spend.Nullifier, rkFq, nil
}

func (c *ShieldedIcs20WithdrawalCircuit) verifyChangeOutput(
	api frontend.API,
	shared *shieldedIcs20WithdrawalSharedContext,
	output *ShieldedIcs20WithdrawalChangeCircuitFields,
) (frontend.Variable, frontend.Variable, error) {
	createdDivGen := gnarkte.Point{X: output.Note.DivGen.X, Y: output.Note.DivGen.Y}
	createdTransmission := gnarkte.Point{X: output.Note.Transmission.X, Y: output.Note.Transmission.Y}

	createdDivGenFq, err := Decaf377CompressToField(api, createdDivGen)
	if err != nil {
		return nil, nil, err
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
		return nil, nil, err
	}
	api.AssertIsEqual(createdCommitment, output.NoteCommitment)

	computedTransmission, err := DiversifiedTransmissionKey(
		api,
		c.Auth.NK,
		shared.ak,
		createdDivGen,
		c.Auth.IVKReduced,
		c.Auth.IVKQuotientA,
	)
	if err != nil {
		return nil, nil, err
	}
	AssertDecafEquivalentIf(api, computedTransmission, createdTransmission, 1)
	api.AssertIsEqual(output.Note.AssetID, shared.sharedAssetID)
	AssertDecafEquivalentIf(api, createdDivGen, shared.senderDivGen, 1)
	AssertDecafEquivalentIf(api, createdTransmission, shared.senderTransmission, 1)

	return output.Note.Amount, output.NoteCommitment, nil
}
