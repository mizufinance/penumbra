package main

import (
	"errors"
	"flag"
	"fmt"
	"math/big"
	"os"
	"path/filepath"
	"time"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend/groth16"
	groth16bls "github.com/consensys/gnark/backend/groth16/bls12-377"
	groth16bw6 "github.com/consensys/gnark/backend/groth16/bw6-761"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/std/algebra/native/sw_bls12377"
	"github.com/consensys/gnark/std/math/emulated"
	stdgroth16 "github.com/consensys/gnark/std/recursion/groth16"
	"golang.org/x/crypto/blake2b"

	"github.com/mizufinance/shieldd/tools/gnark/internal/artifacts"
	"github.com/mizufinance/shieldd/tools/gnark/internal/compliance"
	"github.com/mizufinance/shieldd/tools/gnark/internal/primitives"
)

const (
	protocolVersion = 2
	treeDepth       = 20
)

var (
	leafDomain         = fqDomain("shieldd.nullifier.imt.leaf")
	historyEmptyDomain = fqDomain("shieldd.nullifier.history.empty")
	historyNodeDomain  = fqDomain("shieldd.nullifier.history.node")
)

type indexedLeaf struct {
	Value           frontend.Variable
	NextIndex       frontend.Variable
	NextValue       frontend.Variable
	IsLowerSentinel frontend.Variable
	IsTerminal      frontend.Variable
}

type innerCircuit struct {
	Nullifier               frontend.Variable
	Leaf                    indexedLeaf
	LeafPosition            frontend.Variable
	Path                    [treeDepth][3]frontend.Variable
	ProtocolVersion         frontend.Variable `gnark:",public"`
	PublicNullifier         frontend.Variable `gnark:",public"`
	GenerationIndex         frontend.Variable `gnark:",public"`
	GenerationRoot          frontend.Variable `gnark:",public"`
	GenerationStartPosition frontend.Variable `gnark:",public"`
	GenerationEndPosition   frontend.Variable `gnark:",public"`
	StartHistoryHead        frontend.Variable `gnark:",public"`
	EndHistoryHead          frontend.Variable `gnark:",public"`
}

type outerCircuit struct {
	Proofs           []stdgroth16.Proof[sw_bls12377.G1Affine, sw_bls12377.G2Affine]
	InnerWitnesses   []stdgroth16.Witness[sw_bls12377.ScalarField]
	VerifyingKey     stdgroth16.VerifyingKey[sw_bls12377.G1Affine, sw_bls12377.G2Affine, sw_bls12377.GT] `gnark:"-"`
	ProtocolVersion  frontend.Variable                                                                   `gnark:",public"`
	Nullifier        frontend.Variable                                                                   `gnark:",public"`
	ChunkIndex       frontend.Variable                                                                   `gnark:",public"`
	StartHistoryHead frontend.Variable                                                                   `gnark:",public"`
	EndHistoryHead   frontend.Variable                                                                   `gnark:",public"`
}

