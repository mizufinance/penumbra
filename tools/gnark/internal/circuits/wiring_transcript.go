package circuits

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"reflect"
	"sort"
	"strings"
	"unsafe"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	"github.com/consensys/gnark/frontend/schema"
)

const wiringTranscriptSchema = "shieldd.gnark.wiring"

// WiringTranscript records the semantic call graph emitted by a circuit Define.
type WiringTranscript struct {
	circuit           string
	nIn               int
	nOut              int
	recordCounts      bool
	constraintCounter func() (int, bool)
	compiler          frontend.Compiler
	events            []wiringEvent
	bindings          []semanticBinding
	bindingNames      map[string]struct{}
}

type wiringEvent struct {
	op         string
	args       []string
	constraint int
}

func newWiringTranscript(circuit string, nIn, nOut int) *WiringTranscript {
	return &WiringTranscript{
		circuit: circuit, nIn: nIn, nOut: nOut,
		bindingNames: make(map[string]struct{}),
	}
}

type semanticBinding struct {
	name        string
	expressions []constraint.LinearExpression
}

func (t *WiringTranscript) record(op string, args ...string) {
	if t == nil {
		return
	}
	constraint := -1
	if t.recordCounts && t.constraintCounter != nil {
		if count, ok := t.constraintCounter(); ok {
			constraint = count
		}
	}
	t.events = append(t.events, wiringEvent{op: op, args: args, constraint: constraint})
}

func (t *WiringTranscript) bindCompiler(compiler frontend.Compiler) {
	if t == nil {
		return
	}
	t.compiler = compiler
	if !t.recordCounts {
		return
	}
	t.constraintCounter = func() (int, bool) {
		return currentConstraintCount(compiler)
	}
}

func (t *WiringTranscript) bind(name string, variables ...frontend.Variable) {
	if t == nil {
		return
	}
	if t.compiler == nil {
		panic("semantic binding recorded before compiler attachment")
	}
	if name == "" || strings.ContainsAny(name, " \t\r\n") {
		panic(fmt.Sprintf("invalid semantic binding name %q", name))
	}
	if _, exists := t.bindingNames[name]; exists {
		panic(fmt.Sprintf("duplicate semantic binding %q", name))
	}
	expressions := make([]constraint.LinearExpression, len(variables))
	for index, variable := range variables {
		canonical := t.compiler.ToCanonicalVariable(variable)
		expression, ok := canonical.(constraint.LinearExpression)
		if !ok {
			panic(fmt.Sprintf(
				"semantic binding %q expression %d has unsupported canonical type %T",
				name, index, canonical,
			))
		}
		expressions[index] = expression.Clone()
	}
	t.bindingNames[name] = struct{}{}
	t.bindings = append(t.bindings, semanticBinding{name: name, expressions: expressions})
}

func (t *WiringTranscript) canonical() (string, error) {
	var b strings.Builder
	fmt.Fprintf(&b, "schema %s\n", wiringTranscriptSchema)
	fmt.Fprintf(&b, "circuit %s\n", t.circuit)
	fmt.Fprintf(&b, "shape n_in=%d n_out=%d\n", t.nIn, t.nOut)
	for i, event := range t.events {
		if strings.ContainsAny(event.op, " \t\r\n") {
			return "", fmt.Errorf("wiring transcript op contains whitespace: %q", event.op)
		}
		fmt.Fprintf(&b, "%04d %s", i+1, event.op)
		for _, arg := range event.args {
			if strings.ContainsAny(arg, " \t\r\n") {
				return "", fmt.Errorf("wiring transcript arg contains whitespace: %q", arg)
			}
			b.WriteByte(' ')
			b.WriteString(arg)
		}
		b.WriteByte('\n')
	}
	return b.String(), nil
}

func (c *NoteReshapeCircuit) traceWiring(op string, args ...string) {
	if c.wiringTrace != nil {
		c.wiringTrace.record(op, args...)
	}
}

func (c *NoteReshapeCircuit) bindSemantic(name string, variables ...frontend.Variable) {
	if c.wiringTrace != nil {
		c.wiringTrace.bind(name, variables...)
	}
}

