package compliance

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type dleqVerifyCircuit struct {
	R            frontend.Variable
	AckX         frontend.Variable
	AckY         frontend.Variable
	SPointX      frontend.Variable
	SPointY      frontend.Variable
	EpkX         frontend.Variable
	EpkY         frontend.Variable
	MetadataHash frontend.Variable
	IsRegulated  frontend.Variable

	PublishedC frontend.Variable `gnark:",public"`
	PublishedS frontend.Variable `gnark:",public"`
}

func (c *dleqVerifyCircuit) Define(api frontend.API) error {
	return VerifyDLEQ(
		api,
		c.R,
		point(c.AckX, c.AckY),
		point(c.SPointX, c.SPointY),
		point(c.EpkX, c.EpkY),
		c.MetadataHash,
		c.PublishedC,
		c.PublishedS,
		c.IsRegulated,
	)
}

func point(x, y frontend.Variable) gnarkte.Point {
	return gnarkte.Point{X: x, Y: y}
}

func loadDLEQAssignment(metadata string, isRegulated int) (*dleqVerifyCircuit, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}
	orderBitLen := primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()
	sPoint, err := primitives.ScalarMulNative(
		gnarkte.Point{
			X: primitives.MustBigInt(vectors.DleqFixture.AckX),
			Y: primitives.MustBigInt(vectors.DleqFixture.AckY),
		},
		primitives.MustBigInt(vectors.DleqFixture.R),
		orderBitLen,
	)
	if err != nil {
		return nil, err
	}
	return &dleqVerifyCircuit{
		R:            vectors.DleqFixture.R,
		AckX:         vectors.DleqFixture.AckX,
		AckY:         vectors.DleqFixture.AckY,
		SPointX:      sPoint.X.(*big.Int).String(),
		SPointY:      sPoint.Y.(*big.Int).String(),
		EpkX:         vectors.DleqFixture.EpkX,
		EpkY:         vectors.DleqFixture.EpkY,
		MetadataHash: metadata,
		IsRegulated:  isRegulated,
		PublishedC:   vectors.DleqFixture.DleqC,
		PublishedS:   vectors.DleqFixture.DleqS,
	}, nil
}

func TestDLEQVerifierMatchesRustFixture(t *testing.T) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	assignment, err := loadDLEQAssignment(vectors.DleqFixture.MetadataHash, 1)
	if err != nil {
		t.Fatalf("build assignment: %v", err)
	}

	assert := test.NewAssert(t)
	assert.CheckCircuit(
		&dleqVerifyCircuit{},
		test.WithCurves(ecc.BLS12_377),
		test.WithBackends(backend.GROTH16),
		test.WithValidAssignment(assignment),
	)
}

func TestDLEQVerifierRejectsWrongMetadataWhenRegulated(t *testing.T) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	assignment, err := loadDLEQAssignment(vectors.DleqFixture.WrongMetadataHash, 1)
	if err != nil {
		t.Fatalf("build assignment: %v", err)
	}

	if err := test.IsSolved(&dleqVerifyCircuit{}, assignment, ecc.BLS12_377.ScalarField()); err == nil {
		t.Fatalf("expected wrong metadata to fail the regulated DLEQ verifier")
	}
}

func TestDLEQVerifierSkipsWrongMetadataWhenUnregulated(t *testing.T) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}
	assignment, err := loadDLEQAssignment(vectors.DleqFixture.WrongMetadataHash, 0)
	if err != nil {
		t.Fatalf("build assignment: %v", err)
	}

	if err := test.IsSolved(&dleqVerifyCircuit{}, assignment, ecc.BLS12_377.ScalarField()); err != nil {
		t.Fatalf("expected unregulated DLEQ verifier to skip challenge enforcement: %v", err)
	}
}

func scalarMulLENaiveForTest(api frontend.API, curve gnarkte.Curve, base gnarkte.Point, scalar frontend.Variable, nBits int) gnarkte.Point {
	bits := api.ToBinary(scalar, nBits)
	result := gnarkte.Point{X: 0, Y: 1}
	current := base

	for _, bit := range bits {
		sum := curve.Add(result, current)
		result = gnarkte.Point{
			X: api.Select(bit, sum.X, result.X),
			Y: api.Select(bit, sum.Y, result.Y),
		}
		current = curve.Double(current)
	}

	return result
}

type scalarMulEquivalenceCircuit struct {
	P1X frontend.Variable
	P1Y frontend.Variable
	P2X frontend.Variable
	P2Y frontend.Variable
	S1  frontend.Variable
	S2  frontend.Variable
}

func (c *scalarMulEquivalenceCircuit) Define(api frontend.API) error {
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return err
	}
	p1 := point(c.P1X, c.P1Y)
	p2 := point(c.P2X, c.P2Y)
	newResult := curve.DoubleBaseScalarMul(p1, p2, c.S1, c.S2)
	oldLeft := scalarMulLENaiveForTest(api, curve, p1, c.S1, primitives.Decaf377FieldBits)
	oldRight := scalarMulLENaiveForTest(api, curve, p2, c.S2, primitives.Decaf377FieldBits)
	oldResult := curve.Add(oldLeft, oldRight)
	primitives.AssertDecafEquivalent(api, newResult, oldResult)
	return nil
}

func TestDoubleBaseScalarMulMatchesNaiveImplementation(t *testing.T) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		t.Fatalf("load vectors: %v", err)
	}

	tests := []struct {
		name   string
		assign *scalarMulEquivalenceCircuit
	}{
		{
			name: "generator and epk",
			assign: &scalarMulEquivalenceCircuit{
				P1X: vectors.Decaf377CompanionCurve.GeneratorX,
				P1Y: vectors.Decaf377CompanionCurve.GeneratorY,
				P2X: vectors.DleqFixture.EpkX,
				P2Y: vectors.DleqFixture.EpkY,
				S1:  vectors.DleqFixture.DleqS,
				S2:  vectors.DleqFixture.DleqC,
			},
		},
		{
			name: "ack and epk",
			assign: &scalarMulEquivalenceCircuit{
				P1X: vectors.DleqFixture.AckX,
				P1Y: vectors.DleqFixture.AckY,
				P2X: vectors.DleqFixture.EpkX,
				P2Y: vectors.DleqFixture.EpkY,
				S1:  vectors.DleqFixture.DleqS,
				S2:  vectors.DleqFixture.DleqC,
			},
		},
	}

	assert := test.NewAssert(t)
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			assert.CheckCircuit(
				&scalarMulEquivalenceCircuit{},
				test.WithCurves(ecc.BLS12_377),
				test.WithBackends(backend.GROTH16),
				test.WithValidAssignment(tc.assign),
			)
		})
	}
}
