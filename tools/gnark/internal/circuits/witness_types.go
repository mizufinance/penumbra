package circuits

import (
	"github.com/consensys/gnark/frontend"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/compliance"
)

type NoteFields struct {
	Blinding         frontend.Variable
	Amount           frontend.Variable
	AssetID          frontend.Variable
	DivGen           Point2D
	TransmissionKeyS frontend.Variable
	Transmission     Point2D
	ClueKey          frontend.Variable
}

type StateCommitmentFields struct {
	Commitment frontend.Variable
	Position   frontend.Variable
	Path       [StateCommitmentDepth][3]frontend.Variable
}

type SpendAuthFields struct {
	VBlinding    frontend.Variable
	Randomizer   frontend.Variable
	AK           Point2D
	NK           frontend.Variable
	IVKReduced   frontend.Variable
	IVKQuotientA frontend.Variable
}

type IndexedLeafFields struct {
	Value          frontend.Variable
	NextIndex      frontend.Variable
	NextValue      frontend.Variable
	DKPub          Point2D
	Threshold      frontend.Variable
	ChannelsHash   frontend.Variable
	RingPK         Point2D
	RingIDHash     frontend.Variable
	PolicyIDHash   frontend.Variable
	PermissionHash frontend.Variable
	ResourceHash   frontend.Variable
}

type AssetTreeFields struct {
	Leaf     IndexedLeafFields
	Path     [compliance.ComplianceQuadTreeDepth][3]frontend.Variable
	Position frontend.Variable
}

type UserComplianceFields struct {
	DivGen       Point2D
	Transmission Point2D
	AssetID      frontend.Variable
	D            frontend.Variable
	Path         [compliance.ComplianceQuadTreeDepth][3]frontend.Variable
	Position     frontend.Variable
}

type SpendEncryptionFields struct {
	Epk                  Point2D
	C2Core               frontend.Variable
	ComplianceCiphertext [SpendCiphertextFQCount]frontend.Variable
	IsRegulated          frontend.Variable
	IsFlagged            frontend.Variable
	ComplianceEphemeral  frontend.Variable
	Salt                 frontend.Variable
	TxBlindingNonce      frontend.Variable
}

type OutputEncryptionFields struct {
	Epk1                 Point2D
	Epk2                 Point2D
	Epk3                 Point2D
	C2Core               frontend.Variable
	C2Ext                frontend.Variable
	C2Sext               frontend.Variable
	ComplianceCiphertext [compliance.OutputCiphertextFQCount]frontend.Variable
	IsRegulated          frontend.Variable
	IsFlagged            frontend.Variable
	ComplianceEphemeral  frontend.Variable
	R2                   frontend.Variable
	R3                   frontend.Variable
	Salt                 frontend.Variable
	TxBlindingNonce      frontend.Variable
}

type DLEQFields struct {
	C frontend.Variable
	S frontend.Variable
}
