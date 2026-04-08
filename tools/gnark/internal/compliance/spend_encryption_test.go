package compliance

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type spendComplianceCircuit struct {
	AssetAnchor      frontend.Variable
	ComplianceAnchor frontend.Variable

	NoteAssetID frontend.Variable
	NoteAmount  frontend.Variable
	NoteDivGenX frontend.Variable
	NoteDivGenY frontend.Variable
	NoteTransX  frontend.Variable
	NoteTransY  frontend.Variable

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
	AssetPath                 [ComplianceQuadTreeDepth][3]frontend.Variable
	AssetPosition             frontend.Variable

	UserDivGenX        frontend.Variable
	UserDivGenY        frontend.Variable
	UserTransX         frontend.Variable
	UserTransY         frontend.Variable
	UserAssetID        frontend.Variable
	UserD              frontend.Variable
	CompliancePath     [ComplianceQuadTreeDepth][3]frontend.Variable
	CompliancePosition frontend.Variable

	IsRegulated frontend.Variable
	IsFlagged   frontend.Variable

	EpkX                frontend.Variable
	EpkY                frontend.Variable
	C2Core              frontend.Variable
	ComplianceCipher0   frontend.Variable
	ComplianceCipher1   frontend.Variable
	ComplianceCipher2   frontend.Variable
	ComplianceCipher3   frontend.Variable
	ComplianceCipher4   frontend.Variable
	TargetTimestamp     frontend.Variable
	DleqC               frontend.Variable
	DleqS               frontend.Variable
	ComplianceEphemeral frontend.Variable
	Salt                frontend.Variable
}

