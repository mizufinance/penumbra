package compliance

import (
	"fmt"

	"golang.org/x/crypto/blake2b"

	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

const (
	unregulatedDKPubDomain  = "penumbra.compliance.unregulated.dk-pub.v1"
	unregulatedRingPKDomain = "penumbra.compliance.unregulated.ring-pk.v1"
)

func deriveUnregulatedSinkPoint(domain string) (gnarkte.Point, error) {
	hash := blake2b.Sum512([]byte(domain))
	pointDomain := primitives.LittleEndianBytesToBigInt(hash[:])
	point, err := primitives.Decaf377EncodeToCurveNative(pointDomain)
	if err != nil {
		return gnarkte.Point{}, fmt.Errorf("derive unregulated sink point for %q: %w", domain, err)
	}
	return point, nil
}

func UnregulatedComplianceKeys() (gnarkte.Point, gnarkte.Point, error) {
	dkPub, err := deriveUnregulatedSinkPoint(unregulatedDKPubDomain)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	ringPK, err := deriveUnregulatedSinkPoint(unregulatedRingPKDomain)
	if err != nil {
		return gnarkte.Point{}, gnarkte.Point{}, err
	}
	return ringPK, dkPub, nil
}

func SelectPoint(
	api frontend.API,
	cond frontend.Variable,
	ifTrue gnarkte.Point,
	ifFalse gnarkte.Point,
) gnarkte.Point {
	return gnarkte.Point{
		X: api.Select(cond, ifTrue.X, ifFalse.X),
		Y: api.Select(cond, ifTrue.Y, ifFalse.Y),
	}
}
