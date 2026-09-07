package main

/*
#include <stdint.h>
#include <stdlib.h>

typedef struct {
	uint64_t handle;
	double init_ms;
	void* err_ptr;
	size_t err_len;
} ShielddGnarkInitResult;

typedef struct {
	void* ptr;
	size_t len;
	uint32_t status;
	double prove_ms;
} ShielddGnarkBytesResult;
*/
import "C"

import (
	"fmt"
	"time"
	"unsafe"

	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/logger"

	"github.com/mizufinance/shieldd/tools/gnark/internal/abi"
	"github.com/mizufinance/shieldd/tools/gnark/internal/artifacts"
	"github.com/mizufinance/shieldd/tools/gnark/internal/circuits"
	"github.com/mizufinance/shieldd/tools/gnark/internal/cshared"
	"github.com/mizufinance/shieldd/tools/gnark/internal/generated"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

const noteReshapeProofResultMagic = "PNRP"

type proverContext struct {
	circuitName string
	familyID    uint32
	ccs         constraint.ConstraintSystem
	pk          *groth16bls.ProvingKey
}

var contexts = cshared.NewRegistry[proverContext]()

func compileNoteReshapeCircuit(family generated.NoteReshapeFamilySpec) (constraint.ConstraintSystem, error) {
	return frontend.Compile(
		primitives.ScalarField(),
		r1cs.NewBuilder,
		circuits.NewNoteReshapeCircuit(family.Label, family.NIn, family.NOut),
	)
}

func noteReshapeFamilyForCircuit(circuit string) (generated.NoteReshapeFamilySpec, error) {
	family, ok := generated.NoteReshapeFamilyByLabel(circuit)
	if !ok {
		return generated.NoteReshapeFamilySpec{}, fmt.Errorf("unsupported noteReshape circuit %q", circuit)
	}
	return family, nil
}

func packProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, _, err := abi.DecodeNoteReshapeWitness(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode noteReshape witness: %w", err)
	}
	return cshared.PackProofResult(noteReshapeProofResultMagic, witness.ClaimedStatementHash, proof, proveMS)
}

func initContext(circuit string, pk *groth16bls.ProvingKey, metadata *artifacts.CircuitMetadataJSON) (*proverContext, error) {
	family, err := noteReshapeFamilyForCircuit(circuit)
	if err != nil {
		return nil, err
	}
	ccs, err := compileNoteReshapeCircuit(family)
	if err != nil {
		return nil, fmt.Errorf("compile %s circuit: %w", family.Label, err)
	}
	if err := artifacts.ValidateCircuitMetadataForCircuit(metadata, family.Label, ccs); err != nil {
		return nil, err
	}
	return &proverContext{
		circuitName: family.Label,
		familyID:    family.ID,
		ccs:         ccs,
		pk:          pk,
	}, nil
}

//export shieldd_gnark_note_reshape_init
func shieldd_gnark_note_reshape_init(artifactDir *C.char, artifactDirLen C.size_t, out *C.ShielddGnarkInitResult) {
	if out == nil {
		return
	}
	logger.Disable()
	writeInitResult(out, cshared.InitFromDir(
		contexts,
		unsafe.Pointer(artifactDir),
		uint64(artifactDirLen),
		initContext,
	))
}

//export shieldd_gnark_note_reshape_init_from_bytes
func shieldd_gnark_note_reshape_init_from_bytes(
	pkData unsafe.Pointer,
	pkLen C.size_t,
	metadataData unsafe.Pointer,
	metadataLen C.size_t,
	out *C.ShielddGnarkInitResult,
) {
	if out == nil {
		return
	}
	logger.Disable()
	writeInitResult(out, cshared.InitFromBytes(
		contexts,
		pkData,
		uint64(pkLen),
		metadataData,
		uint64(metadataLen),
		"bundled noteReshape circuit_metadata.json",
		initContext,
	))
}

//export shieldd_gnark_note_reshape_prove
func shieldd_gnark_note_reshape_prove(handle C.uint64_t, witnessPtr unsafe.Pointer, witnessLen C.size_t, out *C.ShielddGnarkBytesResult) {
	if out == nil {
		return
	}
	logger.Disable()
	writeBytesResult(out, cshared.Prove(contexts, uint64(handle), witnessPtr, uint64(witnessLen), proveContext))
}

func proveContext(ctx *proverContext, witnessPayload []byte) ([]byte, float64, error) {
	assignment, family, err := abi.NewNoteReshapeCircuitAssignmentFromWitness(witnessPayload)
	if err != nil {
		return nil, 0, fmt.Errorf("decode witness: %w", err)
	}
	if family.ID != ctx.familyID {
		return nil, 0, fmt.Errorf(
			"noteReshape witness family mismatch: got %s (%d), expected %s (%d)",
			family.Label,
			family.ID,
			ctx.circuitName,
			ctx.familyID,
		)
	}

	fullWitness, err := frontend.NewWitness(assignment, primitives.ScalarField())
	if err != nil {
		return nil, 0, fmt.Errorf("construct gnark witness: %w", err)
	}

	start := time.Now()
	proofIface, err := groth16.Prove(ctx.ccs, ctx.pk, fullWitness)
	proveMS := time.Since(start).Seconds() * 1000
	if err != nil {
		return nil, proveMS, fmt.Errorf("prove %s: %w", ctx.circuitName, err)
	}
	proof, ok := proofIface.(*groth16bls.Proof)
	if !ok {
		return nil, proveMS, fmt.Errorf("unexpected proof type %T", proofIface)
	}

	payload, err := packProofResult(witnessPayload, proof, proveMS)
	if err != nil {
		return nil, proveMS, fmt.Errorf("pack proof result: %w", err)
	}
	return payload, proveMS, nil
}

//export shieldd_gnark_note_reshape_free
func shieldd_gnark_note_reshape_free(ptr unsafe.Pointer, _ C.size_t) {
	cshared.Free(ptr)
}

//export shieldd_gnark_note_reshape_shutdown
func shieldd_gnark_note_reshape_shutdown(handle C.uint64_t) {
	contexts.Delete(uint64(handle))
}

func writeInitResult(out *C.ShielddGnarkInitResult, result cshared.InitResult) {
	*out = C.ShielddGnarkInitResult{}
	out.handle = C.uint64_t(result.Handle)
	out.init_ms = C.double(result.InitMS)
	if len(result.Err) == 0 {
		return
	}
	ptr, n, err := cshared.AllocBytes(result.Err)
	if err != nil {
		ptr, n, _ = cshared.AllocBytes([]byte(err.Error()))
	}
	out.err_ptr = ptr
	out.err_len = C.size_t(n)
}

func writeBytesResult(out *C.ShielddGnarkBytesResult, result cshared.BytesResult) {
	*out = C.ShielddGnarkBytesResult{}
	out.status = C.uint32_t(result.Status)
	out.prove_ms = C.double(result.ProveMS)
	if len(result.Payload) == 0 {
		return
	}
	ptr, n, err := cshared.AllocBytes(result.Payload)
	if err != nil {
		result = cshared.Failure(err, result.ProveMS)
		out.status = C.uint32_t(result.Status)
		ptr, n, _ = cshared.AllocBytes(result.Payload)
	}
	out.ptr = ptr
	out.len = C.size_t(n)
}

func main() {}