func (c *innerCircuit) Define(api frontend.API) error {
	api.AssertIsEqual(c.ProtocolVersion, protocolVersion)
	api.AssertIsEqual(c.PublicNullifier, c.Nullifier)
	api.ToBinary(c.GenerationIndex, 64)
	api.ToBinary(c.GenerationStartPosition, 48)
	api.ToBinary(c.GenerationEndPosition, 48)
	api.AssertIsLessOrEqual(c.GenerationStartPosition, c.GenerationEndPosition)
	api.AssertIsBoolean(c.Leaf.IsLowerSentinel)
	api.AssertIsBoolean(c.Leaf.IsTerminal)
	api.ToBinary(c.Leaf.NextIndex, treeDepth*2)
	api.AssertIsEqual(api.Mul(c.Leaf.IsLowerSentinel, c.Leaf.Value), 0)
	api.AssertIsEqual(c.Leaf.IsLowerSentinel, api.IsZero(c.LeafPosition))
	api.AssertIsEqual(api.Mul(c.Leaf.IsTerminal, c.Leaf.NextIndex), 0)
	api.AssertIsEqual(api.Mul(c.Leaf.IsTerminal, c.Leaf.NextValue), 0)
	api.AssertIsEqual(api.Mul(api.Sub(1, c.Leaf.IsTerminal), api.IsZero(c.Leaf.NextIndex)), 0)
	api.AssertIsEqual(
		api.Mul(api.Sub(1, c.Leaf.IsLowerSentinel), api.Sub(1, compliance.CanonicalFqLess(api, c.Leaf.Value, c.Nullifier))),
		0,
	)
	api.AssertIsEqual(
		api.Mul(api.Sub(1, c.Leaf.IsTerminal), api.Sub(1, compliance.CanonicalFqLess(api, c.Nullifier, c.Leaf.NextValue))),
		0,
	)
	leafHash, err := primitives.Poseidon377Hash5(api, leafDomain, [5]frontend.Variable{
		c.Leaf.Value,
		c.Leaf.NextIndex,
		c.Leaf.NextValue,
		c.Leaf.IsLowerSentinel,
		c.Leaf.IsTerminal,
	})
	if err != nil {
		return err
	}
	root, err := verifyQuadPath20(api, leafHash, c.Path, c.LeafPosition)
	if err != nil {
		return err
	}
	api.AssertIsEqual(root, c.GenerationRoot)
	historyHead, err := primitives.Poseidon377Hash5(api, historyNodeDomain, [5]frontend.Variable{
		c.StartHistoryHead,
		c.GenerationIndex,
		c.GenerationRoot,
		c.GenerationStartPosition,
		c.GenerationEndPosition,
	})
	if err != nil {
		return err
	}
	api.AssertIsEqual(historyHead, c.EndHistoryHead)
	return nil
}

func (c *outerCircuit) Define(api frontend.API) error {
	if len(c.Proofs) == 0 || len(c.Proofs) != len(c.InnerWitnesses) {
		return errors.New("outer proof and witness counts must be equal and nonzero")
	}
	verifier, err := stdgroth16.NewVerifier[
		sw_bls12377.ScalarField,
		sw_bls12377.G1Affine,
		sw_bls12377.G2Affine,
		sw_bls12377.GT,
	](api)
	if err != nil {
		return err
	}
	scalarField, err := emulated.NewField[sw_bls12377.ScalarField](api)
	if err != nil {
		return err
	}
	api.AssertIsEqual(c.ProtocolVersion, protocolVersion)
	api.ToBinary(c.ChunkIndex, 64)
	currentHead := c.StartHistoryHead
	for i := range c.Proofs {
		if len(c.InnerWitnesses[i].Public) != 8 {
			return fmt.Errorf("inner witness %d has %d public inputs, want 8", i, len(c.InnerWitnesses[i].Public))
		}
		if err := verifier.AssertProof(c.VerifyingKey, c.Proofs[i], c.InnerWitnesses[i], stdgroth16.WithCompleteArithmetic()); err != nil {
			return err
		}
		inner := c.InnerWitnesses[i].Public
		assertEmulatedEqualsNative(api, scalarField, &inner[0], c.ProtocolVersion, 32)
		assertEmulatedEqualsNative(api, scalarField, &inner[1], c.Nullifier, 253)
		expectedGeneration := api.Add(api.Mul(c.ChunkIndex, len(c.Proofs)), i)
		assertEmulatedEqualsNative(api, scalarField, &inner[2], expectedGeneration, 64)
		assertEmulatedEqualsNative(api, scalarField, &inner[6], currentHead, 253)
		currentHead = emulatedToNative(api, scalarField, &inner[7])
	}
	api.AssertIsEqual(currentHead, c.EndHistoryHead)
	return nil
}

