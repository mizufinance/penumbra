package generated

type ConsolidateFamilySpec struct {
	ID           uint32
	Label        string
	ArtifactName string
	NIn          int
	NOut         int
}

var ConsolidateFamilies = []ConsolidateFamilySpec{
	{ID: 1, Label: "consolidate2x1", ArtifactName: "consolidate2x1", NIn: 2, NOut: 1},
	{ID: 2, Label: "consolidate4x1", ArtifactName: "consolidate4x1", NIn: 4, NOut: 1},
	{ID: 3, Label: "consolidate8x1", ArtifactName: "consolidate8x1", NIn: 8, NOut: 1},
}

func ConsolidateFamilyByID(id uint32) (ConsolidateFamilySpec, bool) {
	for _, family := range ConsolidateFamilies {
		if family.ID == id {
			return family, true
		}
	}
	return ConsolidateFamilySpec{}, false
}

func ConsolidateFamilyByLabel(label string) (ConsolidateFamilySpec, bool) {
	for _, family := range ConsolidateFamilies {
		if family.Label == label {
			return family, true
		}
	}
	return ConsolidateFamilySpec{}, false
}
