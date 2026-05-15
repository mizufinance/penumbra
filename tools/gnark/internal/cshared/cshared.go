package cshared

/*
#include <stdlib.h>
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

	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"

	"github.com/mizufinance/penumbra/tools/gnark/internal/artifacts"
)

const (
	MaxWitnessBytes = 4 * 1024 * 1024
	MaxResultBytes  = 1024 * 1024
	MaxInitBytes    = 256 * 1024 * 1024
)

type InitResult struct {
	Handle uint64
	InitMS float64
	Err    []byte
}

type BytesResult struct {
	Status  uint32
	Payload []byte
	ProveMS float64
}

type Registry[T any] struct {
	mu      sync.RWMutex
	next    uint64
	entries map[uint64]*T
}

func NewRegistry[T any]() *Registry[T] {
	return &Registry[T]{
		next:    1,
		entries: make(map[uint64]*T),
	}
}

func (r *Registry[T]) Add(ctx *T) uint64 {
	r.mu.Lock()
	defer r.mu.Unlock()

	handle := r.next
	r.next++
	r.entries[handle] = ctx
	return handle
}

func (r *Registry[T]) Get(handle uint64) (*T, bool) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	ctx, ok := r.entries[handle]
	return ctx, ok
}

func (r *Registry[T]) Delete(handle uint64) {
	r.mu.Lock()
	defer r.mu.Unlock()

	delete(r.entries, handle)
}

func SafeBytes(ptr unsafe.Pointer, n uint64, max uint64) ([]byte, error) {
	if ptr == nil && n != 0 {
		return nil, fmt.Errorf("nil pointer with non-zero length %d", n)
	}
	if n > max {
		return nil, fmt.Errorf("byte slice length %d exceeds max %d", n, max)
	}
	if n == 0 {
		return nil, nil
	}
	if n > uint64(int(^uint(0)>>1)) {
		return nil, fmt.Errorf("byte slice length %d exceeds int max", n)
	}

	out := make([]byte, int(n))
	copy(out, unsafe.Slice((*byte)(ptr), int(n)))
	return out, nil
}

func AllocBytes(payload []byte) (unsafe.Pointer, uint64, error) {
	if len(payload) > MaxResultBytes {
		return nil, 0, fmt.Errorf("result payload %d bytes exceeds max %d", len(payload), MaxResultBytes)
	}
	if len(payload) == 0 {
		return nil, 0, nil
	}
	return C.CBytes(payload), uint64(len(payload)), nil
}

func Free(ptr unsafe.Pointer) {
	if ptr != nil {
		C.free(ptr)
	}
}

type InitContextFunc[T any] func(circuit string, pk *groth16bls.ProvingKey, metadata *artifacts.CircuitMetadataJSON) (*T, error)

func InitFromDir[T any](
	registry *Registry[T],
	artifactDirPtr unsafe.Pointer,
	artifactDirLen uint64,
	initContext InitContextFunc[T],
) InitResult {
	dirBytes, err := SafeBytes(artifactDirPtr, artifactDirLen, MaxInitBytes)
	if err != nil {
		return initError(fmt.Errorf("read artifact dir: %w", err))
	}
	dir := string(dirBytes)

	start := time.Now()
	metadata, err := artifacts.LoadCircuitMetadata(dir)
	if err != nil {
		return initError(fmt.Errorf("load circuit metadata: %w", err))
	}
	pk, err := loadProvingKey(filepath.Join(dir, "proving_key.bin"))
	if err != nil {
		return initError(fmt.Errorf("load proving key: %w", err))
	}
	ctx, err := initContext(metadata.Circuit, pk, metadata)
	if err != nil {
		return initError(err)
	}

	return InitResult{
		Handle: registry.Add(ctx),
		InitMS: time.Since(start).Seconds() * 1000,
	}
}

func InitFromBytes[T any](
	registry *Registry[T],
	pkData unsafe.Pointer,
	pkLen uint64,
	metadataData unsafe.Pointer,
	metadataLen uint64,
	metadataLabel string,
	initContext InitContextFunc[T],
) InitResult {
	start := time.Now()
	metadataBytes, err := SafeBytes(metadataData, metadataLen, MaxInitBytes)
	if err != nil {
		return initError(fmt.Errorf("read metadata bytes: %w", err))
	}
	metadata, err := artifacts.LoadCircuitMetadataBytes(metadataBytes, metadataLabel)
	if err != nil {
		return initError(fmt.Errorf("load circuit metadata from bytes: %w", err))
	}
	pkBytes, err := SafeBytes(pkData, pkLen, MaxInitBytes)
	if err != nil {
		return initError(fmt.Errorf("read proving key bytes: %w", err))
	}
	pk, err := loadProvingKeyFromBytes(pkBytes)
	if err != nil {
		return initError(fmt.Errorf("load proving key from bytes: %w", err))
	}
	ctx, err := initContext(metadata.Circuit, pk, metadata)
	if err != nil {
		return initError(err)
	}

	return InitResult{
		Handle: registry.Add(ctx),
		InitMS: time.Since(start).Seconds() * 1000,
	}
}

type ProveFunc[T any] func(ctx *T, witnessPayload []byte) ([]byte, float64, error)

func Prove[T any](
	registry *Registry[T],
	handle uint64,
	witnessPtr unsafe.Pointer,
	witnessLen uint64,
	prove ProveFunc[T],
) BytesResult {
	ctx, ok := registry.Get(handle)
	if !ok {
		return Failure(fmt.Errorf("unknown prover handle"), 0)
	}
	witnessPayload, err := SafeBytes(witnessPtr, witnessLen, MaxWitnessBytes)
	if err != nil {
		return Failure(fmt.Errorf("read witness: %w", err), 0)
	}

	payload, proveMS, err := prove(ctx, witnessPayload)
	if err != nil {
		return Failure(err, proveMS)
	}
	return Success(payload, proveMS)
}

func Success(payload []byte, proveMS float64) BytesResult {
	if len(payload) > MaxResultBytes {
		return Failure(
			fmt.Errorf("proof result %d bytes exceeds max %d", len(payload), MaxResultBytes),
			proveMS,
		)
	}
	return BytesResult{Status: 0, Payload: payload, ProveMS: proveMS}
}

func Failure(err error, proveMS float64) BytesResult {
	payload := []byte(err.Error())
	if len(payload) > MaxResultBytes {
		suffix := []byte("... truncated")
		keep := MaxResultBytes - len(suffix)
		payload = append(append([]byte{}, payload[:keep]...), suffix...)
	}
	return BytesResult{Status: 1, Payload: payload, ProveMS: proveMS}
}

func PackProofResult(magic string, claimedStatementHash [32]byte, proof *groth16bls.Proof, proveMS float64) ([]byte, error) {
	buf := bytes.NewBuffer(make([]byte, 0, 4+4+4+4+8+32+384))
	buf.WriteString(magic)
	_ = binary.Write(buf, binary.LittleEndian, uint32(1))
	_ = binary.Write(buf, binary.LittleEndian, uint32(0))
	_ = binary.Write(buf, binary.LittleEndian, uint32(0))
	_ = binary.Write(buf, binary.LittleEndian, uint64(proveMS*1000))
	buf.Write(claimedStatementHash[:])

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

func initError(err error) InitResult {
	return InitResult{Err: Failure(err, 0).Payload}
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
