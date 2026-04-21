package compliance

import (
	"github.com/consensys/gnark/frontend"
	"github.com/mizufinance/penumbra/tools/gnark/internal/primitives"
)

func ComputeMetadataHash(api frontend.API, policyIDHash, resourceHash, permissionHash, tier, targetTimestamp, salt frontend.Variable) (frontend.Variable, error) {
	vectors, err := primitives.LoadPrototypeVectors()
	if err != nil {
		return nil, err
	}
	return primitives.Poseidon377Hash6(
		api,
		primitives.MustBigInt(vectors.Poseidon377.DLEQMetadataDomain),
		[6]frontend.Variable{policyIDHash, resourceHash, permissionHash, tier, targetTimestamp, salt},
	)
}
