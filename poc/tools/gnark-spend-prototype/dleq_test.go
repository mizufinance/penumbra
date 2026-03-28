package prototype

import (
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/test"
)

type dleqVerifyCircuit struct {
	R            frontend.Variable
	AckX         frontend.Variable
	AckY         frontend.Variable
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
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}
	return &dleqVerifyCircuit{
		R:            vectors.DleqFixture.R,
		AckX:         vectors.DleqFixture.AckX,
		AckY:         vectors.DleqFixture.AckY,
		EpkX:         vectors.DleqFixture.EpkX,
		EpkY:         vectors.DleqFixture.EpkY,
		MetadataHash: metadata,
		IsRegulated:  isRegulated,
		PublishedC:   vectors.DleqFixture.DleqC,
		PublishedS:   vectors.DleqFixture.DleqS,
	}, nil
}

func TestDLEQVerifierMatchesRustFixture(t *testing.T) {
	vectors, err := loadPrototypeVectors()
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
	vectors, err := loadPrototypeVectors()
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
	vectors, err := loadPrototypeVectors()
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