func verifyQuadPath20(api frontend.API, leaf frontend.Variable, path [treeDepth][3]frontend.Variable, position frontend.Variable) (frontend.Variable, error) {
	current := leaf
	bits := api.ToBinary(position, treeDepth*2)
	for level := 0; level < treeDepth; level++ {
		bit0 := bits[level*2]
		bit1 := bits[level*2+1]
		is0 := api.Mul(api.Sub(1, bit0), api.Sub(1, bit1))
		is1 := api.Mul(bit0, api.Sub(1, bit1))
		is2 := api.Mul(api.Sub(1, bit0), bit1)
		is3 := api.Mul(bit0, bit1)
		child0 := api.Select(is0, current, path[level][0])
		child1 := api.Select(is1, current, api.Select(is0, path[level][0], path[level][1]))
		child2 := api.Select(is2, current, api.Select(bit1, path[level][2], path[level][1]))
		child3 := api.Select(is3, current, path[level][2])
		var err error
		current, err = primitives.Poseidon377Hash4(api, 0, [4]frontend.Variable{child0, child1, child2, child3})
		if err != nil {
			return nil, err
		}
	}
	return current, nil
}

func main() {
	batch := flag.Int("batch", 10, "number of BLS proofs verified in one BW6 proof")
	outDir := flag.String("out-dir", "", "artifact output directory")
	writeProvingKeys := flag.Bool("write-proving-keys", false, "write unsafe proving keys")
	flag.Parse()
	if *batch < 1 {
		fatal(errors.New("batch must be positive"))
	}
	if *batch != 10 && *outDir != "" {
		fatal(errors.New("protocol artifacts require batch 10"))
	}
	template, _, root, err := circuits(0, 0, 1<<32)
	if err != nil {
		fatal(err)
	}
	innerCCS, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, template)
	if err != nil {
		fatal(err)
	}
	innerPK, innerVK, err := groth16.Setup(innerCCS)
	if err != nil {
		fatal(err)
	}
	circuitVK, err := stdgroth16.ValueOfVerifyingKeyFixed[sw_bls12377.G1Affine, sw_bls12377.G2Affine, sw_bls12377.GT](innerVK)
	if err != nil {
		fatal(err)
	}
	outerTemplate := &outerCircuit{
		Proofs:         make([]stdgroth16.Proof[sw_bls12377.G1Affine, sw_bls12377.G2Affine], *batch),
		InnerWitnesses: make([]stdgroth16.Witness[sw_bls12377.ScalarField], *batch),
		VerifyingKey:   circuitVK,
	}
	startHead, err := primitives.Poseidon377Hash1Native(historyEmptyDomain, big.NewInt(0))
	if err != nil {
		fatal(err)
	}
	outerAssignment := &outerCircuit{
		Proofs:           make([]stdgroth16.Proof[sw_bls12377.G1Affine, sw_bls12377.G2Affine], *batch),
		InnerWitnesses:   make([]stdgroth16.Witness[sw_bls12377.ScalarField], *batch),
		VerifyingKey:     circuitVK,
		ProtocolVersion:  protocolVersion,
		Nullifier:        9,
		ChunkIndex:       0,
		StartHistoryHead: startHead,
	}
	currentHead := new(big.Int).Set(startHead)
	var firstInnerProof groth16.Proof
	var innerProve time.Duration
	for i := 0; i < *batch; i++ {
		outerTemplate.Proofs[i] = stdgroth16.PlaceholderProof[sw_bls12377.G1Affine, sw_bls12377.G2Affine](innerCCS)
		outerTemplate.InnerWitnesses[i] = stdgroth16.PlaceholderWitness[sw_bls12377.ScalarField](innerCCS)
		_, current, _, err := circuits(uint64(i), uint64(i)<<32, uint64(i+1)<<32)
		if err != nil {
			fatal(err)
		}
		witness, err := frontend.NewWitness(current, ecc.BLS12_377.ScalarField())
		if err != nil {
			fatal(err)
		}
		started := time.Now()
		proof, err := groth16.Prove(innerCCS, innerPK, witness, stdgroth16.GetNativeProverOptions(ecc.BW6_761.ScalarField(), ecc.BLS12_377.ScalarField()))
		if err != nil {
			fatal(err)
		}
		innerProve += time.Since(started)
		public, err := witness.Public()
		if err != nil {
			fatal(err)
		}
		if err := groth16.Verify(
			proof,
			innerVK,
			public,
			stdgroth16.GetNativeVerifierOptions(ecc.BW6_761.ScalarField(), ecc.BLS12_377.ScalarField()),
		); err != nil {
			fatal(fmt.Errorf("verify generated inner proof %d: %w", i, err))
		}
		if i == 0 {
			firstInnerProof = proof
		}
		outerAssignment.Proofs[i], err = stdgroth16.ValueOfProof[sw_bls12377.G1Affine, sw_bls12377.G2Affine](proof)
		if err != nil {
			fatal(err)
		}
		outerAssignment.InnerWitnesses[i], err = stdgroth16.ValueOfWitness[sw_bls12377.ScalarField](public)
		if err != nil {
			fatal(err)
		}
		currentHead, err = primitives.Poseidon377Hash5Native(historyNodeDomain, [5]*big.Int{
			currentHead,
			new(big.Int).SetUint64(uint64(i)),
			root,
			new(big.Int).SetUint64(uint64(i) << 32),
			new(big.Int).SetUint64(uint64(i+1) << 32),
		})
		if err != nil {
			fatal(err)
		}
	}
	outerAssignment.EndHistoryHead = currentHead
	outerCCS, err := frontend.Compile(ecc.BW6_761.ScalarField(), r1cs.NewBuilder, outerTemplate)
	if err != nil {
		fatal(err)
	}
	outerPK, outerVK, err := groth16.Setup(outerCCS)
	if err != nil {
		fatal(err)
	}
	outerWitness, err := frontend.NewWitness(outerAssignment, ecc.BW6_761.ScalarField())
	if err != nil {
		fatal(err)
	}
	outerProof, err := groth16.Prove(outerCCS, outerPK, outerWitness)
	if err != nil {
		fatal(err)
	}
	outerPublic, err := outerWitness.Public()
	if err != nil {
		fatal(err)
	}
	if err := groth16.Verify(outerProof, outerVK, outerPublic); err != nil {
		fatal(fmt.Errorf("verify generated outer proof: %w", err))
	}
	if *outDir != "" {
		if err := writeArtifacts(*outDir, innerPK, innerVK, firstInnerProof, outerPK, outerVK, outerProof, innerCCS.GetNbConstraints(), outerCCS.GetNbConstraints(), *writeProvingKeys, startHead, currentHead); err != nil {
			fatal(err)
		}
	}
	fmt.Printf("tree_depth=%d\nbatch=%d\ninner_constraints=%d\ninner_prove_mean_ms=%.3f\ninner_wire_proof_bytes=192\nouter_constraints=%d\nouter_wire_proof_bytes=480\n", treeDepth, *batch, innerCCS.GetNbConstraints(), float64(innerProve.Microseconds())/1000/float64(*batch), outerCCS.GetNbConstraints())
}

