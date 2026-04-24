package compliance

import (
	"math/big"

	curves "github.com/consensys/gnark-crypto/ecc/twistededwards"
	"github.com/consensys/gnark/frontend"
	gnarkte "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/std/conversion"
	"github.com/consensys/gnark/std/hash/sha2"
	"github.com/consensys/gnark/std/math/uints"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

var derivationDomainBytes = []byte("elgamal-derivation-v1\x00\x00")

func DeriveACKFromLeafDNative(ringPK gnarkte.Point, d *big.Int) (gnarkte.Point, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	return primitives.ScalarMulNative(ringPK, d, primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen())
}

func DeriveACKFromLeafD(api frontend.API, ringPK gnarkte.Point, d frontend.Variable) (gnarkte.Point, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return gnarkte.Point{}, err
	}
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	return ScalarMulLE(api, curve, ringPK, d, primitives.MustBigInt(vectors.Decaf377CompanionCurve.Order).BitLen()), nil
}

func ScalarMulBitsLE(api frontend.API, curve gnarkte.Curve, base gnarkte.Point, bits []frontend.Variable) gnarkte.Point {
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

func DeriveACKFromSubjectBD(api frontend.API, ringPK gnarkte.Point, subjectBD frontend.Variable) (gnarkte.Point, error) {
	curve, err := gnarkte.NewEdCurve(api, curves.BLS12_377)
	if err != nil {
		return gnarkte.Point{}, err
	}
	subjectBytesMSB, err := conversion.NativeToBytes(api, subjectBD)
	if err != nil {
		return gnarkte.Point{}, err
	}
	subjectBytesLE := make([]uints.U8, len(subjectBytesMSB))
	for i := range subjectBytesMSB {
		subjectBytesLE[i] = subjectBytesMSB[len(subjectBytesMSB)-1-i]
	}

	h, err := sha2.New(api)
	if err != nil {
		return gnarkte.Point{}, err
	}
	h.Write(uints.NewU8Array(derivationDomainBytes))
	h.Write(subjectBytesLE)
	digest := h.Sum()

	byteAPI, err := uints.NewBytes(api)
	if err != nil {
		return gnarkte.Point{}, err
	}
	digestBitsLE := make([]frontend.Variable, 0, len(digest)*8)
	for _, b := range digest {
		digestBitsLE = append(digestBitsLE, api.ToBinary(byteAPI.Value(b), 8)...)
	}
	return ScalarMulBitsLE(api, curve, ringPK, digestBitsLE), nil
}
