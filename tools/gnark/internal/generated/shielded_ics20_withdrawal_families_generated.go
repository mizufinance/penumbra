package generated

type ShieldedIcs20WithdrawalFamilySpec struct {
	ID           uint32
	Label        string
	ArtifactName string
	NIn          int
}

var ShieldedIcs20WithdrawalFamilies = []ShieldedIcs20WithdrawalFamilySpec{
	{ID: 1, Label: "shielded_ics20_withdrawal", ArtifactName: "shielded_ics20_withdrawal", NIn: 2},
}

func ShieldedIcs20WithdrawalFamilyByID(id uint32) (ShieldedIcs20WithdrawalFamilySpec, bool) {
	for _, family := range ShieldedIcs20WithdrawalFamilies {
		if family.ID == id {
			return family, true
		}
	}
	return ShieldedIcs20WithdrawalFamilySpec{}, false
}

func ShieldedIcs20WithdrawalFamilyByLabel(label string) (ShieldedIcs20WithdrawalFamilySpec, bool) {
	for _, family := range ShieldedIcs20WithdrawalFamilies {
		if family.Label == label {
			return family, true
		}
	}
	return ShieldedIcs20WithdrawalFamilySpec{}, false
}