func (c *TransferCircuit) traceWiring(op string, args ...string) {
	if c.wiringTrace != nil {
		c.wiringTrace.record(op, args...)
	}
}

func (c *TransferCircuit) bindSemantic(name string, variables ...frontend.Variable) {
	if c.wiringTrace != nil {
		c.wiringTrace.bind(name, variables...)
	}
}

func (c *ShieldedIcs20WithdrawalCircuit) traceWiring(op string, args ...string) {
	if c.wiringTrace != nil {
		c.wiringTrace.record(op, args...)
	}
}

func (c *ShieldedIcs20WithdrawalCircuit) bindSemantic(
	name string,
	variables ...frontend.Variable,
) {
	if c.wiringTrace != nil {
		c.wiringTrace.bind(name, variables...)
	}
}

func (c *NoteSeizureCircuit) traceWiring(op string, args ...string) {
	if c.wiringTrace != nil {
		c.wiringTrace.record(op, args...)
	}
}

func (c *NoteSeizureCircuit) bindSemantic(name string, variables ...frontend.Variable) {
	if c.wiringTrace != nil {
		c.wiringTrace.bind(name, variables...)
	}
}

func (c *NoteReshapeCircuit) bindWiringTrace(api frontend.API) {
	if c.wiringTrace != nil {
		c.wiringTrace.bindCompiler(api.Compiler())
	}
}

func (c *TransferCircuit) bindWiringTrace(api frontend.API) {
	if c.wiringTrace != nil {
		c.wiringTrace.bindCompiler(api.Compiler())
	}
}

func (c *ShieldedIcs20WithdrawalCircuit) bindWiringTrace(api frontend.API) {
	if c.wiringTrace != nil {
		c.wiringTrace.bindCompiler(api.Compiler())
	}
}

func (c *NoteSeizureCircuit) bindWiringTrace(api frontend.API) {
	if c.wiringTrace != nil {
		c.wiringTrace.bindCompiler(api.Compiler())
	}
}

func noteReshapeCircuitWithTranscript(label string, nIn, nOut int, transcript *WiringTranscript) frontend.Circuit {
	circuit := NewNoteReshapeCircuit(label, nIn, nOut)
	circuit.wiringTrace = transcript
	return circuit
}

func shieldedIcs20WithdrawalCircuitWithTranscript(
	nIn int,
	transcript *WiringTranscript,
) frontend.Circuit {
	circuit := NewShieldedIcs20WithdrawalCircuit(nIn)
	circuit.wiringTrace = transcript
	return circuit
}

func transferCircuitWithTranscript(transcript *WiringTranscript) frontend.Circuit {
	circuit := NewTransferCircuit()
	circuit.wiringTrace = transcript
	return circuit
}

func noteSeizureCircuitWithTranscript(transcript *WiringTranscript) frontend.Circuit {
	circuit := NewNoteSeizureCircuit()
	circuit.wiringTrace = transcript
	return circuit
}

// ExportNoteReshapeWiringTranscript returns the canonical transcript for a
// registered NoteReshape family.
func ExportNoteReshapeWiringTranscript(label string, nIn, nOut int) (string, error) {
	transcript := newWiringTranscript(label, nIn, nOut)
	if _, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		noteReshapeCircuitWithTranscript(label, nIn, nOut, transcript),
	); err != nil {
		return "", fmt.Errorf("compile %s for wiring transcript: %w", label, err)
	}
	return transcript.canonical()
}

// ExportNoteReshape8x1WiringTranscript retains the focused test entry point.
func ExportNoteReshape8x1WiringTranscript() (string, error) {
	return ExportNoteReshapeWiringTranscript("note_reshape8x1", 8, 1)
}

// ExportTransferWiringTranscript returns the canonical transcript for the
// checked-in transfer Define path.
func ExportTransferWiringTranscript() (string, error) {
	transcript := newWiringTranscript("transfer", TransferCircuitInputs, TransferCircuitOutputs)
	if _, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		transferCircuitWithTranscript(transcript),
	); err != nil {
		return "", fmt.Errorf("compile transfer for wiring transcript: %w", err)
	}
	return transcript.canonical()
}

