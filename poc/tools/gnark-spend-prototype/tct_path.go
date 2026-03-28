package prototype

import (
	"math/big"

	"github.com/consensys/gnark/frontend"
)

func tctLeafHash(api frontend.API, commitment frontend.Variable) (frontend.Variable, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}
	return Poseidon377Hash1(api, mustBigInt(vectors.Poseidon377.TCTDomain), commitment)
}

func tctNodeHash(api frontend.API, height int, children [4]frontend.Variable) (frontend.Variable, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}
	domain := new(big.Int).Add(mustBigInt(vectors.Poseidon377.TCTDomain), big.NewInt(int64(height)))
	domain.Mod(domain, ScalarField())
	return Poseidon377Hash4(api, domain, children)
}

func VerifyStateCommitmentPath(
	api frontend.API,
	commitment frontend.Variable,
	position frontend.Variable,
	authPath [][3]frontend.Variable,
) (frontend.Variable, error) {
	leaf, err := tctLeafHash(api, commitment)
	if err != nil {
		return nil, err
	}

	positionBits := api.ToBinary(position, 48)
	current := leaf
	for height := 1; height <= len(authPath); height++ {
		shift := 2 * (height - 1)
		bit0 := positionBits[shift]
		bit1 := positionBits[shift+1]

		isIndex0 := api.Mul(api.Sub(1, bit0), api.Sub(1, bit1))
		isIndex1 := api.Mul(bit0, api.Sub(1, bit1))
		isIndex2 := api.Mul(api.Sub(1, bit0), bit1)
		isIndex3 := api.Mul(bit0, bit1)
		siblings := authPath[len(authPath)-height]

		child0 := api.Select(isIndex0, current, siblings[0])
		child1Not1 := api.Select(isIndex0, siblings[0], siblings[1])
		child1 := api.Select(isIndex1, current, child1Not1)
		child2Not2 := api.Select(bit1, siblings[2], siblings[1])
		child2 := api.Select(isIndex2, current, child2Not2)
		child3 := api.Select(isIndex3, current, siblings[2])

		current, err = tctNodeHash(api, height, [4]frontend.Variable{child0, child1, child2, child3})
		if err != nil {
			return nil, err
		}
	}
	return current, nil
}

func VerifyStateCommitmentPathNative(fixture spendFixture) (*big.Int, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return nil, err
	}
	domain := mustBigInt(vectors.Poseidon377.TCTDomain)
	current, err := Poseidon377Hash1Native(domain, mustBigInt(fixture.Private.StateCommitmentProof.Commitment))
	if err != nil {
		return nil, err
	}

	position := fixture.Private.StateCommitmentProof.Position
	for height := 1; height <= len(fixture.Private.StateCommitmentProof.AuthPath); height++ {
		shift := 2 * (height - 1)
		bit0 := (position >> shift) & 1
		bit1 := (position >> (shift + 1)) & 1
		index := int(bit0 + 2*bit1)
		siblings := fixture.Private.StateCommitmentProof.AuthPath[len(fixture.Private.StateCommitmentProof.AuthPath)-height]

		children := [4]*big.Int{}
		siblingIdx := 0
		for i := 0; i < 4; i++ {
			if i == index {
				children[i] = current
			} else {
				children[i] = mustBigInt(siblings[siblingIdx])
				siblingIdx++
			}
		}

		heightDomain := new(big.Int).Add(domain, big.NewInt(int64(height)))
		heightDomain.Mod(heightDomain, ScalarField())
		current, err = Poseidon377Hash4Native(heightDomain, children)
		if err != nil {
			return nil, err
		}
	}
	return current, nil
}
