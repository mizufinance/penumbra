// Code generated from shielded_withdrawal_families.json. DO NOT EDIT.
package generated

type ShieldedWithdrawalFamilySpec struct {
	ID                 uint32
	Label              string
	ArtifactName       string
	NIn                int
	NOut               int
	BundledLibBasename string
}

var ShieldedWithdrawalFamilies = []ShieldedWithdrawalFamilySpec{
	{
		ID:                 1,
		Label:              "shielded_withdrawal",
		ArtifactName:       "shielded_withdrawal",
		NIn:                2,
		NOut:               1,
		BundledLibBasename: "libshieldd_gnark_shielded_withdrawal",
	},
}

func ShieldedWithdrawalFamilyByID(id uint32) (ShieldedWithdrawalFamilySpec, bool) {
	for _, family := range ShieldedWithdrawalFamilies {
		if family.ID == id {
			return family, true
		}
	}
	return ShieldedWithdrawalFamilySpec{}, false
}

func ShieldedWithdrawalFamilyByLabel(label string) (ShieldedWithdrawalFamilySpec, bool) {
	for _, family := range ShieldedWithdrawalFamilies {
		if family.Label == label {
			return family, true
		}
	}
	return ShieldedWithdrawalFamilySpec{}, false
}
