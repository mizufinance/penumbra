package primitives

import (
	_ "embed"
	"encoding/json"
	"github.com/consensys/gnark-crypto/ecc"
	"math/big"
	"sync"
)

//go:embed vectors/phase05_vectors.json
var embeddedPhase05Vectors []byte

//go:embed vectors/spend_fixture.json
var embeddedSpendFixture []byte

//go:embed vectors/spend_witness_v1.bin
var embeddedSpendWitnessV1 []byte

//go:embed vectors/output_witness_v1.bin
var embeddedOutputWitnessV1 []byte

type CurveVectors struct {
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

type PoseidonRateVectors struct {
	Alpha         uint32   `json:"alpha"`
	FullRounds    int      `json:"full_rounds"`
	PartialRounds int      `json:"partial_rounds"`
	Width         int      `json:"width"`
	Rate          int      `json:"rate"`
	MDS           []string `json:"mds"`
	ARC           []string `json:"arc"`
}

type PoseidonVectors struct {
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
	Rate1                 PoseidonRateVectors `json:"rate_1"`
	Rate2                 PoseidonRateVectors `json:"rate_2"`
	Rate3                 PoseidonRateVectors `json:"rate_3"`
	Rate4                 PoseidonRateVectors `json:"rate_4"`
	Rate5                 PoseidonRateVectors `json:"rate_5"`
	Rate6                 PoseidonRateVectors `json:"rate_6"`
	Rate7                 PoseidonRateVectors `json:"rate_7"`
}

type DecafCompressVector struct {
	Scalar          string `json:"scalar"`
	X               string `json:"x"`
	Y               string `json:"y"`
	CompressToField string `json:"compress_to_field"`
	EncodingHex     string `json:"encoding_hex"`
}

type DecafEncodeVector struct {
	Input           string `json:"input"`
	X               string `json:"x"`
	Y               string `json:"y"`
	CompressToField string `json:"compress_to_field"`
	EncodingHex     string `json:"encoding_hex"`
}

type DLEQFixture struct {
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

type PrototypeVectors struct {
	Decaf377CompanionCurve CurveVectors          `json:"decaf377_companion_curve"`
	Poseidon377            PoseidonVectors       `json:"poseidon377"`
	Decaf377Compress       []DecafCompressVector `json:"decaf377_compress_vectors"`
	Decaf377Encode         []DecafEncodeVector   `json:"decaf377_encode_vectors"`
	DleqFixture            DLEQFixture           `json:"dleq_fixture"`
}

type SpendPublicFixture struct {
	Anchor                  string             `json:"anchor"`
	BalanceCommitmentHex    string             `json:"balance_commitment_hex"`
	BalanceCommitmentAffine PointAffineFixture `json:"balance_commitment_affine"`
	Nullifier               string             `json:"nullifier"`
	RKHex                   string             `json:"rk_hex"`
	RKAffine                PointAffineFixture `json:"rk_affine"`
	AssetAnchor             string             `json:"asset_anchor"`
	ComplianceAnchor        string             `json:"compliance_anchor"`
	EpkHex                  string             `json:"epk_hex"`
	EpkAffine               PointAffineFixture `json:"epk_affine"`
	C2Core                  string             `json:"c2_core"`
	ComplianceCiphertext    []string           `json:"compliance_ciphertext"`
	TargetTimestamp         string             `json:"target_timestamp"`
	DleqC                   string             `json:"dleq_c"`
	DleqS                   string             `json:"dleq_s"`
	SenderLeafHash          string             `json:"sender_leaf_hash"`
}

type StateCommitmentProofFixture struct {
	Commitment string      `json:"commitment"`
	Position   uint64      `json:"position"`
	AuthPath   [][3]string `json:"auth_path"`
}

type PointAffineFixture struct {
	X string `json:"x"`
	Y string `json:"y"`
}

type MerklePathLayerFixture struct {
	Siblings []string `json:"siblings"`
}

type MerklePathFixture struct {
	Layers []MerklePathLayerFixture `json:"layers"`
}

type IndexedLeafFixture struct {
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

type AddressFixture struct {
	Inner string `json:"inner"`
}

type AssetIDFixture struct {
	Inner string `json:"inner"`
}

type ComplianceLeafFixture struct {
	Address AddressFixture `json:"address"`
	AssetID AssetIDFixture `json:"assetId"`
	D       string         `json:"d"`
}

type SpendPrivateFixture struct {
	Note                           json.RawMessage             `json:"note"`
	NoteBytesHex                   string                      `json:"note_bytes_hex"`
	NoteBlinding                   string                      `json:"note_blinding"`
	NoteAmount                     string                      `json:"note_amount"`
	NoteAssetID                    string                      `json:"note_asset_id"`
	DiversifiedGeneratorHex        string                      `json:"diversified_generator_hex"`
	DiversifiedGeneratorAffine     PointAffineFixture          `json:"diversified_generator_affine"`
	TransmissionKeyHex             string                      `json:"transmission_key_hex"`
	TransmissionKeyAffine          PointAffineFixture          `json:"transmission_key_affine"`
	ClueKey                        string                      `json:"clue_key"`
	StateCommitmentProof           StateCommitmentProofFixture `json:"state_commitment_proof"`
	VBlinding                      string                      `json:"v_blinding"`
	SpendAuthRandomizer            string                      `json:"spend_auth_randomizer"`
	AKHex                          string                      `json:"ak_hex"`
	AKAffine                       PointAffineFixture          `json:"ak_affine"`
	NK                             string                      `json:"nk"`
	AssetPath                      MerklePathFixture           `json:"asset_path"`
	AssetPosition                  uint64                      `json:"asset_position"`
	AssetIndexedLeaf               IndexedLeafFixture          `json:"asset_indexed_leaf"`
	AssetIndexedLeafDKPubAffine    PointAffineFixture          `json:"asset_indexed_leaf_dk_pub_affine"`
	AssetIndexedLeafRingPKAffine   PointAffineFixture          `json:"asset_indexed_leaf_ring_pk_affine"`
	IsRegulated                    bool                        `json:"is_regulated"`
	CompliancePath                 MerklePathFixture           `json:"compliance_path"`
	CompliancePosition             uint64                      `json:"compliance_position"`
	UserLeaf                       ComplianceLeafFixture       `json:"user_leaf"`
	UserLeafCommitment             string                      `json:"user_leaf_commitment"`
	UserDDecimal                   string                      `json:"user_d_decimal"`
	UserDiversifiedGeneratorAffine PointAffineFixture          `json:"user_diversified_generator_affine"`
	UserTransmissionKeyAffine      PointAffineFixture          `json:"user_transmission_key_affine"`
	ComplianceEphemeralSecret      string                      `json:"compliance_ephemeral_secret"`
	TxBlindingNonce                string                      `json:"tx_blinding_nonce"`
	IsFlagged                      bool                        `json:"is_flagged"`
	Salt                           string                      `json:"salt"`
}

type SpendFixture struct {
	SchemaVersion             string              `json:"schema_version"`
	Public                    SpendPublicFixture  `json:"public"`
	Private                   SpendPrivateFixture `json:"private"`
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
	vectorsData      PrototypeVectors
	vectorsErr       error
	spendFixtureOnce sync.Once
	spendFixtureData SpendFixture
	spendFixtureErr  error
)

func ScalarField() *big.Int {
	return ecc.BLS12_377.ScalarField()
}

func LoadPrototypeVectors() (PrototypeVectors, error) {
	vectorsOnce.Do(func() {
		vectorsErr = json.Unmarshal(embeddedPhase05Vectors, &vectorsData)
	})
	return vectorsData, vectorsErr
}

func LoadSpendFixture() (SpendFixture, error) {
	spendFixtureOnce.Do(func() {
		spendFixtureErr = json.Unmarshal(embeddedSpendFixture, &spendFixtureData)
	})
	return spendFixtureData, spendFixtureErr
}

func LoadSpendWitnessV1() []byte {
	return embeddedSpendWitnessV1
}

func LoadOutputWitnessV1() []byte {
	return embeddedOutputWitnessV1
}

func MustBigInt(decimal string) *big.Int {
	value, ok := new(big.Int).SetString(decimal, 10)
	if !ok {
		panic("invalid decimal big.Int: " + decimal)
	}
	return value
}

func MustBigIntSlice(decimals []string) []*big.Int {
	values := make([]*big.Int, len(decimals))
	for i, decimal := range decimals {
		values[i] = MustBigInt(decimal)
	}
	return values
}
