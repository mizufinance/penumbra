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
	"bytes"
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
	"unsafe"

	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/logger"

	"github.com/penumbra-zone/penumbra/tools/gnark/internal/abi"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/artifacts"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/circuits"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

const (
	spendProofResultMagic   = "PSPR"
	spendProofResultVersion = 1
)

type proverContext struct {
	ccs constraint.ConstraintSystem
	pk  *groth16bls.ProvingKey
}

var (
	contextMu  sync.RWMutex
	nextHandle uint64 = 1
	contexts          = make(map[uint64]*proverContext)
)

func setError(out *C.PenumbraGnarkInitResult, err error) {
	if out == nil || err == nil {
		return
	}
	bytes := []byte(err.Error())
	out.err_ptr = C.CBytes(bytes)
	out.err_len = C.size_t(len(bytes))
}

func setBytesResult(out *C.PenumbraGnarkBytesResult, status C.uint32_t, payload []byte, proveMS float64) {
	if out == nil {
		return
	}
	out.status = status
	out.prove_ms = C.double(proveMS)
	if len(payload) == 0 {
		out.ptr = nil
		out.len = 0
		return
	}
	out.ptr = C.CBytes(payload)
	out.len = C.size_t(len(payload))
}

func loadProvingKey(path string) (*groth16bls.ProvingKey, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	pk := new(groth16bls.ProvingKey)
	if _, err := pk.ReadFrom(file); err != nil {
		return nil, err
	}
	return pk, nil
}

func loadProvingKeyFromBytes(data []byte) (*groth16bls.ProvingKey, error) {
	pk := new(groth16bls.ProvingKey)
	if _, err := pk.ReadFrom(bytes.NewReader(data)); err != nil {
		return nil, err
	}
	return pk, nil
}

func packProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	buf := bytes.NewBuffer(make([]byte, 0, 4+4+4+4+8+32+384))
	buf.WriteString(spendProofResultMagic)
	_ = binary.Write(buf, binary.LittleEndian, uint32(spendProofResultVersion))
	_ = binary.Write(buf, binary.LittleEndian, uint32(0))
	_ = binary.Write(buf, binary.LittleEndian, uint32(0))
	_ = binary.Write(buf, binary.LittleEndian, uint64(proveMS*1000))

	witness, err := abi.DecodeSpendWitnessV1(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode spend witness: %w", err)
	}
	buf.Write(witness.ClaimedStatementHash[:])

	ax := proof.Ar.X.Bytes()
	ay := proof.Ar.Y.Bytes()
	bxa0 := proof.Bs.X.A0.Bytes()
	bxa1 := proof.Bs.X.A1.Bytes()
	bya0 := proof.Bs.Y.A0.Bytes()
	bya1 := proof.Bs.Y.A1.Bytes()
	cx := proof.Krs.X.Bytes()
	cy := proof.Krs.Y.Bytes()
	buf.Write(ax[:])
	buf.Write(ay[:])
	buf.Write(bxa0[:])
	buf.Write(bxa1[:])
	buf.Write(bya0[:])
	buf.Write(bya1[:])
	buf.Write(cx[:])
	buf.Write(cy[:])

	out := buf.Bytes()
	binary.LittleEndian.PutUint32(out[8:12], uint32(len(out)))
	return out, nil
}

//export penumbra_gnark_spend_init
func penumbra_gnark_spend_init(artifactDir *C.char, artifactDirLen C.size_t, out *C.PenumbraGnarkInitResult) {
	if out == nil {
		return
	}
	*out = C.PenumbraGnarkInitResult{}
	logger.Disable()

	dir := string(C.GoBytes(unsafe.Pointer(artifactDir), C.int(artifactDirLen)))
	start := time.Now()
	ccs, err := frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, &circuits.SpendCircuit{})
	if err != nil {
		setError(out, fmt.Errorf("compile spend circuit: %w", err))
		return
	}
	metadata, err := artifacts.LoadCircuitMetadata(dir)
	if err != nil {
		setError(out, fmt.Errorf("load circuit metadata: %w", err))
		return
	}
	if err := artifacts.ValidateCircuitMetadataForCircuit(metadata, "spend", ccs); err != nil {
		setError(out, err)
		return
	}
	pk, err := loadProvingKey(filepath.Join(dir, "proving_key.bin"))
	if err != nil {
		setError(out, fmt.Errorf("load proving key: %w", err))
		return
	}

	contextMu.Lock()
	handle := nextHandle
	nextHandle++
	contexts[handle] = &proverContext{ccs: ccs, pk: pk}
	contextMu.Unlock()

	out.handle = C.uint64_t(handle)
	out.init_ms = C.double(time.Since(start).Seconds() * 1000)
}