func circuits(generation, start, end uint64) (*innerCircuit, *innerCircuit, *big.Int, error) {
	zero := make([]*big.Int, treeDepth+1)
	zero[0] = big.NewInt(0)
	for level := 1; level <= treeDepth; level++ {
		var err error
		zero[level], err = primitives.Poseidon377Hash4Native(big.NewInt(0), [4]*big.Int{zero[level-1], zero[level-1], zero[level-1], zero[level-1]})
		if err != nil {
			return nil, nil, nil, err
		}
	}
	lowerHash, err := primitives.Poseidon377Hash5Native(leafDomain, [5]*big.Int{big.NewInt(0), big.NewInt(1), big.NewInt(7), big.NewInt(1), big.NewInt(0)})
	if err != nil {
		return nil, nil, nil, err
	}
	predecessorHash, err := primitives.Poseidon377Hash5Native(leafDomain, [5]*big.Int{big.NewInt(7), big.NewInt(0), big.NewInt(0), big.NewInt(0), big.NewInt(1)})
	if err != nil {
		return nil, nil, nil, err
	}
	root, err := primitives.Poseidon377Hash4Native(big.NewInt(0), [4]*big.Int{lowerHash, predecessorHash, zero[0], zero[0]})
	if err != nil {
		return nil, nil, nil, err
	}
	for level := 1; level < treeDepth; level++ {
		root, err = primitives.Poseidon377Hash4Native(big.NewInt(0), [4]*big.Int{root, zero[level], zero[level], zero[level]})
		if err != nil {
			return nil, nil, nil, err
		}
	}
	template := &innerCircuit{}
	assignment := &innerCircuit{
		Nullifier:               9,
		Leaf:                    indexedLeaf{Value: 7, NextIndex: 0, NextValue: 0, IsLowerSentinel: 0, IsTerminal: 1},
		LeafPosition:            1,
		ProtocolVersion:         protocolVersion,
		PublicNullifier:         9,
		GenerationIndex:         generation,
		GenerationRoot:          root,
		GenerationStartPosition: start,
		GenerationEndPosition:   end,
	}
	startHead, endHead, err := historyHeads(generation, root)
	if err != nil {
		return nil, nil, nil, err
	}
	assignment.StartHistoryHead = startHead
	assignment.EndHistoryHead = endHead
	assignment.Path[0] = [3]frontend.Variable{lowerHash, zero[0], zero[0]}
	for level := 1; level < treeDepth; level++ {
		assignment.Path[level] = [3]frontend.Variable{zero[level], zero[level], zero[level]}
	}
	return template, assignment, root, nil
}

