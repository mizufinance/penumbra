package circuits

import (
	"fmt"
	decafgnark "github.com/mizufinance/decaf377-go/gnark"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	. "github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
	"golang.org/x/crypto/blake2b"
)

type NoteReshapeOutputCircuitFields struct {
	NoteCommitment frontend.Variable
	Note           NoteReshapeNoteCircuitFields
	Recovery       RecoveryCapsuleFields
}

type NoteReshapeNoteCircuitFields struct {
	Blinding           frontend.Variable
	Amount             frontend.Variable
	RecoveryCommitment frontend.Variable
}

type NoteReshapeSharedNoteContextCircuitFields struct {
	AssetID frontend.Variable
	DivGen  Point2D
}

type NoteReshapeSenderCircuitFields struct {
	Capk          Point2D
	RnkDhPk       Point2D
	RnkCommitment frontend.Variable
	Status        frontend.Variable
	Path          [ComplianceQuadTreeDepth][3]frontend.Variable
	Position      frontend.Variable
}

type NoteReshapeSpendCircuitFields struct {
	Nullifier       frontend.Variable
	RK              Point2D
	Note            NoteReshapeNoteCircuitFields
	StateProof      StateCommitmentFields
	AuthRandomizer  frontend.Variable
	HistoryRequired frontend.Variable
}

type NoteReshapeSyntheticSpendCircuitFields struct {
	NoteReshapeSpendCircuitFields
	IsDummy            frontend.Variable
	DummyNullifierSeed frontend.Variable
}

type NoteReshapeCircuit struct {
	label       string
	nIn         int
	nOut        int
	wiringTrace *WiringTranscript

	ClaimedStatementHash  frontend.Variable `gnark:",public"`
	AssetAnchor           frontend.Variable
	ComplianceAnchor      frontend.Variable
	RoutingTag            frontend.Variable
	RoutingParameterSetID frontend.Variable
	RecentPositionFloor   frontend.Variable

	Anchor                frontend.Variable
	BalanceCommitment     Point2D
	ActionBalanceBlinding frontend.Variable
	IsRegulated           frontend.Variable
	RegulatedPrecision    frontend.Variable
	UnregulatedPrecision  frontend.Variable
	RoutingAsOfHeight     frontend.Variable
	RoutingNonce          frontend.Variable

	Shared          NoteReshapeSharedNoteContextCircuitFields
	Sender          NoteReshapeSenderCircuitFields
	Auth            TransferAuthSharedFields
	Asset           AssetTreeFields
	Spends          []NoteReshapeSpendCircuitFields
	SyntheticSpends []NoteReshapeSyntheticSpendCircuitFields
	Outputs         []NoteReshapeOutputCircuitFields
}

func NewNoteReshapeCircuit(label string, nIn, nOut int) *NoteReshapeCircuit {
	circuit := &NoteReshapeCircuit{
		label:   label,
		nIn:     nIn,
		nOut:    nOut,
		Outputs: make([]NoteReshapeOutputCircuitFields, nOut),
	}
	if family, ok := generated.NoteReshapeFamilyByLabel(label); ok &&
		family.InputPadding == generated.InputPaddingSyntheticPrivate {
		circuit.SyntheticSpends = make([]NoteReshapeSyntheticSpendCircuitFields, nIn)
	} else {
		circuit.Spends = make([]NoteReshapeSpendCircuitFields, nIn)
	}
	return circuit
}

