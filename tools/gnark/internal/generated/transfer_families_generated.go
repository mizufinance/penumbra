package generated

type TransferFamilySpec struct {
	Label              string
	ArtifactName       string
	NIn                int
	NOut               int
	BundledLibBasename string
}

var TransferFamilies = []TransferFamilySpec{
	{
		Label:              "transfer",
		ArtifactName:       "transfer",
		NIn:                2,
		NOut:               2,
		BundledLibBasename: "libpenumbra_gnark_transfer",
	},
}

func TransferFamilyByLabel(label string) (TransferFamilySpec, bool) {
	for _, family := range TransferFamilies {
		if family.Label == label {
			return family, true
		}
	}
	return TransferFamilySpec{}, false
}