func historyHeads(generation uint64, root *big.Int) (*big.Int, *big.Int, error) {
	current, err := primitives.Poseidon377Hash1Native(historyEmptyDomain, big.NewInt(0))
	if err != nil {
		return nil, nil, err
	}
	for index := uint64(0); index < generation; index++ {
		current, err = primitives.Poseidon377Hash5Native(historyNodeDomain, [5]*big.Int{
			current,
			new(big.Int).SetUint64(index),
			root,
			new(big.Int).SetUint64(index << 32),
			new(big.Int).SetUint64((index + 1) << 32),
		})
		if err != nil {
			return nil, nil, err
		}
	}
	start := new(big.Int).Set(current)
	end, err := primitives.Poseidon377Hash5Native(historyNodeDomain, [5]*big.Int{
		current,
		new(big.Int).SetUint64(generation),
		root,
		new(big.Int).SetUint64(generation << 32),
		new(big.Int).SetUint64((generation + 1) << 32),
	})
	return start, end, err
}

func fqDomain(label string) *big.Int {
	sum := blake2b.Sum512([]byte(label))
	value := new(big.Int).SetBytes(reverse(sum[:]))
	return value.Mod(value, ecc.BLS12_377.ScalarField())
}

func reverse(value []byte) []byte {
	out := append([]byte(nil), value...)
	for i, j := 0, len(out)-1; i < j; i, j = i+1, j-1 {
		out[i], out[j] = out[j], out[i]
	}
	return out
}

func assertEmulatedEqualsNative(api frontend.API, field *emulated.Field[sw_bls12377.ScalarField], actual *emulated.Element[sw_bls12377.ScalarField], expected frontend.Variable, bits int) {
	actualBits := field.ToBitsCanonical(actual)
	expectedBits := api.ToBinary(expected, bits)
	for i := 0; i < bits; i++ {
		api.AssertIsEqual(actualBits[i], expectedBits[i])
	}
	for i := bits; i < len(actualBits); i++ {
		api.AssertIsEqual(actualBits[i], 0)
	}
}

func emulatedToNative(api frontend.API, field *emulated.Field[sw_bls12377.ScalarField], value *emulated.Element[sw_bls12377.ScalarField]) frontend.Variable {
	return api.FromBinary(field.ToBitsCanonical(value)...)
}