func (c *NoteReshapeCircuit) Define(api frontend.API) error {
	c.bindWiringTrace(api)
	c.bindSemantic("asset_anchor", c.AssetAnchor)
	c.bindSemantic("compliance_anchor", c.ComplianceAnchor)
	c.bindSemantic("routing.tag", c.RoutingTag)
	c.bindSemantic("routing.parameter_set_id", c.RoutingParameterSetID)
	c.bindSemantic("routing.regulated_precision", c.RegulatedPrecision)
	c.bindSemantic("routing.unregulated_precision", c.UnregulatedPrecision)
	c.bindSemantic("routing.as_of_height", c.RoutingAsOfHeight)
	c.bindSemantic("routing.nonce", c.RoutingNonce)
	family, ok := generated.NoteReshapeFamilyByLabel(c.label)
	if !ok {
		return fmt.Errorf("unsupported note reshape family %q", c.label)
	}
	if c.nIn <= 0 || c.nOut <= 0 {
		return fmt.Errorf("%s circuit shape must be positive, got %dx%d", c.label, c.nIn, c.nOut)
	}
	if len(c.Outputs) != c.nOut {
		return fmt.Errorf("%s circuit output shape mismatch: expected %d, got %d", c.label, c.nOut, len(c.Outputs))
	}
	if family.InputPadding == generated.InputPaddingSyntheticPrivate {
		if len(c.Spends) != 0 || len(c.SyntheticSpends) != c.nIn {
			return fmt.Errorf("%s synthetic input shape mismatch: expected %d synthetic spends and no fixed spends, got %d and %d", c.label, c.nIn, len(c.SyntheticSpends), len(c.Spends))
		}
	} else if len(c.Spends) != c.nIn || len(c.SyntheticSpends) != 0 {
		return fmt.Errorf("%s fixed input shape mismatch: expected %d fixed spends and no synthetic spends, got %d and %d", c.label, c.nIn, len(c.Spends), len(c.SyntheticSpends))
	}
	if family.NIn != c.nIn || family.NOut != c.nOut {
		return fmt.Errorf("%s circuit shape mismatch: registry requires %dx%d, got %dx%d", c.label, family.NIn, family.NOut, c.nIn, c.nOut)
	}
	if family.InputPadding == generated.InputPaddingSyntheticPrivate {
		if err := c.validateSyntheticInputSelectors(api, family); err != nil {
			return err
		}
	}

	sharedAK := gnarkte.Point{X: c.Auth.AK.X, Y: c.Auth.AK.Y}
	claimedBalanceCommitment := gnarkte.Point{X: c.BalanceCommitment.X, Y: c.BalanceCommitment.Y}
	sharedDivGen := gnarkte.Point{X: c.Shared.DivGen.X, Y: c.Shared.DivGen.Y}
	c.bindSemantic("claimed.statement_hash", c.ClaimedStatementHash)
	c.bindSemantic("recent_position_floor", c.RecentPositionFloor)
	c.bindSemantic("anchor", c.Anchor)
	c.bindSemantic(
		"claimed.balance_commitment",
		claimedBalanceCommitment.X,
		claimedBalanceCommitment.Y,
	)
	c.bindSemantic("action.balance_blinding", c.ActionBalanceBlinding)
	c.bindSemantic("is_regulated", c.IsRegulated)
	c.bindSemantic("shared.asset_id", c.Shared.AssetID)
	c.bindSemantic("shared.div_gen", sharedDivGen.X, sharedDivGen.Y)
	c.bindSemantic("sender.capk", c.Sender.Capk.X, c.Sender.Capk.Y)
	c.bindSemantic("sender.rnk_commitment", c.Sender.RnkCommitment)
	c.bindSemantic("sender.status", c.Sender.Status)
	c.bindSemantic("sender.path", quadPathVariables(c.Sender.Path)...)
	c.bindSemantic("sender.position", c.Sender.Position)
	c.bindSemantic("auth.ak", sharedAK.X, sharedAK.Y)
	c.bindSemantic("auth.nk", c.Auth.NK)
	c.bindSemantic("auth.ivk_reduced", c.Auth.IVKReduced)
	c.bindSemantic("auth.ivk_quotient_a", c.Auth.IVKQuotientA)
	c.traceWiring(
		"shared.bind",
		"shared.ak=auth.ak",
		"claimed.balance_commitment=balance_commitment",
		"shared.div_gen=witness.shared.div_gen",
		"shared.asset_id=witness.shared.asset_id",
	)
	c.traceWiring(
		"assert.decaf_non_identity",
		"point=auth.ak",
		"coordinate=x",
	)
	AssertDecafNonIdentity(api, sharedAK)
	c.traceWiring(
		"assert.decaf_non_identity",
		"point=shared.div_gen",
		"coordinate=x",
	)
	AssertDecafNonIdentity(api, sharedDivGen)
	c.traceWiring("decaf.assert_on_curve", "point=claimed.balance_commitment")
	assertDecafPointOnCurve(api, claimedBalanceCommitment)
	c.traceWiring("decaf.compress_to_field", "in=shared.div_gen", "out=shared.div_gen_fq")
	sharedDivGenFq, err := decafgnark.CompressToField(api, sharedDivGen)
	if err != nil {
		return err
	}
	c.bindSemantic("shared.div_gen_fq", sharedDivGenFq)

	c.traceWiring("assert.ne", "lhs=auth.ivk_reduced", "rhs=0")
	AssertIncomingViewingKeyNonzero(api, c.Auth.IVKReduced)
	c.traceWiring("decaf.diversified_transmission_key", "nk=auth.nk", "ak=shared.ak", "div_gen=shared.div_gen", "ivk_reduced=auth.ivk_reduced", "ivk_quotient_a=auth.ivk_quotient_a", "out=shared.transmission.computed")
	computedSharedTransmission, ivkBits, err := diversifiedTransmissionKeyAndBitsAfterIvkNonzero(
		api,
		c.Auth.NK,
		sharedAK,
		sharedDivGen,
		c.Auth.IVKReduced,
		c.Auth.IVKQuotientA,
	)
	if err != nil {
		return err
	}
	c.bindSemantic(
		"shared.transmission.computed",
		computedSharedTransmission.X,
		computedSharedTransmission.Y,
	)
	c.traceWiring(
		"assert.decaf_non_identity",
		"point=shared.transmission.computed",
		"coordinate=x",
	)
	AssertDecafNonIdentity(api, computedSharedTransmission)
	c.traceWiring("decaf.compress_to_field", "in=shared.transmission.computed", "out=shared.transmission.fq")
	sharedTransmissionFq, err := decafgnark.CompressToField(api, computedSharedTransmission)
	if err != nil {
		return err
	}
	c.bindSemantic("shared.transmission.fq", sharedTransmissionFq)
	c.traceWiring("assert.boolean", "var=is_regulated")
	api.AssertIsBoolean(c.IsRegulated)
	if err := verifyRoutingAssetRegistry(
		api,
		c.traceWiring,
		c.bindSemantic,
		c.Asset,
		c.AssetAnchor,
		c.Shared.AssetID,
		c.IsRegulated,
	); err != nil {
		return err
	}
	c.traceWiring("gadget.compliance_leaf", "div_gen_fq=shared.div_gen_fq", "transmission_fq=shared.transmission.fq", "asset_id=shared.asset_id", "capk=sender.capk", "rnk_dh_pk=sender.rnk_dh_pk", "rnk_commitment=sender.rnk_commitment", "status=sender.status", "out=sender.leaf_commitment")
	senderLeafCommitment, err := ComplianceLeafCommitmentFromCompressed(
		api,
		sharedDivGenFq,
		sharedTransmissionFq,
		c.Shared.AssetID,
		gnarkte.Point{X: c.Sender.Capk.X, Y: c.Sender.Capk.Y},
		gnarkte.Point{X: c.Sender.RnkDhPk.X, Y: c.Sender.RnkDhPk.Y},
		c.Sender.RnkCommitment,
		c.Sender.Status,
	)
	if err != nil {
		return err
	}
	c.bindSemantic("sender.leaf_commitment", senderLeafCommitment)
	unregulatedRingPK, _, err := UnregulatedComplianceKeys()
	if err != nil {
		return err
	}
	effectiveRingPK := SelectPoint(
		api,
		c.IsRegulated,
		gnarkte.Point{X: c.Asset.Leaf.RingPK.X, Y: c.Asset.Leaf.RingPK.Y},
		unregulatedRingPK,
	)
	derivedRNK, err := RegulatedNullifierKey(
		api,
		ivkBits,
		gnarkte.Point{X: c.Sender.RnkDhPk.X, Y: c.Sender.RnkDhPk.Y},
		sharedDivGenFq,
		sharedTransmissionFq,
		c.Shared.AssetID,
		effectiveRingPK,
	)
	if err != nil {
		return err
	}
	effectiveNK := api.Select(c.IsRegulated, derivedRNK, c.Auth.NK)
	rnkCommitment, err := ComplianceNullifierKeyCommitment(api, derivedRNK)
	if err != nil {
		return err
	}
	AssertEqualIf(api, rnkCommitment, c.Sender.RnkCommitment, c.IsRegulated)
	c.traceWiring("gadget.compliance_path", "leaf=sender.leaf_commitment", "path=sender.path", "position=sender.position", "out=sender.compliance_root")
	senderComplianceRoot, err := VerifyQuadPath(api, senderLeafCommitment, c.Sender.Path, c.Sender.Position)
	if err != nil {
		return err
	}
	c.bindSemantic("sender.compliance_root", senderComplianceRoot)
	c.traceWiring("assert.eq_if", "lhs=sender.compliance_root", "rhs=compliance_anchor", "cond=is_regulated")
	AssertEqualIf(api, senderComplianceRoot, c.ComplianceAnchor, c.IsRegulated)
	c.traceWiring("gadget.active_lifecycle", "lifecycle=sender.status", "cond=is_regulated")
	AssertActiveComplianceLifecycle(api, c.Sender.Status, c.IsRegulated)
	if err := verifySingleRoutingTag(
		api,
		c.traceWiring,
		c.RoutingTag,
		c.RoutingParameterSetID,
		c.IsRegulated,
		c.RegulatedPrecision,
		c.UnregulatedPrecision,
		c.RoutingAsOfHeight,
		c.RoutingNonce,
		sharedTransmissionFq,
	); err != nil {
		return err
	}

	inputAmounts := make([]frontend.Variable, 0, c.nIn)
	outputAmounts := make([]frontend.Variable, 0, c.nOut)
	statementFields := make([]frontend.Variable, 0, NoteReshapeStatementFieldCount(c.nIn, c.nOut))
	outputCommitments := make([]frontend.Variable, 0, 2*c.nOut)
	nullifiersAndRKs := make([]frontend.Variable, 0, 3*c.nIn)

	for i := 0; i < c.nIn; i++ {
		spendName := fmt.Sprintf("spend%d", i)
		c.traceWiring("spend.begin", spendName)
		var amount, nullifier, rkCompressed, claimedHistoryRequired frontend.Variable
		if family.InputPadding == generated.InputPaddingSyntheticPrivate {
			amount, nullifier, rkCompressed, err = c.verifyPaddedNoteReshapeSpend(
				api,
				spendName,
				sharedAK,
				sharedDivGenFq,
				sharedTransmissionFq,
				effectiveNK,
				&c.SyntheticSpends[i],
				i,
			)
			claimedHistoryRequired = c.SyntheticSpends[i].HistoryRequired
		} else {
			amount, nullifier, rkCompressed, err = c.verifyFixedNoteReshapeSpend(
				api,
				spendName,
				sharedAK,
				sharedDivGenFq,
				sharedTransmissionFq,
				effectiveNK,
				&c.Spends[i],
			)
			claimedHistoryRequired = c.Spends[i].HistoryRequired
		}
		if err != nil {
			return err
		}
		c.traceWiring("spend.collect", spendName, "amount->input_amounts", "nullifier->statement.nullifiers_and_rks", "rk_compressed->statement.nullifiers_and_rks")
		inputAmounts = append(inputAmounts, amount)
		nullifiersAndRKs = append(nullifiersAndRKs, nullifier, rkCompressed, claimedHistoryRequired)
	}

	for i := range c.Outputs {
		outputName := fmt.Sprintf("output%d", i)
		c.traceWiring("output.begin", outputName)
		amount, commitment, err := c.verifyFixedNoteReshapeOutput(
			api,
			outputName,
			sharedDivGenFq,
			sharedTransmissionFq,
			&c.Outputs[i],
		)
		if err != nil {
			return err
		}
		c.traceWiring("output.collect", outputName, "amount->output_amounts", "commitment->statement.output_commitments")
		outputAmounts = append(outputAmounts, amount)
		outputCommitments = append(
			outputCommitments,
			commitment,
			c.Outputs[i].Note.RecoveryCommitment,
		)
	}

	// NB-1: note_reshape is always conservation-exact (single asset ID shared
	// by every spend/output, net value change is always zero), so the
	// balance commitment collapses to a Sigma(in)=Sigma(out) assert plus the
	// blinding ladder - no per-amount value-generator ladders are needed.
	c.traceWiring("decaf.conservation_net_balance_commitment", "inputs=input_amounts", "outputs=output_amounts", "blinding=action_balance_blinding", "out=balance_commitment.computed")
	balanceCommitmentPoint, err := computeConservationNetBalanceCommitment(
		api,
		inputAmounts,
		outputAmounts,
		c.ActionBalanceBlinding,
	)
	if err != nil {
		return err
	}
	c.bindSemantic(
		"balance_commitment.computed",
		balanceCommitmentPoint.X,
		balanceCommitmentPoint.Y,
	)
	c.traceWiring("decaf.assert_equivalent", "lhs=balance_commitment.computed", "rhs=claimed.balance_commitment")
	decafgnark.AssertEquivalent(api, balanceCommitmentPoint, claimedBalanceCommitment)
	c.traceWiring("decaf.compress_to_field", "in=balance_commitment.computed", "out=balance_commitment.fq")
	balanceCommitmentFq, err := decafgnark.CompressToField(api, balanceCommitmentPoint)
	if err != nil {
		return err
	}
	c.bindSemantic("balance_commitment.fq", balanceCommitmentFq)

	c.traceWiring("statement.append", "field=anchor")
	statementFields = append(statementFields, c.Anchor)
	c.traceWiring("statement.append_all", "fields=output_commitments")
	statementFields = append(statementFields, outputCommitments...)
	c.traceWiring("statement.append", "field=balance_commitment.fq")
	statementFields = append(statementFields, balanceCommitmentFq)
	statementFields = append(
		statementFields,
		c.AssetAnchor,
		c.ComplianceAnchor,
		c.RoutingTag,
		c.RoutingParameterSetID,
	)
	statementFields = append(statementFields, c.RecentPositionFloor)
	c.traceWiring("statement.append_all", "fields=nullifiers_and_rks")
	statementFields = append(statementFields, nullifiersAndRKs...)
	c.traceWiring("statement.hash", "family="+c.label, "fields=statement_fields", "out=statement_hash")
	c.bindSemantic("statement.fields", statementFields...)
	statementHash, err := noteReshapeStatementHash(api, c.label, c.nIn, c.nOut, statementFields)
	if err != nil {
		return err
	}
	c.bindSemantic("statement.hash", statementHash)
	c.traceWiring("assert.eq", "lhs=statement_hash", "rhs=claimed_statement_hash")
	api.AssertIsEqual(statementHash, c.ClaimedStatementHash)
	return nil
}

