package main

/*
#include <stdint.h>
#include <stdlib.h>

typedef struct {
	uint64_t handle;
	double init_ms;
	void* err_ptr;
	size_t err_len;
} PenumbraGnarkInitResult;

typedef struct {
	void* ptr;
	size_t len;
	uint32_t status;
	double prove_ms;
} PenumbraGnarkBytesResult;
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

	"github.com/mizufinance/penumbra/tools/gnark/internal/abi"
	"github.com/mizufinance/penumbra/tools/gnark/internal/artifacts"
	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/cshared"
	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

const transferProofResultMagic = "PTPR"

type proverContext struct {
	circuitName string
	ccs         constraint.ConstraintSystem
	pk          *groth16bls.ProvingKey
}

var contexts = cshared.NewRegistry[proverContext]()

func compileTransferCircuit() (constraint.ConstraintSystem, error) {
	return frontend.Compile(
		primitives.ScalarField(),
		r1cs.NewBuilder,
		circuits.NewTransferCircuit(),
	)
}

func transferFamilyForCircuit(circuit string) (generated.TransferFamilySpec, error) {
	family, ok := generated.TransferFamilyByLabel(circuit)
	if !ok {
		return generated.TransferFamilySpec{}, fmt.Errorf("unsupported transfer circuit %q", circuit)
	}
	return family, nil
}

func packProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	witness, _, err := abi.DecodeTransferWitnessV1(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode transfer witness: %w", err)
	}
	return cshared.PackProofResult(transferProofResultMagic, witness.ClaimedStatementHash, proof, proveMS)
}

func initContext(circuit string, pk *groth16bls.ProvingKey, metadata *artifacts.CircuitMetadataJSON) (*proverContext, error) {
	family, err := transferFamilyForCircuit(circuit)
	if err != nil {
		return nil, err
	}
	ccs, err := compileTransferCircuit()
	if err != nil {
		return nil, fmt.Errorf("compile %s circuit: %w", family.Label, err)
	}
	if err := artifacts.ValidateCircuitMetadataForCircuit(metadata, family.Label, ccs); err != nil {
		return nil, err
	}
	return &proverContext{
		circuitName: family.Label,
		ccs:         ccs,
		pk:          pk,
	}, nil
}

//export penumbra_gnark_transfer_init
func penumbra_gnark_transfer_init(artifactDir *C.char, artifactDirLen C.size_t, out *C.PenumbraGnarkInitResult) {
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

//export penumbra_gnark_transfer_init_from_bytes
func penumbra_gnark_transfer_init_from_bytes(
	pkData unsafe.Pointer,
	pkLen C.size_t,
	metadataData unsafe.Pointer,
	metadataLen C.size_t,
	out *C.PenumbraGnarkInitResult,
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
		"bundled transfer circuit_metadata.json",
		initContext,
	))
}

//export penumbra_gnark_transfer_prove
func penumbra_gnark_transfer_prove(handle C.uint64_t, witnessPtr unsafe.Pointer, witnessLen C.size_t, out *C.PenumbraGnarkBytesResult) {
	if out == nil {
		return
	}
	logger.Disable()
	writeBytesResult(out, cshared.Prove(contexts, uint64(handle), witnessPtr, uint64(witnessLen), proveContext))
}

func proveContext(ctx *proverContext, witnessPayload []byte) ([]byte, float64, error) {
	assignment, family, err := abi.NewTransferCircuitAssignmentFromWitnessV1(witnessPayload)
	if err != nil {
		return nil, 0, fmt.Errorf("decode witness: %w", err)
	}
	if family.Label != ctx.circuitName {
		return nil, 0, fmt.Errorf(
			"transfer witness circuit mismatch: got %s, expected %s",
			family.Label,
			ctx.circuitName,
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

//export penumbra_gnark_transfer_free
func penumbra_gnark_transfer_free(ptr unsafe.Pointer, _ C.size_t) {
	cshared.Free(ptr)
}

//export penumbra_gnark_transfer_shutdown
func penumbra_gnark_transfer_shutdown(handle C.uint64_t) {
	contexts.Delete(uint64(handle))
}

func writeInitResult(out *C.PenumbraGnarkInitResult, result cshared.InitResult) {
	*out = C.PenumbraGnarkInitResult{}
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

func writeBytesResult(out *C.PenumbraGnarkBytesResult, result cshared.BytesResult) {
	*out = C.PenumbraGnarkBytesResult{}
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