type blsCommitmentKeyJSON struct {
	G         artifacts.G2PointJSON `json:"g"`
	GSigmaNeg artifacts.G2PointJSON `json:"g_sigma_neg"`
}
type bw6CommitmentKeyJSON struct {
	G         artifacts.G1PointJSON `json:"g"`
	GSigmaNeg artifacts.G1PointJSON `json:"g_sigma_neg"`
}
type blsVerifyingKeyJSON struct {
	artifacts.VerifyingKeyJSON
	CommitmentKeys               []blsCommitmentKeyJSON `json:"commitment_keys"`
	PublicAndCommitmentCommitted [][]int                `json:"public_and_commitment_committed"`
}
type bw6VerifyingKeyJSON struct {
	AlphaG1                      artifacts.G1PointJSON   `json:"alpha_g1"`
	BetaG2                       artifacts.G1PointJSON   `json:"beta_g2"`
	GammaG2                      artifacts.G1PointJSON   `json:"gamma_g2"`
	DeltaG2                      artifacts.G1PointJSON   `json:"delta_g2"`
	GammaABCG1                   []artifacts.G1PointJSON `json:"gamma_abc_g1"`
	CommitmentKeys               []bw6CommitmentKeyJSON  `json:"commitment_keys"`
	PublicAndCommitmentCommitted [][]int                 `json:"public_and_commitment_committed"`
}
type blsProofJSON struct {
	artifacts.ProofJSON
}
type bw6ProofJSON struct {
	A             artifacts.G1PointJSON `json:"a"`
	B             artifacts.G1PointJSON `json:"b"`
	C             artifacts.G1PointJSON `json:"c"`
	Commitment    artifacts.G1PointJSON `json:"commitment"`
	CommitmentPok artifacts.G1PointJSON `json:"commitment_pok"`
}

func writeArtifacts(dir string, innerPK groth16.ProvingKey, innerVK groth16.VerifyingKey, innerProof groth16.Proof, outerPK groth16.ProvingKey, outerVK groth16.VerifyingKey, outerProof groth16.Proof, innerConstraints, outerConstraints int, writePK bool, startHead, endHead *big.Int) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	blsVK := innerVK.(*groth16bls.VerifyingKey)
	bw6VK := outerVK.(*groth16bw6.VerifyingKey)
	blsProof := innerProof.(*groth16bls.Proof)
	bw6Proof := outerProof.(*groth16bw6.Proof)
	if err := artifacts.WriteJSON(filepath.Join(dir, "generation_verifying_key.json"), encodeBLSVK(blsVK)); err != nil {
		return err
	}
	if err := artifacts.WriteJSON(filepath.Join(dir, "chunk_verifying_key.json"), encodeBW6VK(bw6VK)); err != nil {
		return err
	}
	if err := artifacts.WriteJSON(filepath.Join(dir, "sample_generation_proof.json"), blsProofJSON{ProofJSON: artifacts.EncodeProofJSON(blsProof)}); err != nil {
		return err
	}
	if err := artifacts.WriteJSON(filepath.Join(dir, "sample_chunk_proof.json"), bw6ProofJSON{A: artifacts.G1PointJSON{X: bw6Proof.Ar.X.String(), Y: bw6Proof.Ar.Y.String()}, B: artifacts.G1PointJSON{X: bw6Proof.Bs.X.String(), Y: bw6Proof.Bs.Y.String()}, C: artifacts.G1PointJSON{X: bw6Proof.Krs.X.String(), Y: bw6Proof.Krs.Y.String()}, Commitment: artifacts.G1PointJSON{X: bw6Proof.Commitments[0].X.String(), Y: bw6Proof.Commitments[0].Y.String()}, CommitmentPok: artifacts.G1PointJSON{X: bw6Proof.CommitmentPok.X.String(), Y: bw6Proof.CommitmentPok.Y.String()}}); err != nil {
		return err
	}
	metadata := map[string]any{"schema": "shieldd.historical-proof.indexed", "unsafe_test_setup": true, "protocol_version": protocolVersion, "tree_depth": treeDepth, "chunk_width": 10, "inner_constraints": innerConstraints, "outer_constraints": outerConstraints, "generation_wire_proof_bytes": 192, "chunk_wire_proof_bytes": 480}
	if err := artifacts.WriteJSON(filepath.Join(dir, "metadata.json"), metadata); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dir, "generation_root.txt"), []byte(littleEndianHex(rootFromCircuit())+"\n"), 0o644); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dir, "start_history_head.txt"), []byte(littleEndianHex(startHead)+"\n"), 0o644); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dir, "end_history_head.txt"), []byte(littleEndianHex(endHead)+"\n"), 0o644); err != nil {
		return err
	}
	_, firstEnd, err := historyHeads(0, rootFromCircuit())
	if err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dir, "first_end_history_head.txt"), []byte(littleEndianHex(firstEnd)+"\n"), 0o644); err != nil {
		return err
	}
	if writePK {
		for name, key := range map[string]groth16.ProvingKey{"generation_proving_key.bin": innerPK, "chunk_proving_key.bin": outerPK} {
			file, err := os.Create(filepath.Join(dir, name))
			if err != nil {
				return err
			}
			if _, err = key.WriteTo(file); err != nil {
				return err
			}
			_ = file.Close()
		}
	}
	return nil
}

