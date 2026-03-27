package prototype

import (
	_ "embed"
	"encoding/json"
	"math/big"
	"sync"
)

//go:embed vectors/phase05_vectors.json
var embeddedPhase05Vectors []byte

//go:embed vectors/spend_fixture.json
var embeddedSpendFixture []byte

//go:embed vectors/spend_witness_v1.bin
var embeddedSpendWitnessV1 []byte

type curveVectors struct {
	A                                     string `json:"a"`
	D                                     string `json:"d"`
	AMinusD                               string `json:"a_minus_d"`
	Order                                 string `json:"order"`
	Zeta                                  string `json:"zeta"`
	GeneratorX                            string `json:"generator_x"`
	GeneratorY                            string `json:"generator_y"`
	GeneratorEncodingHex                  string `json:"generator_encoding_hex"`
	GeneratorCompressToField              string `json:"generator_compress_to_field"`
	ValueBlindingGeneratorInput           string `json:"value_blinding_generator_input"`
	ValueBlindingGeneratorX               string `json:"value_blinding_generator_x"`
	ValueBlindingGeneratorY               string `json:"value_blinding_generator_y"`
	ValueBlindingGeneratorEncodingHex     string `json:"value_blinding_generator_encoding_hex"`
	ValueBlindingGeneratorCompressToField string `json:"value_blinding_generator_compress_to_field"`
}

type poseidonRateVectors struct {
	Alpha         uint32   `json:"alpha"`
	FullRounds    int      `json:"full_rounds"`
	PartialRounds int      `json:"partial_rounds"`
	Width         int      `json:"width"`
	Rate          int      `json:"rate"`
	MDS           []string `json:"mds"`
	ARC           []string `json:"arc"`
}

type poseidonVectors struct {
	SpendDomain           string              `json:"spend_domain"`
	SpendPad0             string              `json:"spend_pad_0"`
	SpendPad1             string              `json:"spend_pad_1"`
	NoteCommitDomain      string              `json:"note_commit_domain"`
	NullifierDomain       string              `json:"nullifier_domain"`
	ValueGeneratorDomain  string              `json:"value_generator_domain"`
	IVKDomain             string              `json:"ivk_domain"`
	TCTDomain             string              `json:"tct_domain"`
	SenderLeafDomain      string              `json:"sender_leaf_domain"`
	ComplianceLeafDomain  string              `json:"compliance_leaf_domain"`
	IssuerDetectionDomain string              `json:"issuer_detection_domain"`
	DLEQMetadataDomain    string              `json:"dleq_metadata_domain"`
	IMTLeafDomain         string              `json:"imt_leaf_domain"`
	IMTParamsDomain       string              `json:"imt_params_domain"`
	IMTRingDomain         string              `json:"imt_ring_domain"`
	Hash7Domain           string              `json:"hash7_domain"`
	Hash7Inputs           []string            `json:"hash7_inputs"`
	Hash7Output           string              `json:"hash7_output"`
	Rate1                 poseidonRateVectors `json:"rate_1"`
	Rate2                 poseidonRateVectors `json:"rate_2"`
	Rate3                 poseidonRateVectors `json:"rate_3"`
	Rate4                 poseidonRateVectors `json:"rate_4"`
	Rate5                 poseidonRateVectors `json:"rate_5"`
	Rate6                 poseidonRateVectors `json:"rate_6"`
	Rate7                 poseidonRateVectors `json:"rate_7"`
}

type decafCompressVector struct {
	Scalar          string `json:"scalar"`
	X               string `json:"x"`
	Y               string `json:"y"`
	CompressToField string `json:"compress_to_field"`
	EncodingHex     string `json:"encoding_hex"`
}

type decafEncodeVector struct {
	Input           string `json:"input"`
	X               string `json:"x"`
	Y               string `json:"y"`
	CompressToField string `json:"compress_to_field"`
	EncodingHex     string `json:"encoding_hex"`
}

