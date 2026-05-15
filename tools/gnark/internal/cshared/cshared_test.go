package cshared

import (
	"strings"
	"testing"
	"unsafe"
)

func TestSafeBytesRejectsOversizedInputBeforeCopying(t *testing.T) {
	var b byte
	ptr := unsafe.Pointer(&b)
	_, err := SafeBytes(ptr, MaxWitnessBytes+1, MaxWitnessBytes)
	if err == nil {
		t.Fatal("expected oversized input to fail")
	}
	if !strings.Contains(err.Error(), "exceeds max") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestSuccessRejectsOversizedResult(t *testing.T) {
	result := Success(make([]byte, MaxResultBytes+1), 12.5)
	if result.Status != 1 {
		t.Fatalf("expected failure status, got %d", result.Status)
	}
	if !strings.Contains(string(result.Payload), "proof result") {
		t.Fatalf("unexpected payload: %s", result.Payload)
	}
}

func TestProveRejectsOversizedWitnessBeforeCallback(t *testing.T) {
	registry := NewRegistry[int]()
	value := 7
	handle := registry.Add(&value)

	called := false
	var b byte
	result := Prove(
		registry,
		handle,
		unsafe.Pointer(&b),
		MaxWitnessBytes+1,
		func(_ *int, _ []byte) ([]byte, float64, error) {
			called = true
			return nil, 0, nil
		},
	)

	if called {
		t.Fatal("prove callback should not run for oversized witness")
	}
	if result.Status != 1 {
		t.Fatalf("expected failure status, got %d", result.Status)
	}
	if !strings.Contains(string(result.Payload), "read witness") {
		t.Fatalf("unexpected payload: %s", result.Payload)
	}
}

func TestRegistryAddGetDelete(t *testing.T) {
	registry := NewRegistry[int]()
	value := 7
	handle := registry.Add(&value)

	got, ok := registry.Get(handle)
	if !ok || got == nil || *got != value {
		t.Fatalf("unexpected registry lookup: %v %v", got, ok)
	}

	registry.Delete(handle)
	if _, ok := registry.Get(handle); ok {
		t.Fatal("expected deleted handle to be absent")
	}
}