// ExportShieldedIcs20WithdrawalWiringTranscript returns the canonical
// transcript for the registered withdrawal Define path.
func ExportShieldedIcs20WithdrawalWiringTranscript(
	label string,
	nIn int,
) (string, error) {
	transcript := newWiringTranscript(label, nIn, 1)
	if _, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		shieldedIcs20WithdrawalCircuitWithTranscript(nIn, transcript),
	); err != nil {
		return "", fmt.Errorf("compile %s for wiring transcript: %w", label, err)
	}
	return transcript.canonical()
}

const constraintManifestSchema = "shieldd.gnark.constraint_manifest"

type ConstraintManifest struct {
	Schema           string                      `json:"schema"`
	Circuit          string                      `json:"circuit"`
	Shape            ConstraintManifestShape     `json:"shape"`
	NbConstraints    int                         `json:"nb_constraints"`
	NbPublic         int                         `json:"nb_public_variables"`
	NbSecret         int                         `json:"nb_secret_variables"`
	NbInternal       int                         `json:"nb_internal_variables"`
	SR1CSSHA256Hex   string                      `json:"sr1cs_sha256_hex,omitempty"`
	WitnessWires     []ConstraintWitnessWire     `json:"witness_wires"`
	SemanticBindings []ConstraintSemanticBinding `json:"semantic_bindings"`
	Segments         []ConstraintManifestSegment `json:"segments"`
	Breakdown        ConstraintManifestBreakdown `json:"breakdown"`
}

type ConstraintSemanticBinding struct {
	Name        string                       `json:"name"`
	Expressions []ConstraintLinearExpression `json:"expressions"`
}

type ConstraintLinearExpression struct {
	Constant string                           `json:"constant"`
	Terms    []ConstraintLinearExpressionTerm `json:"terms"`
}

type ConstraintLinearExpressionTerm struct {
	WireID      int    `json:"wire_id"`
	Coefficient string `json:"coefficient"`
}

type ConstraintWitnessWire struct {
	WireID         int    `json:"wire_id"`
	Path           string `json:"path"`
	Visibility     string `json:"visibility"`
	ConstraintRows int    `json:"constraint_rows"`
}

type ConstraintManifestShape struct {
	NIn  int `json:"n_in"`
	NOut int `json:"n_out"`
}

type ConstraintManifestSegment struct {
	Index           int      `json:"index"`
	Op              string   `json:"op"`
	Args            []string `json:"args,omitempty"`
	Start           int      `json:"start"`
	End             int      `json:"end"`
	ConstraintCount int      `json:"constraint_count"`
	InputWireIDs    []int    `json:"input_wire_ids,omitempty"`
	OutputWireIDs   []int    `json:"output_wire_ids,omitempty"`
}

type ConstraintManifestBreakdown struct {
	TotalConstraints int `json:"total_constraints"`
	Segments         int `json:"segments"`
}

func (m *ConstraintManifest) CanonicalJSON() ([]byte, error) {
	return json.MarshalIndent(m, "", "  ")
}

func WriteConstraintManifest(path string, manifest *ConstraintManifest) error {
	data, err := manifest.CanonicalJSON()
	if err != nil {
		return fmt.Errorf("marshal constraint manifest: %w", err)
	}
	data = append(data, '\n')
	if err := os.WriteFile(path, data, 0o644); err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}
	return nil
}

func ExportNoteReshapeConstraintManifest(label string, nIn, nOut int, sr1csPath string) (*ConstraintManifest, error) {
	_, manifest, err := CompileNoteReshapeForExport(label, nIn, nOut)
	if err != nil {
		return nil, err
	}
	if sr1csPath != "" {
		hash, err := sha256HexFile(sr1csPath)
		if err != nil {
			return nil, err
		}
		manifest.SR1CSSHA256Hex = hash
	}
	return manifest, nil
}

