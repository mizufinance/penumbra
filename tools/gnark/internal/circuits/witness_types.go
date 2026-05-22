package circuits

import (
	"github.com/consensys/gnark/frontend"
	"github.com/mizufinance/penumbra/tools/gnark/internal/compliance"
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
	SlotCount      frontend.Variable
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
	DivGen         Point2D
	Transmission   Point2D
	AssetID        frontend.Variable
	SlotID         frontend.Variable
	SlotDerivation frontend.Variable
	D              frontend.Variable
	Path           [compliance.ComplianceQuadTreeDepth][3]frontend.Variable
	Position       frontend.Variable
}

type TransferComplianceProofFields struct {
	Statement   TransferComplianceStatementFields
	Challenge   frontend.Variable
	Response    frontend.Variable
	DerivedPK   Point2D
	EncCmt      Point2D
	SharedPoint Point2D
}

type TransferComplianceStatementFields struct {
	SubjectDerivation frontend.Variable
	RingIDHash        frontend.Variable
	PolicyIDHash      frontend.Variable
	ResourceHash      frontend.Variable
	PermissionHash    frontend.Variable
	Tier              frontend.Variable
	TargetTimestamp   frontend.Variable
	Salt              frontend.Variable
}

type TransferComplianceCoreFields struct {
	Epk        Point2D
	C2         frontend.Variable
	Ciphertext [compliance.TransferCoreCiphertextFQCount]frontend.Variable
	Proof      TransferComplianceProofFields
}

type TransferComplianceExtFields struct {
	Epk        Point2D
	C2         frontend.Variable
	Ciphertext [compliance.TransferExtCiphertextFQCount]frontend.Variable
	Proof      TransferComplianceProofFields
}

type TransferComplianceFields struct {
	TransferNonceRoot   frontend.Variable
	DetectionCiphertext [compliance.TransferDetectionFQCount]frontend.Variable
	SenderRCore         frontend.Variable
	SenderRExt          frontend.Variable
	OutputRCore         frontend.Variable
	OutputRExt          frontend.Variable
	SenderCore          TransferComplianceCoreFields
	SenderExt           TransferComplianceExtFields
	OutputCore          TransferComplianceCoreFields
	OutputExt           TransferComplianceExtFields
}