func (c *NoteReshapeCircuit) validateSyntheticInputSelectors(api frontend.API, family generated.NoteReshapeFamilySpec) error {
	for index, spend := range c.SyntheticSpends {
		c.traceWiring("assert.boolean", fmt.Sprintf("value=spend%d.is_dummy", index))
		api.AssertIsBoolean(spend.IsDummy)
	}
	c.traceWiring("assert.dummy_suffix", "flags=spends.is_dummy")
	assertDummySuffix(api, func(index int) frontend.Variable { return c.SyntheticSpends[index].IsDummy }, c.nIn)
	activeInputs := frontend.Variable(0)
	for _, spend := range c.SyntheticSpends {
		activeInputs = api.Add(activeInputs, api.Sub(1, spend.IsDummy))
	}
	c.traceWiring("assert.active_range", "value=active_input_count", fmt.Sprintf("min=%d", family.MinRealInputs), fmt.Sprintf("max=%d", family.MaxRealInputs))
	assertIntegerRange(api, activeInputs, family.MinRealInputs, family.MaxRealInputs)
	return nil
}

func assertDummySuffix(api frontend.API, flag func(int) frontend.Variable, length int) {
	api.AssertIsEqual(flag(0), 0)
	for index := 1; index < length; index++ {
		api.AssertIsEqual(
			api.Mul(flag(index-1), api.Sub(1, flag(index))),
			0,
		)
	}
}