// CompileNoteReshapeForExport returns the compiled circuit and semantic manifest.
func CompileNoteReshapeForExport(label string, nIn, nOut int) (constraint.ConstraintSystem, *ConstraintManifest, error) {
	transcript := newWiringTranscript(label, nIn, nOut)
	transcript.recordCounts = true
	circuit := noteReshapeCircuitWithTranscript(label, nIn, nOut, transcript)
	ccs, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		circuit,
	)
	if err != nil {
		return nil, nil, fmt.Errorf("compile %s for export: %w", label, err)
	}
	manifest, err := transcript.constraintManifest(ccs, "", circuit)
	if err != nil {
		return nil, nil, fmt.Errorf("manifest %s for export: %w", label, err)
	}
	return ccs, manifest, nil
}

func ExportNoteReshape8x1ConstraintManifest(sr1csPath string) (*ConstraintManifest, error) {
	return ExportNoteReshapeConstraintManifest("note_reshape8x1", 8, 1, sr1csPath)
}

func ExportTransferConstraintManifest(sr1csPath string) (*ConstraintManifest, error) {
	_, manifest, err := CompileTransferForExport()
	if err != nil {
		return nil, err
	}
	if sr1csPath != "" {
		hash, err := sha256HexFile(sr1csPath)
		if err != nil {
			return nil, err
		}
		manifest.SR1CSSHA256Hex = hash
	}
	return manifest, nil
}

func ExportShieldedIcs20WithdrawalConstraintManifest(
	sr1csPath string,
) (*ConstraintManifest, error) {
	_, manifest, err := CompileShieldedIcs20WithdrawalForExport(
		"shielded_ics20_withdrawal",
		2,
	)
	if err != nil {
		return nil, err
	}
	if sr1csPath != "" {
		hash, err := sha256HexFile(sr1csPath)
		if err != nil {
			return nil, err
		}
		manifest.SR1CSSHA256Hex = hash
	}
	return manifest, nil
}

func ExportNoteSeizureConstraintManifest(sr1csPath string) (*ConstraintManifest, error) {
	_, manifest, err := CompileNoteSeizureForExport()
	if err != nil {
		return nil, err
	}
	if sr1csPath != "" {
		hash, err := sha256HexFile(sr1csPath)
		if err != nil {
			return nil, err
		}
		manifest.SR1CSSHA256Hex = hash
	}
	return manifest, nil
}

// CompileTransferForExport returns the compiled circuit and semantic manifest.
func CompileTransferForExport() (constraint.ConstraintSystem, *ConstraintManifest, error) {
	transcript := newWiringTranscript("transfer", TransferCircuitInputs, TransferCircuitOutputs)
	transcript.recordCounts = true
	circuit := transferCircuitWithTranscript(transcript)
	ccs, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		circuit,
	)
	if err != nil {
		return nil, nil, fmt.Errorf("compile transfer for export: %w", err)
	}
	manifest, err := transcript.constraintManifest(ccs, "", circuit)
	if err != nil {
		return nil, nil, fmt.Errorf("manifest transfer for export: %w", err)
	}
	return ccs, manifest, nil
}

// CompileShieldedIcs20WithdrawalForExport returns the circuit and semantic manifest.
func CompileShieldedIcs20WithdrawalForExport(
	label string,
	nIn int,
) (constraint.ConstraintSystem, *ConstraintManifest, error) {
	transcript := newWiringTranscript(label, nIn, 1)
	transcript.recordCounts = true
	circuit := shieldedIcs20WithdrawalCircuitWithTranscript(nIn, transcript)
	ccs, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		circuit,
	)
	if err != nil {
		return nil, nil, fmt.Errorf("compile %s for export: %w", label, err)
	}
	manifest, err := transcript.constraintManifest(ccs, "", circuit)
	if err != nil {
		return nil, nil, fmt.Errorf("manifest %s for export: %w", label, err)
	}
	return ccs, manifest, nil
}

