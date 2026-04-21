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

	"github.com/mizufinance/penumbra/tools/gnark/internal/abi"
	"github.com/mizufinance/penumbra/tools/gnark/internal/artifacts"
	"github.com/mizufinance/penumbra/tools/gnark/internal/circuits"
	"github.com/mizufinance/penumbra/tools/gnark/internal/generated"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

const (
	shieldedIcs20WithdrawalProofResultMagic   = "PIPR"
	shieldedIcs20WithdrawalProofResultVersion = 1
)

type proverContext struct {
	circuitName string
	familyID    uint32
	ccs         constraint.ConstraintSystem
	pk          *groth16bls.ProvingKey
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

func safeGoBytes(ptr unsafe.Pointer, n C.size_t) ([]byte, error) {
	if ptr == nil && n != 0 {
		return nil, fmt.Errorf("nil pointer with non-zero length %d", uint64(n))
	}
	if uint64(n) > uint64(^uint32(0)>>1) {
		return nil, fmt.Errorf("byte slice length %d exceeds C.int max", uint64(n))
	}
	return C.GoBytes(ptr, C.int(n)), nil
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

func compileShieldedIcs20WithdrawalCircuit(family generated.ShieldedIcs20WithdrawalFamilySpec) (constraint.ConstraintSystem, error) {
	return frontend.Compile(
		primitives.ScalarField(),
		r1cs.NewBuilder,
		circuits.NewShieldedIcs20WithdrawalCircuit(family.NIn),
	)
}

func shieldedIcs20WithdrawalFamilyForCircuit(circuit string) (generated.ShieldedIcs20WithdrawalFamilySpec, error) {
	family, ok := generated.ShieldedIcs20WithdrawalFamilyByLabel(circuit)
	if !ok {
		return generated.ShieldedIcs20WithdrawalFamilySpec{}, fmt.Errorf("unsupported shielded ICS-20 withdrawal circuit %q", circuit)
	}
	return family, nil
}

func packProofResult(witnessPayload []byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	buf := bytes.NewBuffer(make([]byte, 0, 4+4+4+4+8+32+384))
	buf.WriteString(shieldedIcs20WithdrawalProofResultMagic)
	_ = binary.Write(buf, binary.LittleEndian, uint32(shieldedIcs20WithdrawalProofResultVersion))
	_ = binary.Write(buf, binary.LittleEndian, uint32(0))
	_ = binary.Write(buf, binary.LittleEndian, uint32(0))
	_ = binary.Write(buf, binary.LittleEndian, uint64(proveMS*1000))

	witness, _, err := abi.DecodeShieldedIcs20WithdrawalWitnessV1(witnessPayload)
	if err != nil {
		return nil, fmt.Errorf("decode shielded ICS-20 withdrawal witness: %w", err)
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

func initContext(circuit string, pk *groth16bls.ProvingKey, metadata *artifacts.CircuitMetadataJSON) (*proverContext, error) {
	family, err := shieldedIcs20WithdrawalFamilyForCircuit(circuit)
	if err != nil {
		return nil, err
	}
	ccs, err := compileShieldedIcs20WithdrawalCircuit(family)
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

//export penumbra_gnark_shielded_ics20_withdrawal_init
func penumbra_gnark_shielded_ics20_withdrawal_init(artifactDir *C.char, artifactDirLen C.size_t, out *C.PenumbraGnarkInitResult) {
	if out == nil {
		return
	}
	*out = C.PenumbraGnarkInitResult{}
	logger.Disable()

	dirBytes, err := safeGoBytes(unsafe.Pointer(artifactDir), artifactDirLen)
	if err != nil {
		setError(out, fmt.Errorf("read artifact dir: %w", err))
		return
	}
	dir := string(dirBytes)
	start := time.Now()
	metadata, err := artifacts.LoadCircuitMetadata(dir)
	if err != nil {
		setError(out, fmt.Errorf("load circuit metadata: %w", err))
		return
	}
	pk, err := loadProvingKey(filepath.Join(dir, "proving_key.bin"))
	if err != nil {
		setError(out, fmt.Errorf("load proving key: %w", err))
		return
	}
	ctx, err := initContext(metadata.Circuit, pk, metadata)
	if err != nil {
		setError(out, err)
		return
	}

	contextMu.Lock()
	handle := nextHandle
	nextHandle++
	contexts[handle] = ctx
	contextMu.Unlock()

	out.handle = C.uint64_t(handle)
	out.init_ms = C.double(time.Since(start).Seconds() * 1000)
}

//export penumbra_gnark_shielded_ics20_withdrawal_init_from_bytes
func penumbra_gnark_shielded_ics20_withdrawal_init_from_bytes(
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
	metadataBytes, err := safeGoBytes(metadataData, metadataLen)
	if err != nil {
		setError(out, fmt.Errorf("read metadata bytes: %w", err))
		return
	}
	metadata, err := artifacts.LoadCircuitMetadataBytes(
		metadataBytes,
		"bundled shielded ICS-20 withdrawal circuit_metadata.json",
	)
	if err != nil {
		setError(out, fmt.Errorf("load circuit metadata from bytes: %w", err))
		return
	}
	pkBytes, err := safeGoBytes(pkData, pkLen)
	if err != nil {
		setError(out, fmt.Errorf("read proving key bytes: %w", err))
		return
	}
	pk, err := loadProvingKeyFromBytes(pkBytes)
	if err != nil {
		setError(out, fmt.Errorf("load proving key from bytes: %w", err))
		return
	}
	ctx, err := initContext(metadata.Circuit, pk, metadata)
	if err != nil {
		setError(out, err)
		return
	}

	contextMu.Lock()
	handle := nextHandle
	nextHandle++
	contexts[handle] = ctx
	contextMu.Unlock()

	out.handle = C.uint64_t(handle)
	out.init_ms = C.double(time.Since(start).Seconds() * 1000)
}

//export penumbra_gnark_shielded_ics20_withdrawal_prove
func penumbra_gnark_shielded_ics20_withdrawal_prove(handle C.uint64_t, witnessData unsafe.Pointer, witnessLen C.size_t, out *C.PenumbraGnarkBytesResult) {
	if out == nil {
		return
	}
	*out = C.PenumbraGnarkBytesResult{}
	logger.Disable()

	witnessPayload, err := safeGoBytes(witnessData, witnessLen)
	if err != nil {
		setBytesResult(out, 1, []byte(fmt.Sprintf("read witness bytes: %v", err)), 0)
		return
	}

	contextMu.RLock()
	ctx, ok := contexts[uint64(handle)]
	contextMu.RUnlock()
	if !ok {
		setBytesResult(out, 1, []byte(fmt.Sprintf("unknown prover handle %d", uint64(handle))), 0)
		return
	}

	assignment, _, err := abi.NewShieldedIcs20WithdrawalCircuitAssignmentFromWitnessV1(witnessPayload)
	if err != nil {
		setBytesResult(out, 1, []byte(fmt.Sprintf("decode shielded ICS-20 withdrawal witness: %v", err)), 0)
		return
	}
	fullWitness, err := frontend.NewWitness(assignment, primitives.ScalarField())
	if err != nil {
		setBytesResult(out, 1, []byte(fmt.Sprintf("construct shielded ICS-20 withdrawal witness: %v", err)), 0)
		return
	}

	proveStart := time.Now()
	proofIface, err := groth16.Prove(ctx.ccs, ctx.pk, fullWitness)
	if err != nil {
		setBytesResult(out, 1, []byte(fmt.Sprintf("prove shielded ICS-20 withdrawal: %v", err)), 0)
		return
	}
	proof, ok := proofIface.(*groth16bls.Proof)
	if !ok {
		setBytesResult(out, 1, []byte(fmt.Sprintf("unexpected shielded ICS-20 withdrawal proof type %T", proofIface)), 0)
		return
	}
	payload, err := packProofResult(witnessPayload, proof, time.Since(proveStart).Seconds()*1000)
	if err != nil {
		setBytesResult(out, 1, []byte(err.Error()), 0)
		return
	}
	setBytesResult(out, 0, payload, time.Since(proveStart).Seconds()*1000)
}

//export penumbra_gnark_shielded_ics20_withdrawal_free
func penumbra_gnark_shielded_ics20_withdrawal_free(ptr unsafe.Pointer, len C.size_t) {
	if ptr == nil {
		return
	}
	C.free(ptr)
}

//export penumbra_gnark_shielded_ics20_withdrawal_shutdown
func penumbra_gnark_shielded_ics20_withdrawal_shutdown(handle C.uint64_t) {
	contextMu.Lock()
	delete(contexts, uint64(handle))
	contextMu.Unlock()
}

func main() {}