type dleqFixture struct {
	ChallengeKeepBits int    `json:"challenge_keep_bits"`
	MetadataHash      string `json:"metadata_hash"`
	WrongMetadataHash string `json:"wrong_metadata_hash"`
	R                 string `json:"r"`
	AckX              string `json:"ack_x"`
	AckY              string `json:"ack_y"`
	EpkX              string `json:"epk_x"`
	EpkY              string `json:"epk_y"`
	DleqC             string `json:"dleq_c"`
	DleqS             string `json:"dleq_s"`
}

type prototypeVectors struct {
	Decaf377CompanionCurve curveVectors          `json:"decaf377_companion_curve"`
	Poseidon377            poseidonVectors       `json:"poseidon377"`
	Decaf377Compress       []decafCompressVector `json:"decaf377_compress_vectors"`
	Decaf377Encode         []decafEncodeVector   `json:"decaf377_encode_vectors"`
	DleqFixture            dleqFixture           `json:"dleq_fixture"`
}

type spendPublicFixture struct {
	Anchor                  string             `json:"anchor"`
	BalanceCommitmentHex    string             `json:"balance_commitment_hex"`
	BalanceCommitmentAffine pointAffineFixture `json:"balance_commitment_affine"`
	Nullifier               string             `json:"nullifier"`
	RKHex                   string             `json:"rk_hex"`
	RKAffine                pointAffineFixture `json:"rk_affine"`
	AssetAnchor             string             `json:"asset_anchor"`
	ComplianceAnchor        string             `json:"compliance_anchor"`
	EpkHex                  string             `json:"epk_hex"`
	EpkAffine               pointAffineFixture `json:"epk_affine"`
	C2Core                  string             `json:"c2_core"`
	ComplianceCiphertext    []string           `json:"compliance_ciphertext"`
	TargetTimestamp         string             `json:"target_timestamp"`
	DleqC                   string             `json:"dleq_c"`
	DleqS                   string             `json:"dleq_s"`
	SenderLeafHash          string             `json:"sender_leaf_hash"`
}

type stateCommitmentProofFixture struct {
	Commitment string      `json:"commitment"`
	Position   uint64      `json:"position"`
	AuthPath   [][3]string `json:"auth_path"`
}

type pointAffineFixture struct {
	X string `json:"x"`
	Y string `json:"y"`
}

type merklePathLayerFixture struct {
	Siblings []string `json:"siblings"`
}

type merklePathFixture struct {
	Layers []merklePathLayerFixture `json:"layers"`
}

type indexedLeafFixture struct {
	Value          []byte      `json:"value"`
	NextIndex      uint64      `json:"next_index"`
	NextValue      []byte      `json:"next_value"`
	DKPub          []byte      `json:"dk_pub"`
	Threshold      json.Number `json:"threshold"`
	ChannelsHash   []byte      `json:"channels_hash"`
	RingPK         []byte      `json:"ring_pk"`
	RingIDHash     []byte      `json:"ring_id_hash"`
	PolicyIDHash   []byte      `json:"policy_id_hash"`
	PermissionHash []byte      `json:"permission_hash"`
	ResourceHash   []byte      `json:"resource_hash"`
}

type addressFixture struct {
	Inner string `json:"inner"`
}

type assetIDFixture struct {
	Inner string `json:"inner"`
}

type complianceLeafFixture struct {
	Address addressFixture `json:"address"`
	AssetID assetIDFixture `json:"assetId"`
	D       string         `json:"d"`
}

