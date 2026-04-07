package circuits

import (
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/penumbra-zone/penumbra/tools/gnark/internal/primitives"
)

type Point2D struct {
	X frontend.Variable
	Y frontend.Variable
}

func BoolToField(value bool) int {
	if value {
		return 1
	}
	return 0
}

func PointAffineToNative(point primitives.PointAffineFixture) gnarkte.Point {
	return gnarkte.Point{
		X: primitives.MustBigInt(point.X),
		Y: primitives.MustBigInt(point.Y),
	}
}