// CompileNoteSeizureForExport returns the circuit and semantic manifest.
func CompileNoteSeizureForExport() (constraint.ConstraintSystem, *ConstraintManifest, error) {
	transcript := newWiringTranscript("note_seizure", 1, 0)
	transcript.recordCounts = true
	circuit := noteSeizureCircuitWithTranscript(transcript)
	ccs, err := frontend.Compile(
		ecc.BLS12_377.ScalarField(),
		r1cs.NewBuilder,
		circuit,
	)
	if err != nil {
		return nil, nil, fmt.Errorf("compile note_seizure for export: %w", err)
	}
	manifest, err := transcript.constraintManifest(ccs, "", circuit)
	if err != nil {
		return nil, nil, fmt.Errorf("manifest note_seizure for export: %w", err)
	}
	return ccs, manifest, nil
}

func (t *WiringTranscript) constraintManifest(
	ccs constraint.ConstraintSystem,
	sr1csPath string,
	circuit frontend.Circuit,
) (*ConstraintManifest, error) {
	total := ccs.GetNbConstraints()
	segments := make([]ConstraintManifestSegment, 0, len(t.events))
	for i, event := range t.events {
		start := event.constraint
		if start < 0 {
			return nil, fmt.Errorf("missing constraint count for event %d %s", i+1, event.op)
		}
		end := total
		if i+1 < len(t.events) {
			end = t.events[i+1].constraint
		}
		if end < start {
			return nil, fmt.Errorf("constraint count regressed at event %d %s: start %d end %d", i+1, event.op, start, end)
		}
		segments = append(segments, ConstraintManifestSegment{
			Index:           i + 1,
			Op:              event.op,
			Args:            append([]string(nil), event.args...),
			Start:           start,
			End:             end,
			ConstraintCount: end - start,
		})
	}

	breakdown := ConstraintManifestBreakdown{
		TotalConstraints: total,
		Segments:         len(segments),
	}

	var sr1csHash string
	if sr1csPath != "" {
		hash, err := sha256HexFile(sr1csPath)
		if err != nil {
			return nil, err
		}
		sr1csHash = hash
	}
	witnessWires, err := constraintWitnessWires(ccs, circuit)
	if err != nil {
		return nil, err
	}
	if err := attachConstraintRows(ccs, witnessWires); err != nil {
		return nil, err
	}
	semanticBindings, err := constraintSemanticBindings(ccs, t.bindings)
	if err != nil {
		return nil, err
	}

	return &ConstraintManifest{
		Schema:           constraintManifestSchema,
		Circuit:          t.circuit,
		Shape:            ConstraintManifestShape{NIn: t.nIn, NOut: t.nOut},
		NbConstraints:    total,
		NbPublic:         ccs.GetNbPublicVariables(),
		NbSecret:         ccs.GetNbSecretVariables(),
		NbInternal:       ccs.GetNbInternalVariables(),
		SR1CSSHA256Hex:   sr1csHash,
		WitnessWires:     witnessWires,
		SemanticBindings: semanticBindings,
		Segments:         segments,
		Breakdown:        breakdown,
	}, nil
}

func attachConstraintRows(
	ccs constraint.ConstraintSystem,
	witnessWires []ConstraintWitnessWire,
) error {
	r1cs, ok := ccs.(constraint.R1CS[constraint.U64])
	if !ok {
		return fmt.Errorf("constraint system is not a U64 R1CS")
	}

	rowsByWire := make(map[int]int, len(witnessWires))
	modulus := ccs.Field()
	for _, row := range r1cs.GetR1Cs() {
		left := normalizeLinearExpression(ccs, row.L, modulus)
		right := normalizeLinearExpression(ccs, row.R, modulus)
		output := normalizeLinearExpression(ccs, row.O, modulus)
		candidates := make(map[int]struct{})
		for wireID := range left.coefficients {
			candidates[wireID] = struct{}{}
		}
		for wireID := range right.coefficients {
			candidates[wireID] = struct{}{}
		}
		for wireID := range output.coefficients {
			candidates[wireID] = struct{}{}
		}
		for wireID := range candidates {
			if r1cPolynomialDependsOn(
				left,
				right,
				output,
				wireID,
				modulus,
			) {
				rowsByWire[wireID]++
			}
		}
	}

	for i := range witnessWires {
		witnessWires[i].ConstraintRows = rowsByWire[witnessWires[i].WireID]
	}
	return nil
}