func rootFromCircuit() *big.Int {
	_, _, root, err := circuits(0, 0, 1<<32)
	if err != nil {
		panic(err)
	}
	return root
}

func littleEndianHex(value *big.Int) string {
	bytes := value.FillBytes(make([]byte, 32))
	return fmt.Sprintf("%x", reverse(bytes))
}

func encodeBLSVK(vk *groth16bls.VerifyingKey) blsVerifyingKeyJSON {
	keys := make([]blsCommitmentKeyJSON, len(vk.CommitmentKeys))
	for i, key := range vk.CommitmentKeys {
		keys[i] = blsCommitmentKeyJSON{G: artifacts.G2PointJSON{X: artifacts.Fq2JSON{A0: key.G.X.A0.String(), A1: key.G.X.A1.String()}, Y: artifacts.Fq2JSON{A0: key.G.Y.A0.String(), A1: key.G.Y.A1.String()}}, GSigmaNeg: artifacts.G2PointJSON{X: artifacts.Fq2JSON{A0: key.GSigmaNeg.X.A0.String(), A1: key.GSigmaNeg.X.A1.String()}, Y: artifacts.Fq2JSON{A0: key.GSigmaNeg.Y.A0.String(), A1: key.GSigmaNeg.Y.A1.String()}}}
	}
	return blsVerifyingKeyJSON{VerifyingKeyJSON: artifacts.EncodeVerifyingKeyJSON(vk), CommitmentKeys: keys, PublicAndCommitmentCommitted: vk.PublicAndCommitmentCommitted}
}

func encodeBW6VK(vk *groth16bw6.VerifyingKey) bw6VerifyingKeyJSON {
	points := make([]artifacts.G1PointJSON, len(vk.G1.K))
	for i, point := range vk.G1.K {
		points[i] = artifacts.G1PointJSON{X: point.X.String(), Y: point.Y.String()}
	}
	keys := make([]bw6CommitmentKeyJSON, len(vk.CommitmentKeys))
	for i, key := range vk.CommitmentKeys {
		keys[i] = bw6CommitmentKeyJSON{G: artifacts.G1PointJSON{X: key.G.X.String(), Y: key.G.Y.String()}, GSigmaNeg: artifacts.G1PointJSON{X: key.GSigmaNeg.X.String(), Y: key.GSigmaNeg.Y.String()}}
	}
	return bw6VerifyingKeyJSON{AlphaG1: artifacts.G1PointJSON{X: vk.G1.Alpha.X.String(), Y: vk.G1.Alpha.Y.String()}, BetaG2: artifacts.G1PointJSON{X: vk.G2.Beta.X.String(), Y: vk.G2.Beta.Y.String()}, GammaG2: artifacts.G1PointJSON{X: vk.G2.Gamma.X.String(), Y: vk.G2.Gamma.Y.String()}, DeltaG2: artifacts.G1PointJSON{X: vk.G2.Delta.X.String(), Y: vk.G2.Delta.Y.String()}, GammaABCG1: points, CommitmentKeys: keys, PublicAndCommitmentCommitted: vk.PublicAndCommitmentCommitted}
}

func fatal(err error) { fmt.Fprintln(os.Stderr, "historical proof:", err); os.Exit(1) }
