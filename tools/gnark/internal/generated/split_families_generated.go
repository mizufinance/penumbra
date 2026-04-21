package generated

type SplitFamilySpec struct {
	ID           uint32
	Label        string
	ArtifactName string
	NIn          int
	NOut         int
}

var SplitFamilies = []SplitFamilySpec{
	{ID: 1, Label: "split1x4", ArtifactName: "split1x4", NIn: 1, NOut: 4},
	{ID: 2, Label: "split1x8", ArtifactName: "split1x8", NIn: 1, NOut: 8},
}

func SplitFamilyByID(id uint32) (SplitFamilySpec, bool) {
	for _, family := range SplitFamilies {
		if family.ID == id {
			return family, true
		}
	}
	return SplitFamilySpec{}, false
}

func SplitFamilyByLabel(label string) (SplitFamilySpec, bool) {
	for _, family := range SplitFamilies {
		if family.Label == label {
			return family, true
		}
	}
	return SplitFamilySpec{}, false
}