//export penumbra_gnark_spend_init_from_bytes
func penumbra_gnark_spend_init_from_bytes(
	pkData unsafe.Pointer,
	pkLen C.size_t,
	metadataData unsafe.Pointer,
	metadataLen C.size_t,
	out *C.PenumbraGnarkInitResult,
) {
	if out == nil {
		return
	}
	*out = C.PenumbraGnarkInitResult{}
	logger.Disable()

	start := time.Now()
	ccs, err := frontend.Compile(primitives.ScalarField(), r1cs.NewBuilder, &circuits.SpendCircuit{})
	if err != nil {
		setError(out, fmt.Errorf("compile spend circuit: %w", err))
		return
	}
	metadata, err := artifacts.LoadCircuitMetadataBytes(
		C.GoBytes(metadataData, C.int(metadataLen)),
		"bundled spend circuit_metadata.json",
	)
	if err != nil {
		setError(out, fmt.Errorf("load circuit metadata from bytes: %w", err))
		return
	}
	if err := artifacts.ValidateCircuitMetadataForCircuit(metadata, "spend", ccs); err != nil {
		setError(out, err)
		return
	}
	pk, err := loadProvingKeyFromBytes(C.GoBytes(pkData, C.int(pkLen)))
	if err != nil {
		setError(out, fmt.Errorf("load proving key from bytes: %w", err))
		return
	}

	contextMu.Lock()
	handle := nextHandle
	nextHandle++
	contexts[handle] = &proverContext{ccs: ccs, pk: pk}
	contextMu.Unlock()

	out.handle = C.uint64_t(handle)
	out.init_ms = C.double(time.Since(start).Seconds() * 1000)
}

//export penumbra_gnark_spend_prove
func penumbra_gnark_spend_prove(handle C.uint64_t, witnessPtr unsafe.Pointer, witnessLen C.size_t, out *C.PenumbraGnarkBytesResult) {
	if out == nil {
		return
	}
	*out = C.PenumbraGnarkBytesResult{}

	contextMu.RLock()
	ctx := contexts[uint64(handle)]
	if ctx == nil {
		contextMu.RUnlock()
		setBytesResult(out, 1, []byte("unknown prover handle"), 0)
		return
	}
	defer contextMu.RUnlock()

	witnessPayload := C.GoBytes(witnessPtr, C.int(witnessLen))
	assignment, err := abi.NewSpendCircuitAssignmentFromWitnessV1(witnessPayload)
	if err != nil {
		setBytesResult(out, 1, []byte(fmt.Sprintf("decode witness: %v", err)), 0)
		return
	}
	fullWitness, err := frontend.NewWitness(assignment, primitives.ScalarField())
	if err != nil {
		setBytesResult(out, 1, []byte(fmt.Sprintf("construct gnark witness: %v", err)), 0)
		return
	}

	start := time.Now()
	proofIface, err := groth16.Prove(ctx.ccs, ctx.pk, fullWitness)
	proveMS := time.Since(start).Seconds() * 1000
	if err != nil {
		setBytesResult(out, 1, []byte(fmt.Sprintf("prove spend: %v", err)), proveMS)
		return
	}
	proof, ok := proofIface.(*groth16bls.Proof)
	if !ok {
		setBytesResult(out, 1, []byte(fmt.Sprintf("unexpected proof type %T", proofIface)), proveMS)
		return
	}

	payload, err := packProofResult(witnessPayload, proof, proveMS)
	if err != nil {
		setBytesResult(out, 1, []byte(fmt.Sprintf("pack proof result: %v", err)), proveMS)
		return
	}
	setBytesResult(out, 0, payload, proveMS)
}

//export penumbra_gnark_spend_free
func penumbra_gnark_spend_free(ptr unsafe.Pointer, _ C.size_t) {
	if ptr != nil {
		C.free(ptr)
	}
}

//export penumbra_gnark_spend_shutdown
func penumbra_gnark_spend_shutdown(handle C.uint64_t) {
	contextMu.Lock()
	delete(contexts, uint64(handle))
	contextMu.Unlock()
}

func main() {}
