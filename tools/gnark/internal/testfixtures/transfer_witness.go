package testfixtures

import _ "embed"

//go:embed vectors/transfer_witness_v1.bin
var embeddedTransferWitnessV1 []byte

func LoadTransferWitnessV1(label string) []byte {
	switch label {
	case "transfer":
		return embeddedTransferWitnessV1
	default:
		panic("unknown transfer witness label: " + label)
	}
}