type spendPrivateFixture struct {
	Note                           json.RawMessage             `json:"note"`
	NoteBytesHex                   string                      `json:"note_bytes_hex"`
	NoteBlinding                   string                      `json:"note_blinding"`
	NoteAmount                     string                      `json:"note_amount"`
	NoteAssetID                    string                      `json:"note_asset_id"`
	DiversifiedGeneratorHex        string                      `json:"diversified_generator_hex"`
	DiversifiedGeneratorAffine     pointAffineFixture          `json:"diversified_generator_affine"`
	TransmissionKeyHex             string                      `json:"transmission_key_hex"`
	TransmissionKeyAffine          pointAffineFixture          `json:"transmission_key_affine"`
	ClueKey                        string                      `json:"clue_key"`
	StateCommitmentProof           stateCommitmentProofFixture `json:"state_commitment_proof"`
	VBlinding                      string                      `json:"v_blinding"`
	SpendAuthRandomizer            string                      `json:"spend_auth_randomizer"`
	AKHex                          string                      `json:"ak_hex"`
	AKAffine                       pointAffineFixture          `json:"ak_affine"`
	NK                             string                      `json:"nk"`
	AssetPath                      merklePathFixture           `json:"asset_path"`
	AssetPosition                  uint64                      `json:"asset_position"`
	AssetIndexedLeaf               indexedLeafFixture          `json:"asset_indexed_leaf"`
	AssetIndexedLeafDKPubAffine    pointAffineFixture          `json:"asset_indexed_leaf_dk_pub_affine"`
	AssetIndexedLeafRingPKAffine   pointAffineFixture          `json:"asset_indexed_leaf_ring_pk_affine"`
	IsRegulated                    bool                        `json:"is_regulated"`
	CompliancePath                 merklePathFixture           `json:"compliance_path"`
	CompliancePosition             uint64                      `json:"compliance_position"`
	UserLeaf                       complianceLeafFixture       `json:"user_leaf"`
	UserLeafCommitment             string                      `json:"user_leaf_commitment"`
	UserDDecimal                   string                      `json:"user_d_decimal"`
	UserDiversifiedGeneratorAffine pointAffineFixture          `json:"user_diversified_generator_affine"`
	UserTransmissionKeyAffine      pointAffineFixture          `json:"user_transmission_key_affine"`
	ComplianceEphemeralSecret      string                      `json:"compliance_ephemeral_secret"`
	TxBlindingNonce                string                      `json:"tx_blinding_nonce"`
	IsFlagged                      bool                        `json:"is_flagged"`
	Salt                           string                      `json:"salt"`
}

type spendFixture struct {
	SchemaVersion             string              `json:"schema_version"`
	Public                    spendPublicFixture  `json:"public"`
	Private                   spendPrivateFixture `json:"private"`
	StatementFields           []string            `json:"statement_fields"`
	ClaimedStatementHash      string              `json:"claimed_statement_hash"`
	WrongClaimedStatementHash string              `json:"wrong_claimed_statement_hash"`
	MetadataHash              string              `json:"metadata_hash"`
	PolicyIDHash              string              `json:"policy_id_hash"`
	ResourceHash              string              `json:"resource_hash"`
	PermissionHash            string              `json:"permission_hash"`
	Salt                      string              `json:"salt"`
}

var (
	vectorsOnce      sync.Once
	vectorsData      prototypeVectors
	vectorsErr       error
	spendFixtureOnce sync.Once
	spendFixtureData spendFixture
	spendFixtureErr  error
)

func loadPrototypeVectors() (prototypeVectors, error) {
	vectorsOnce.Do(func() {
		vectorsErr = json.Unmarshal(embeddedPhase05Vectors, &vectorsData)
	})
	return vectorsData, vectorsErr
}

func loadSpendFixture() (spendFixture, error) {
	spendFixtureOnce.Do(func() {
		spendFixtureErr = json.Unmarshal(embeddedSpendFixture, &spendFixtureData)
	})
	return spendFixtureData, spendFixtureErr
}

func loadSpendWitnessV1() []byte {
	return embeddedSpendWitnessV1
}

func mustBigInt(decimal string) *big.Int {
	value, ok := new(big.Int).SetString(decimal, 10)
	if !ok {
		panic("invalid decimal big.Int: " + decimal)
	}
	return value
}

func mustBigIntSlice(decimals []string) []*big.Int {
	values := make([]*big.Int, len(decimals))
	for i, decimal := range decimals {
		values[i] = mustBigInt(decimal)
	}
	return values
}