func assertIntegerRange(api frontend.API, value frontend.Variable, min, max int) {
	product := frontend.Variable(1)
	for allowed := min; allowed <= max; allowed++ {
		product = api.Mul(product, api.Sub(value, allowed))
	}
	api.AssertIsEqual(product, 0)
}

func noteReshapeSyntheticDummyNullifier(
	api frontend.API,
	seed frontend.Variable,
	authRandomizer frontend.Variable,
	slotIndex int,
) (frontend.Variable, error) {
	domainBytes := blake2b.Sum512([]byte("shieldd.note_reshape.synthetic_dummy.nullifier"))
	domain := LittleEndianBytesToBigInt(domainBytes[:])
	return Poseidon377Hash3(api, domain, [3]frontend.Variable{seed, authRandomizer, slotIndex})
}

func noteReshapeStatementHash(
	api frontend.API,
	label string,
	nIn, nOut int,
	fields []frontend.Variable,
) (frontend.Variable, error) {
	return NoteReshapeStatementHashForShape(api, label, nIn, nOut, fields)
}

func (c *NoteReshapeCircuit) verifyPaddedNoteReshapeSpend(
	api frontend.API,
	name string,
	sharedAK gnarkte.Point,
	sharedDivGenFq frontend.Variable,
	sharedTransmissionFq frontend.Variable,
	effectiveNK frontend.Variable,
	spend *NoteReshapeSyntheticSpendCircuitFields,
	index int,
) (frontend.Variable, frontend.Variable, frontend.Variable, error) {
	rkClaimed := gnarkte.Point{X: spend.RK.X, Y: spend.RK.Y}
	isNotDummy := api.Sub(1, spend.IsDummy)

	c.traceWiring(
		"gadget.note_commitment",
		"blinding="+name+".note.blinding",
		"amount="+name+".note.amount",
		"asset_id=shared.asset_id",
		"div_gen_fq=shared.div_gen_fq",
		"transmission_key_s=shared.transmission.fq",
		"out="+name+".note.commitment.computed",
	)
	spentCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		spend.Note.Blinding,
		spend.Note.Amount,
		c.Shared.AssetID,
		sharedDivGenFq,
		sharedTransmissionFq,
		spend.Note.RecoveryCommitment,
	)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(
		name+".note_commitment.inputs",
		spend.Note.Blinding,
		spend.Note.Amount,
		c.Shared.AssetID,
		sharedDivGenFq,
		sharedTransmissionFq,
		spend.Note.RecoveryCommitment,
	)
	c.bindSemantic(name+".note.commitment.computed", spentCommitment)
	c.bindSemantic(name+".state_proof.commitment", spend.StateProof.Commitment)
	c.traceWiring("assert.eq_if", "lhs="+name+".note.commitment.computed", "rhs="+name+".state_proof.commitment", "enabled="+name+".is_real")
	AssertEqualIf(api, spentCommitment, spend.StateProof.Commitment, isNotDummy)

	c.traceWiring("gadget.nullifier", "nk=auth.nk", "commitment="+name+".state_proof.commitment", "position="+name+".state_proof.position", "out="+name+".nullifier.real")
	realNullifier, err := Nullifier(api, effectiveNK, spend.StateProof.Commitment, spend.StateProof.Position)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(name+".nullifier.real", realNullifier)
	statePath := make([][3]frontend.Variable, len(spend.StateProof.Path))
	copy(statePath, spend.StateProof.Path[:])
	c.traceWiring("gadget.state_commitment_path", "commitment="+name+".state_proof.commitment", "position="+name+".state_proof.position", "path="+name+".state_proof.path", "out="+name+".anchor.computed")
	anchor, err := VerifyStateCommitmentPath(api, spend.StateProof.Commitment, spend.StateProof.Position, statePath)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(name+".anchor.computed", anchor)
	c.traceWiring("assert.eq_if", "lhs="+name+".anchor.computed", "rhs=anchor", "enabled="+name+".is_real")
	AssertEqualIf(api, anchor, c.Anchor, isNotDummy)

	c.traceWiring("gadget.synthetic_dummy_nullifier", "seed="+name+".dummy_nullifier_seed", "randomizer="+name+".auth_randomizer", fmt.Sprintf("slot=%d", index), "out="+name+".nullifier.dummy")
	syntheticNullifier, err := noteReshapeSyntheticDummyNullifier(
		api,
		spend.DummyNullifierSeed,
		spend.AuthRandomizer,
		index,
	)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(name+".nullifier.dummy", syntheticNullifier)
	c.traceWiring("dummy.mux", "flag="+name+".is_dummy", "real="+name+".nullifier.real", "dummy="+name+".nullifier.dummy", "out="+name+".nullifier.selected")
	nullifier := api.Add(
		api.Mul(isNotDummy, realNullifier),
		api.Mul(spend.IsDummy, syntheticNullifier),
	)
	c.bindSemantic(name+".nullifier.selected", nullifier)
	c.bindSemantic(name+".nullifier.claimed", spend.Nullifier)
	c.traceWiring("assert.eq", "lhs="+name+".nullifier", "rhs="+name+".nullifier.selected")
	api.AssertIsEqual(spend.Nullifier, nullifier)

	c.traceWiring("decaf.randomized_verification_key", "ak=shared.ak", "randomizer="+name+".auth_randomizer", "out="+name+".rk.real")
	computedRK, err := RandomizedVerificationKey(api, sharedAK, spend.AuthRandomizer)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(name+".rk.real", computedRK.X, computedRK.Y)
	c.bindSemantic(name+".rk.claimed", rkClaimed.X, rkClaimed.Y)
	c.traceWiring("decaf.assert_equivalent_if", "lhs="+name+".rk.real", "rhs="+name+".rk.claimed", "enabled="+name+".is_real")
	decafgnark.AssertEquivalentIf(api, computedRK, rkClaimed, isNotDummy)
	c.traceWiring("assert.eq_if", "lhs="+name+".note.amount", "rhs=0", "enabled="+name+".is_dummy")
	AssertEqualIf(api, spend.Note.Amount, 0, spend.IsDummy)

	c.traceWiring("decaf.compress_to_field", "in="+name+".rk.claimed", "out="+name+".rk.compressed")
	rkCompressed, err := decafgnark.CompressToField(api, rkClaimed)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(name+".rk.compressed", rkCompressed)
	c.traceWiring(
		"history.classify",
		"position="+name+".state_proof.position",
		"floor=recent_position_floor",
		"is_dummy="+name+".is_dummy",
		"out="+name+".history_required",
	)
	c.bindSemantic(name+".history_required", spend.HistoryRequired)
	api.AssertIsEqual(
		spend.HistoryRequired,
		historyRequired(api, spend.StateProof.Position, c.RecentPositionFloor, spend.IsDummy),
	)
	return spend.Note.Amount, nullifier, rkCompressed, nil
}

