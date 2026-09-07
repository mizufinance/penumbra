package circuits

import (
	"github.com/consensys/gnark/frontend"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
)

type NoteFields struct {
	Blinding         frontend.Variable
	Amount           frontend.Variable
	AssetID          frontend.Variable
	DivGen           Point2D
	TransmissionKeyS frontend.Variable
	Transmission     Point2D
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
	Value            frontend.Variable
	NextIndex        frontend.Variable
	NextValue        frontend.Variable
	DKPub            Point2D
	DailyVolumeLimit frontend.Variable
	RoutePolicyHash  frontend.Variable
	RingPK           Point2D
	RingIDHash       frontend.Variable
	PolicyIDHash     frontend.Variable
	PermissionHash   frontend.Variable
	ResourceHash     frontend.Variable
}

type AssetTreeFields struct {
	Leaf     IndexedLeafFields
	Path     [compliance.ComplianceQuadTreeDepth][3]frontend.Variable
	Position frontend.Variable
}

type TransferComplianceMetadataFields struct {
	RingIDHash      frontend.Variable
	PolicyIDHash    frontend.Variable
	ResourceHash    frontend.Variable
	PermissionHash  frontend.Variable
	TargetTimestamp frontend.Variable
	SenderCoreSalt  frontend.Variable
	SenderExtSalt   frontend.Variable
	OutputCoreSalt  frontend.Variable
	OutputExtSalt   frontend.Variable
}

type TransferComplianceCoreFields struct {
	Epk             Point2D
	C2              frontend.Variable
	KeyConfirmation frontend.Variable
	Ciphertext      [compliance.TransferCoreCiphertextFQCount]frontend.Variable
}

type TransferComplianceExtFields struct {
	Epk        Point2D
	C2         frontend.Variable
	Ciphertext [compliance.TransferExtCiphertextFQCount]frontend.Variable
}

type TransferComplianceFields struct {
	TransferNonceRoot   frontend.Variable
	DetectionCiphertext [compliance.TransferDetectionFQCount]frontend.Variable
	Metadata            TransferComplianceMetadataFields
	SenderRCore         frontend.Variable
	SenderRExt          frontend.Variable
	OutputRCore         frontend.Variable
	OutputRExt          frontend.Variable
	SenderCore          TransferComplianceCoreFields
	SenderExt           TransferComplianceExtFields
	OutputCore          TransferComplianceCoreFields
	OutputExt           TransferComplianceExtFields
}