type normalizedLinearExpression struct {
	constant     *big.Int
	coefficients map[int]*big.Int
}

func normalizeLinearExpression(
	ccs constraint.ConstraintSystem,
	expression constraint.LinearExpression,
	modulus *big.Int,
) normalizedLinearExpression {
	normalized := normalizedLinearExpression{
		constant:     new(big.Int),
		coefficients: make(map[int]*big.Int),
	}
	for _, term := range expression {
		coefficient := ccs.ToBigInt(ccs.GetCoefficient(term.CoeffID()))
		if term.IsConstant() || term.WireID() == 0 {
			normalized.constant.Add(normalized.constant, coefficient)
			normalized.constant.Mod(normalized.constant, modulus)
			continue
		}
		wireID := term.WireID()
		if _, ok := normalized.coefficients[wireID]; !ok {
			normalized.coefficients[wireID] = new(big.Int)
		}
		normalized.coefficients[wireID].Add(
			normalized.coefficients[wireID],
			coefficient,
		)
		normalized.coefficients[wireID].Mod(
			normalized.coefficients[wireID],
			modulus,
		)
	}
	for wireID, coefficient := range normalized.coefficients {
		if coefficient.Sign() == 0 {
			delete(normalized.coefficients, wireID)
		}
	}
	return normalized
}

// r1cPolynomialDependsOn checks actual influence in L*R-O, rather than mere
// appearance in one of the three linear expressions. This rejects wires whose
// contributions cancel into a tautology.
func r1cPolynomialDependsOn(
	left normalizedLinearExpression,
	right normalizedLinearExpression,
	output normalizedLinearExpression,
	wireID int,
	modulus *big.Int,
) bool {
	leftWire := coefficientAt(left, wireID)
	rightWire := coefficientAt(right, wireID)
	outputWire := coefficientAt(output, wireID)
	if leftWire.Sign() == 0 && rightWire.Sign() == 0 && outputWire.Sign() == 0 {
		return false
	}

	constant := new(big.Int).Mul(leftWire, right.constant)
	constant.Add(
		constant,
		new(big.Int).Mul(rightWire, left.constant),
	)
	constant.Sub(constant, outputWire)
	constant.Mod(constant, modulus)
	if constant.Sign() != 0 {
		return true
	}

	candidates := make(map[int]struct{})
	for candidate := range left.coefficients {
		candidates[candidate] = struct{}{}
	}
	for candidate := range right.coefficients {
		candidates[candidate] = struct{}{}
	}
	for candidate := range candidates {
		coefficient := new(big.Int).Mul(
			leftWire,
			coefficientAt(right, candidate),
		)
		coefficient.Add(
			coefficient,
			new(big.Int).Mul(
				rightWire,
				coefficientAt(left, candidate),
			),
		)
		coefficient.Mod(coefficient, modulus)
		if coefficient.Sign() != 0 {
			return true
		}
	}
	return false
}

func coefficientAt(expression normalizedLinearExpression, wireID int) *big.Int {
	if coefficient, ok := expression.coefficients[wireID]; ok {
		return coefficient
	}
	return new(big.Int)
}

