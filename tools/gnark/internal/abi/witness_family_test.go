package abi

import (
	"os"
	"testing"

	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type witnessFamily struct {
	name    string
	payload func(t *testing.T) []byte
	decode  func([]byte) error
}

func testWitnessFamilies() []witnessFamily {
	return []witnessFamily{
		{
			name: "spend",
			payload: func(t *testing.T) []byte {
				t.Helper()
				return primitives.LoadSpendWitnessV1()
			},
			decode: func(payload []byte) error {
				_, err := DecodeSpendWitnessV1(payload)
				return err
			},
		},
		{
			name: "output",
			payload: func(t *testing.T) []byte {
				t.Helper()
				return primitives.LoadOutputWitnessV1()
			},
			decode: func(payload []byte) error {
				_, err := DecodeOutputWitnessV1(payload)
				return err
			},
		},
		{
			name: "transfer1x1",
			payload: func(t *testing.T) []byte {
				t.Helper()
				payload, err := os.ReadFile("../../testdata/transfer1x1_witness_v1.bin")
				if err != nil {
					t.Skipf("transfer1x1 witness fixture not found: %v", err)
				}
				return payload
			},
			decode: func(payload []byte) error {
				_, _, err := DecodeTransferWitnessV1(payload)
				return err
			},
		},
	}
}

func TestWitnessFamiliesDecode(t *testing.T) {
	for _, family := range testWitnessFamilies() {
		t.Run(family.name, func(t *testing.T) {
			if err := family.decode(family.payload(t)); err != nil {
				t.Fatalf("decode %s witness: %v", family.name, err)
			}
		})
	}
}

func TestWitnessFamiliesRejectBadHeader(t *testing.T) {
	for _, family := range testWitnessFamilies() {
		t.Run(family.name, func(t *testing.T) {
			payload := append([]byte(nil), family.payload(t)...)
			payload[0] ^= 0xff
			if err := family.decode(payload); err == nil {
				t.Fatalf("expected %s witness to reject mutated header", family.name)
			}
		})
	}
}

func TestWitnessFamiliesRejectTruncatedPayload(t *testing.T) {
	for _, family := range testWitnessFamilies() {
		t.Run(family.name, func(t *testing.T) {
			payload := append([]byte(nil), family.payload(t)...)
			payload = payload[:len(payload)-1]
			if err := family.decode(payload); err == nil {
				t.Fatalf("expected %s witness to reject truncated payload", family.name)
			}
		})
	}
}