func (c *NoteReshapeCircuit) verifyFixedNoteReshapeSpend(
	api frontend.API,
	name string,
	sharedAK gnarkte.Point,
	sharedDivGenFq frontend.Variable,
	sharedTransmissionFq frontend.Variable,
	effectiveNK frontend.Variable,
	spend *NoteReshapeSpendCircuitFields,
) (frontend.Variable, frontend.Variable, frontend.Variable, error) {
	rkClaimed := gnarkte.Point{X: spend.RK.X, Y: spend.RK.Y}

	c.traceWiring(
		"gadget.note_commitment",
		"blinding="+name+".note.blinding",
		"amount="+name+".note.amount",
		"asset_id=shared.asset_id",
		"div_gen_fq=shared.div_gen_fq",
		"transmission_key_s=shared.transmission.fq",
		"out="+name+".note.commitment.computed",
	)
	spentCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		spend.Note.Blinding,
		spend.Note.Amount,
		c.Shared.AssetID,
		sharedDivGenFq,
		sharedTransmissionFq,
		spend.Note.RecoveryCommitment,
	)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(
		name+".note_commitment.inputs",
		spend.Note.Blinding,
		spend.Note.Amount,
		c.Shared.AssetID,
		sharedDivGenFq,
		sharedTransmissionFq,
	)
	c.bindSemantic(name+".note.commitment.computed", spentCommitment)
	c.bindSemantic(name+".state_proof.commitment", spend.StateProof.Commitment)
	c.traceWiring("assert.eq", "lhs="+name+".note.commitment.computed", "rhs="+name+".state_proof.commitment")
	api.AssertIsEqual(spentCommitment, spend.StateProof.Commitment)

	c.traceWiring("gadget.nullifier", "nk=auth.nk", "commitment="+name+".state_proof.commitment", "position="+name+".state_proof.position", "out="+name+".nullifier.computed")
	nullifier, err := Nullifier(api, effectiveNK, spend.StateProof.Commitment, spend.StateProof.Position)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(name+".nullifier.computed", nullifier)
	c.bindSemantic(name+".nullifier.claimed", spend.Nullifier)
	c.traceWiring("assert.eq", "lhs="+name+".nullifier.computed", "rhs="+name+".nullifier")
	api.AssertIsEqual(nullifier, spend.Nullifier)

	statePath := make([][3]frontend.Variable, len(spend.StateProof.Path))
	copy(statePath, spend.StateProof.Path[:])
	c.traceWiring("gadget.state_commitment_path", "commitment="+name+".state_proof.commitment", "position="+name+".state_proof.position", "path="+name+".state_proof.path", "out="+name+".anchor.computed")
	anchor, err := VerifyStateCommitmentPath(api, spend.StateProof.Commitment, spend.StateProof.Position, statePath)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(name+".anchor.computed", anchor)
	c.traceWiring("assert.eq", "lhs="+name+".anchor.computed", "rhs=anchor")
	api.AssertIsEqual(anchor, c.Anchor)

	c.traceWiring("decaf.randomized_verification_key", "ak=shared.ak", "randomizer="+name+".auth_randomizer", "out="+name+".rk.computed")
	computedRK, err := RandomizedVerificationKey(api, sharedAK, spend.AuthRandomizer)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(name+".rk.computed", computedRK.X, computedRK.Y)
	c.bindSemantic(name+".rk.claimed", rkClaimed.X, rkClaimed.Y)
	c.traceWiring("decaf.assert_equivalent", "lhs="+name+".rk.computed", "rhs="+name+".rk.claimed")
	decafgnark.AssertEquivalent(api, computedRK, rkClaimed)
	c.traceWiring("decaf.compress_to_field", "in="+name+".rk.claimed", "out="+name+".rk.compressed")
	rkCompressed, err := decafgnark.CompressToField(api, rkClaimed)
	if err != nil {
		return nil, nil, nil, err
	}
	c.bindSemantic(name+".rk.compressed", rkCompressed)
	c.traceWiring(
		"history.classify",
		"position="+name+".state_proof.position",
		"floor=recent_position_floor",
		"is_dummy=0",
		"out="+name+".history_required",
	)
	c.bindSemantic(name+".history_required", spend.HistoryRequired)
	api.AssertIsEqual(
		spend.HistoryRequired,
		historyRequired(api, spend.StateProof.Position, c.RecentPositionFloor, 0),
	)

	return spend.Note.Amount, nullifier, rkCompressed, nil
}