func (c *spendComplianceCircuit) Define(api frontend.API) error {
	noteDivGen := gnarkte.Point{X: c.NoteDivGenX, Y: c.NoteDivGenY}
	noteTrans := gnarkte.Point{X: c.NoteTransX, Y: c.NoteTransY}
	indexedLeaf := IndexedLeafInputs{
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
	epk := gnarkte.Point{X: c.EpkX, Y: c.EpkY}
	noteDivGenFq, err := primitives.Decaf377CompressToField(api, noteDivGen)
	if err != nil {
		return err
	}
	noteTransFq, err := primitives.Decaf377CompressToField(api, noteTrans)
	if err != nil {
		return err
	}
	epkFq, err := primitives.Decaf377CompressToField(api, epk)
	if err != nil {
		return err
	}

	assetLeafCommitment, err := IndexedLeafCommitment(api, indexedLeaf)
	if err != nil {
		return err
	}
	assetRoot, err := VerifyQuadPath(api, assetLeafCommitment, c.AssetPath, c.AssetPosition)
	if err != nil {
		return err
	}
	api.AssertIsEqual(assetRoot, c.AssetAnchor)

	userLeafCommitment, err := complianceLeafCommitment(api, userDivGen, userTrans, c.UserAssetID, c.UserD)
	if err != nil {
		return err
	}
	complianceRoot, err := VerifyQuadPath(api, userLeafCommitment, c.CompliancePath, c.CompliancePosition)
	if err != nil {
		return err
	}
	AssertEqualIf(api, complianceRoot, c.ComplianceAnchor, c.IsRegulated)

	primitives.AssertDecafEquivalent(api, userDivGen, noteDivGen)
	primitives.AssertDecafEquivalent(api, userTrans, noteTrans)
	api.AssertIsEqual(c.UserAssetID, c.NoteAssetID)

	VerifyThresholdFlagSimple(api, c.NoteAmount, c.IndexedLeafThreshold, c.IsFlagged)

	ack, err := DeriveACKFromLeafD(api, indexedLeaf.RingPK, c.UserD)
	if err != nil {
		return err
	}
	ssDetection, ssCoreUser, ssCore, err := DeriveSharedSecretsSpend(
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
		epkFq,
		c.Salt,
		c.NoteAmount,
		c.NoteAssetID,
		noteDivGenFq,
		noteTransFq,
		[5]frontend.Variable{
			c.ComplianceCipher0,
			c.ComplianceCipher1,
			c.ComplianceCipher2,
			c.ComplianceCipher3,
			c.ComplianceCipher4,
		},
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
	return VerifyDLEQ(
		api,
		c.ComplianceEphemeral,
		ack,
		ssCoreUser,
		epk,
		metadataHash,
		c.DleqC,
		c.DleqS,
		c.IsRegulated,
	)
}

func TestVerifyPoseidonEncryptionSpendNativeMatchesFixture(t *testing.T) {
	fixture, err := primitives.LoadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}
	indexedLeaf, err := IndexedLeafInputsFromFixture(fixture)
	if err != nil {
		t.Fatalf("decode indexed leaf: %v", err)
	}
	userD := primitives.MustBigInt(fixture.Private.UserDDecimal)
	ack, err := DeriveACKFromLeafDNative(indexedLeaf.RingPK, userD)
	if err != nil {
		t.Fatalf("derive ack: %v", err)
	}
	ssDetection, ssCore, err := DeriveSharedSecretsSpendNative(
		primitives.MustBigInt(fixture.Private.ComplianceEphemeralSecret),
		ack,
		indexedLeaf.DKPub,
		fixture.Private.IsFlagged,
	)
	if err != nil {
		t.Fatalf("derive shared secrets: %v", err)
	}
	computed, err := VerifyPoseidonEncryptionSpendNative(
		primitives.MustBigInt(fixture.Private.NoteAmount),
		primitives.MustBigInt(fixture.Private.NoteAssetID),
		pointAffineToNativeFixture(fixture.Private.DiversifiedGeneratorAffine),
		pointAffineToNativeFixture(fixture.Private.TransmissionKeyAffine),
		ssDetection,
		ssCore,
		pointAffineToNativeFixture(fixture.Public.EpkAffine),
		primitives.MustBigInt(fixture.Public.C2Core),
		fixture.Private.IsFlagged,
		primitives.MustBigInt(fixture.Private.Salt),
	)
	if err != nil {
		t.Fatalf("recompute spend ciphertext: %v", err)
	}
	for i, got := range computed {
		if got.String() != fixture.Public.ComplianceCiphertext[i] {
			t.Fatalf("ciphertext[%d] mismatch: got %s want %s", i, got.String(), fixture.Public.ComplianceCiphertext[i])
		}
	}
}

func TestSpendComplianceCircuitMatchesFixture(t *testing.T) {
	fixture, err := primitives.LoadSpendFixture()
	if err != nil {
		t.Fatalf("load spend fixture: %v", err)
	}
	indexedLeaf, err := IndexedLeafInputsFromFixture(fixture)
	if err != nil {
		t.Fatalf("decode indexed leaf: %v", err)
	}
	assetPath, err := QuadPathFromFixture(fixture.Private.AssetPath)
	if err != nil {
		t.Fatalf("decode asset path: %v", err)
	}
	compliancePath, err := QuadPathFromFixture(fixture.Private.CompliancePath)
	if err != nil {
		t.Fatalf("decode compliance path: %v", err)
	}
	var assetPathVars [ComplianceQuadTreeDepth][3]frontend.Variable
	var compliancePathVars [ComplianceQuadTreeDepth][3]frontend.Variable
	for i := 0; i < ComplianceQuadTreeDepth; i++ {
		for j := 0; j < 3; j++ {
			assetPathVars[i][j] = assetPath[i][j].String()
			compliancePathVars[i][j] = compliancePath[i][j].String()
		}
	}

	assignment := &spendComplianceCircuit{
		AssetAnchor:      fixture.Public.AssetAnchor,
		ComplianceAnchor: fixture.Public.ComplianceAnchor,
		NoteAssetID:      fixture.Private.NoteAssetID,
		NoteAmount:       fixture.Private.NoteAmount,
		NoteDivGenX:      fixture.Private.DiversifiedGeneratorAffine.X,
		NoteDivGenY:      fixture.Private.DiversifiedGeneratorAffine.Y,
		NoteTransX:       fixture.Private.TransmissionKeyAffine.X,
		NoteTransY:       fixture.Private.TransmissionKeyAffine.Y,

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

		UserDivGenX:        fixture.Private.UserDiversifiedGeneratorAffine.X,
		UserDivGenY:        fixture.Private.UserDiversifiedGeneratorAffine.Y,
		UserTransX:         fixture.Private.UserTransmissionKeyAffine.X,
		UserTransY:         fixture.Private.UserTransmissionKeyAffine.Y,
		UserAssetID:        fixture.Private.NoteAssetID,
		UserD:              fixture.Private.UserDDecimal,
		CompliancePath:     compliancePathVars,
		CompliancePosition: fixture.Private.CompliancePosition,

		IsRegulated: boolToField(fixture.Private.IsRegulated),
		IsFlagged:   boolToField(fixture.Private.IsFlagged),

		EpkX:                fixture.Public.EpkAffine.X,
		EpkY:                fixture.Public.EpkAffine.Y,
		C2Core:              fixture.Public.C2Core,
		ComplianceCipher0:   fixture.Public.ComplianceCiphertext[0],
		ComplianceCipher1:   fixture.Public.ComplianceCiphertext[1],
		ComplianceCipher2:   fixture.Public.ComplianceCiphertext[2],
		ComplianceCipher3:   fixture.Public.ComplianceCiphertext[3],
		ComplianceCipher4:   fixture.Public.ComplianceCiphertext[4],
		TargetTimestamp:     fixture.Public.TargetTimestamp,
		DleqC:               fixture.Public.DleqC,
		DleqS:               fixture.Public.DleqS,
		ComplianceEphemeral: fixture.Private.ComplianceEphemeralSecret,
		Salt:                fixture.Private.Salt,
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&spendComplianceCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestSpendComplianceCircuitCompiles(t *testing.T) {
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &spendComplianceCircuit{})
	if err != nil {
		t.Fatalf("compile spend compliance circuit: %v", err)
	}
}

func pointAffineToNativeFixture(point primitives.PointAffineFixture) gnarkte.Point {
	return gnarkte.Point{
		X: primitives.MustBigInt(point.X),
		Y: primitives.MustBigInt(point.Y),
	}
}

func boolToField(value bool) int {
	if value {
		return 1
	}
	return 0
}

func complianceLeafCommitment(
	api frontend.API,
	diversifiedGenerator gnarkte.Point,
	transmissionKey gnarkte.Point,
	assetID frontend.Variable,
	d frontend.Variable,
) (frontend.Variable, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}

	diversifiedGeneratorFq, err := primitives.Decaf377CompressToField(api, diversifiedGenerator)
	if err != nil {
		return nil, err
	}
	transmissionKeyFq, err := primitives.Decaf377CompressToField(api, transmissionKey)
	if err != nil {
		return nil, err
	}

	return primitives.Poseidon377Hash4(
		api,
		primitives.MustBigInt(vectors.Poseidon377.ComplianceLeafDomain),
		[4]frontend.Variable{
			diversifiedGeneratorFq,
			transmissionKeyFq,
			assetID,
			d,
		},
	)
}