func constraintSemanticBindings(
	ccs constraint.ConstraintSystem,
	bindings []semanticBinding,
) ([]ConstraintSemanticBinding, error) {
	result := make([]ConstraintSemanticBinding, 0, len(bindings))
	modulus := ccs.Field()
	for _, binding := range bindings {
		expressions := make([]ConstraintLinearExpression, 0, len(binding.expressions))
		for _, expression := range binding.expressions {
			constant := new(big.Int)
			coefficients := make(map[int]*big.Int)
			for _, term := range expression {
				// GetCoefficient exposes backend Montgomery limbs. Convert
				// through the field engine before treating the coefficient as
				// a protocol field value.
				coefficient := ccs.ToBigInt(
					ccs.GetCoefficient(term.CoeffID()),
				)
				if term.IsConstant() || term.WireID() == 0 {
					constant.Add(constant, coefficient)
					constant.Mod(constant, modulus)
					continue
				}
				wireID := term.WireID()
				if _, ok := coefficients[wireID]; !ok {
					coefficients[wireID] = new(big.Int)
				}
				coefficients[wireID].Add(coefficients[wireID], coefficient)
				coefficients[wireID].Mod(coefficients[wireID], modulus)
			}
			wireIDs := make([]int, 0, len(coefficients))
			for wireID, coefficient := range coefficients {
				if coefficient.Sign() != 0 {
					wireIDs = append(wireIDs, wireID)
				}
			}
			sort.Ints(wireIDs)
			terms := make([]ConstraintLinearExpressionTerm, 0, len(wireIDs))
			for _, wireID := range wireIDs {
				terms = append(terms, ConstraintLinearExpressionTerm{
					WireID: wireID, Coefficient: coefficients[wireID].String(),
				})
			}
			expressions = append(expressions, ConstraintLinearExpression{
				Constant: constant.String(),
				Terms:    terms,
			})
		}
		result = append(result, ConstraintSemanticBinding{
			Name: binding.name, Expressions: expressions,
		})
	}
	return result, nil
}

func constraintWitnessWires(
	ccs constraint.ConstraintSystem,
	circuit frontend.Circuit,
) ([]ConstraintWitnessWire, error) {
	var public, secret []string
	tVariable := reflect.TypeOf((*frontend.Variable)(nil)).Elem()
	_, err := schema.Walk(
		ccs.Field(),
		circuit,
		tVariable,
		func(leaf schema.LeafInfo, _ reflect.Value) error {
			switch leaf.Visibility {
			case schema.Public:
				public = append(public, leaf.FullName())
			case schema.Secret:
				secret = append(secret, leaf.FullName())
			}
			return nil
		},
	)
	if err != nil {
		return nil, fmt.Errorf("walk circuit witness schema: %w", err)
	}
	if len(public) != ccs.GetNbPublicVariables()-1 {
		return nil, fmt.Errorf(
			"witness schema public leaves %d != compiled public wires %d",
			len(public),
			ccs.GetNbPublicVariables()-1,
		)
	}
	if len(secret) != ccs.GetNbSecretVariables() {
		return nil, fmt.Errorf(
			"witness schema secret leaves %d != compiled secret wires %d",
			len(secret),
			ccs.GetNbSecretVariables(),
		)
	}
	wires := make([]ConstraintWitnessWire, 0, len(public)+len(secret))
	for index, path := range public {
		wires = append(wires, ConstraintWitnessWire{
			WireID: 1 + index, Path: path, Visibility: "public",
		})
	}
	for index, path := range secret {
		wires = append(wires, ConstraintWitnessWire{
			WireID: ccs.GetNbPublicVariables() + index,
			Path:   path, Visibility: "secret",
		})
	}
	return wires, nil
}

func currentConstraintCount(compiler frontend.Compiler) (int, bool) {
	if compiler == nil {
		return 0, false
	}
	if ccs, ok := compiler.(constraint.ConstraintSystem); ok {
		return ccs.GetNbConstraints(), true
	}
	v := reflect.ValueOf(compiler)
	if !v.IsValid() {
		return 0, false
	}
	if v.Kind() == reflect.Ptr {
		v = v.Elem()
	}
	if !v.IsValid() || v.Kind() != reflect.Struct {
		return 0, false
	}
	field := v.FieldByName("cs")
	if !field.IsValid() || field.IsNil() {
		return 0, false
	}
	field = reflect.NewAt(field.Type(), unsafe.Pointer(field.UnsafeAddr())).Elem()
	ccs, ok := field.Interface().(constraint.ConstraintSystem)
	if !ok {
		return 0, false
	}
	return ccs.GetNbConstraints(), true
}

func sha256HexFile(path string) (string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return "", fmt.Errorf("read %s for sha256: %w", path, err)
	}
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:]), nil
}