func (c *NoteReshapeCircuit) verifyFixedNoteReshapeOutput(
	api frontend.API,
	name string,
	sharedDivGenFq frontend.Variable,
	sharedTransmissionFq frontend.Variable,
	output *NoteReshapeOutputCircuitFields,
) (frontend.Variable, frontend.Variable, error) {
	c.traceWiring(
		"gadget.note_commitment",
		"blinding="+name+".note.blinding",
		"amount="+name+".note.amount",
		"asset_id=shared.asset_id",
		"div_gen_fq=shared.div_gen_fq",
		"transmission_key_s=shared.transmission.fq",
		"out="+name+".note.commitment.computed",
	)
	noteCommitment, err := NoteCommitmentWithCompressedDivGen(
		api,
		output.Note.Blinding,
		output.Note.Amount,
		c.Shared.AssetID,
		sharedDivGenFq,
		sharedTransmissionFq,
		output.Note.RecoveryCommitment,
	)
	if err != nil {
		return nil, nil, err
	}
	c.bindSemantic(
		name+".note_commitment.inputs",
		output.Note.Blinding,
		output.Note.Amount,
		c.Shared.AssetID,
		sharedDivGenFq,
		sharedTransmissionFq,
	)
	c.bindSemantic(name+".note.commitment.computed", noteCommitment)
	c.bindSemantic(name+".note.commitment.claimed", output.NoteCommitment)
	c.traceWiring("assert.eq", "lhs="+name+".note.commitment.computed", "rhs="+name+".note_commitment")
	api.AssertIsEqual(noteCommitment, output.NoteCommitment)
	if err := VerifyRecoveryCapsule(
		api,
		gnarkte.Point{X: c.Sender.Capk.X, Y: c.Sender.Capk.Y},
		output.Note.Amount,
		output.Note.Blinding,
		output.Recovery,
	); err != nil {
		return nil, nil, err
	}
	api.AssertIsEqual(output.Recovery.Commitment, output.Note.RecoveryCommitment)

	return output.Note.Amount, noteCommitment, nil
}
