package prototype

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
)

type poseidonRateConfig struct {
	alpha         uint32
	fullRounds    int
	partialRounds int
	width         int
	rate          int
	mds           []*big.Int
	arc           []*big.Int
}

func poseidonConfigFromVectors(raw poseidonRateVectors) poseidonRateConfig {
	return poseidonRateConfig{
		alpha:         raw.Alpha,
		fullRounds:    raw.FullRounds,
		partialRounds: raw.PartialRounds,
		width:         raw.Width,
		rate:          raw.Rate,
		mds:           mustBigIntSlice(raw.MDS),
		arc:           mustBigIntSlice(raw.ARC),
	}
}

func loadPoseidonRateParams(rate int) (poseidonRateConfig, error) {
	vectors, err := loadPrototypeVectors()
	if err != nil {
		return poseidonRateConfig{}, err
	}

	switch rate {
	case 1:
		return poseidonConfigFromVectors(vectors.Poseidon377.Rate1), nil
	case 2:
		return poseidonConfigFromVectors(vectors.Poseidon377.Rate2), nil
	case 3:
		return poseidonConfigFromVectors(vectors.Poseidon377.Rate3), nil
	case 4:
		return poseidonConfigFromVectors(vectors.Poseidon377.Rate4), nil
	case 5:
		return poseidonConfigFromVectors(vectors.Poseidon377.Rate5), nil
	case 6:
		return poseidonConfigFromVectors(vectors.Poseidon377.Rate6), nil
	case 7:
		return poseidonConfigFromVectors(vectors.Poseidon377.Rate7), nil
	default:
		return poseidonRateConfig{}, fmt.Errorf("unsupported poseidon377 rate %d", rate)
	}
}

func pow17(api frontend.API, value frontend.Variable) frontend.Variable {
	value2 := api.Mul(value, value)
	value4 := api.Mul(value2, value2)
	value8 := api.Mul(value4, value4)
	value16 := api.Mul(value8, value8)
	return api.Mul(value16, value)
}

func mixLayerMDS(api frontend.API, state []frontend.Variable, mds []*big.Int) []frontend.Variable {
	next := make([]frontend.Variable, len(state))
	for row := range next {
		sum := api.Mul(mds[row*len(state)], state[0])
		for col := 1; col < len(state); col++ {
			sum = api.Add(sum, api.Mul(mds[row*len(state)+col], state[col]))
		}
		next[row] = sum
	}
	return next
}

func pow17Native(value, modulus *big.Int) *big.Int {
	value2 := new(big.Int).Mul(value, value)
	value2.Mod(value2, modulus)
	value4 := new(big.Int).Mul(value2, value2)
	value4.Mod(value4, modulus)
	value8 := new(big.Int).Mul(value4, value4)
	value8.Mod(value8, modulus)
	value16 := new(big.Int).Mul(value8, value8)
	value16.Mod(value16, modulus)
	result := new(big.Int).Mul(value16, value)
	result.Mod(result, modulus)
	return result
}

func mixLayerMDSNative(state []*big.Int, mds []*big.Int, modulus *big.Int) []*big.Int {
	next := make([]*big.Int, len(state))
	for row := range next {
		sum := new(big.Int).Mul(mds[row*len(state)], state[0])
		sum.Mod(sum, modulus)
		for col := 1; col < len(state); col++ {
			term := new(big.Int).Mul(mds[row*len(state)+col], state[col])
			term.Mod(term, modulus)
			sum.Add(sum, term)
			sum.Mod(sum, modulus)
		}
		next[row] = sum
	}
	return next
}

func poseidonHash(api frontend.API, cfg poseidonRateConfig, domainSeparator frontend.Variable, inputs []frontend.Variable) (frontend.Variable, error) {
	if cfg.alpha != 17 {
		return nil, fmt.Errorf("unexpected poseidon alpha %d", cfg.alpha)
	}
	if len(inputs) != cfg.rate {
		return nil, fmt.Errorf("poseidon input count mismatch: got %d want %d", len(inputs), cfg.rate)
	}
	if len(cfg.mds) != cfg.width*cfg.width {
		return nil, fmt.Errorf("poseidon mds size mismatch: got %d want %d", len(cfg.mds), cfg.width*cfg.width)
	}
	expectedArc := cfg.width * (cfg.fullRounds + cfg.partialRounds)
	if len(cfg.arc) != expectedArc {
		return nil, fmt.Errorf("poseidon arc size mismatch: got %d want %d", len(cfg.arc), expectedArc)
	}

	state := make([]frontend.Variable, cfg.width)
	state[0] = domainSeparator
	copy(state[1:], inputs)

	roundConstantIndex := 0
	fullHalf := cfg.fullRounds / 2
	applyRoundConstants := func() {
		for i := range state {
			state[i] = api.Add(state[i], cfg.arc[roundConstantIndex])
			roundConstantIndex++
		}
	}

	for round := 0; round < fullHalf; round++ {
		applyRoundConstants()
		for i := range state {
			state[i] = pow17(api, state[i])
		}
		state = mixLayerMDS(api, state, cfg.mds)
	}

	for round := 0; round < cfg.partialRounds; round++ {
		applyRoundConstants()
		state[0] = pow17(api, state[0])
		state = mixLayerMDS(api, state, cfg.mds)
	}

	for round := 0; round < fullHalf; round++ {
		applyRoundConstants()
		for i := range state {
			state[i] = pow17(api, state[i])
		}
		state = mixLayerMDS(api, state, cfg.mds)
	}

	return state[1], nil
}

func poseidonHashNative(cfg poseidonRateConfig, domainSeparator *big.Int, inputs []*big.Int) (*big.Int, error) {
	if cfg.alpha != 17 {
		return nil, fmt.Errorf("unexpected poseidon alpha %d", cfg.alpha)
	}
	if len(inputs) != cfg.rate {
		return nil, fmt.Errorf("poseidon input count mismatch: got %d want %d", len(inputs), cfg.rate)
	}
	if len(cfg.mds) != cfg.width*cfg.width {
		return nil, fmt.Errorf("poseidon mds size mismatch: got %d want %d", len(cfg.mds), cfg.width*cfg.width)
	}
	expectedArc := cfg.width * (cfg.fullRounds + cfg.partialRounds)
	if len(cfg.arc) != expectedArc {
		return nil, fmt.Errorf("poseidon arc size mismatch: got %d want %d", len(cfg.arc), expectedArc)
	}

	modulus := ecc.BLS12_377.ScalarField()
	state := make([]*big.Int, cfg.width)
	state[0] = new(big.Int).Set(domainSeparator)
	for i, input := range inputs {
		state[i+1] = new(big.Int).Set(input)
	}

	roundConstantIndex := 0
	fullHalf := cfg.fullRounds / 2
	applyRoundConstants := func() {
		for i := range state {
			state[i].Add(state[i], cfg.arc[roundConstantIndex])
			state[i].Mod(state[i], modulus)
			roundConstantIndex++
		}
	}

	for round := 0; round < fullHalf; round++ {
		applyRoundConstants()
		for i := range state {
			state[i] = pow17Native(state[i], modulus)
		}
		state = mixLayerMDSNative(state, cfg.mds, modulus)
	}

	for round := 0; round < cfg.partialRounds; round++ {
		applyRoundConstants()
		state[0] = pow17Native(state[0], modulus)
		state = mixLayerMDSNative(state, cfg.mds, modulus)
	}

	for round := 0; round < fullHalf; round++ {
		applyRoundConstants()
		for i := range state {
			state[i] = pow17Native(state[i], modulus)
		}
		state = mixLayerMDSNative(state, cfg.mds, modulus)
	}

	return state[1], nil
}

func Poseidon377Hash1(api frontend.API, domainSeparator frontend.Variable, input frontend.Variable) (frontend.Variable, error) {
	cfg, err := loadPoseidonRateParams(1)
	if err != nil {
		return nil, err
	}
	return poseidonHash(api, cfg, domainSeparator, []frontend.Variable{input})
}

func Poseidon377Hash2(api frontend.API, domainSeparator frontend.Variable, inputs [2]frontend.Variable) (frontend.Variable, error) {
	cfg, err := loadPoseidonRateParams(2)
	if err != nil {
		return nil, err
	}
	return poseidonHash(api, cfg, domainSeparator, []frontend.Variable{inputs[0], inputs[1]})
}

func Poseidon377Hash3(api frontend.API, domainSeparator frontend.Variable, inputs [3]frontend.Variable) (frontend.Variable, error) {
	cfg, err := loadPoseidonRateParams(3)
	if err != nil {
		return nil, err
	}
	return poseidonHash(api, cfg, domainSeparator, []frontend.Variable{inputs[0], inputs[1], inputs[2]})
}

func Poseidon377Hash4(api frontend.API, domainSeparator frontend.Variable, inputs [4]frontend.Variable) (frontend.Variable, error) {
	cfg, err := loadPoseidonRateParams(4)
	if err != nil {
		return nil, err
	}
	return poseidonHash(api, cfg, domainSeparator, []frontend.Variable{inputs[0], inputs[1], inputs[2], inputs[3]})
}

func Poseidon377Hash5(api frontend.API, domainSeparator frontend.Variable, inputs [5]frontend.Variable) (frontend.Variable, error) {
	cfg, err := loadPoseidonRateParams(5)
	if err != nil {
		return nil, err
	}
	return poseidonHash(api, cfg, domainSeparator, []frontend.Variable{inputs[0], inputs[1], inputs[2], inputs[3], inputs[4]})
}

func Poseidon377Hash6(api frontend.API, domainSeparator frontend.Variable, inputs [6]frontend.Variable) (frontend.Variable, error) {
	cfg, err := loadPoseidonRateParams(6)
	if err != nil {
		return nil, err
	}
	return poseidonHash(api, cfg, domainSeparator, []frontend.Variable{inputs[0], inputs[1], inputs[2], inputs[3], inputs[4], inputs[5]})
}

func Poseidon377Hash7(
	api frontend.API,
	domainSeparator frontend.Variable,
	inputs [7]frontend.Variable,
) (frontend.Variable, error) {
	cfg, err := loadPoseidonRateParams(7)
	if err != nil {
		return nil, err
	}
	return poseidonHash(api, cfg, domainSeparator, []frontend.Variable{
		inputs[0],
		inputs[1],
		inputs[2],
		inputs[3],
		inputs[4],
		inputs[5],
		inputs[6],
	})
}

func Poseidon377Hash1Native(domainSeparator, input *big.Int) (*big.Int, error) {
	cfg, err := loadPoseidonRateParams(1)
	if err != nil {
		return nil, err
	}
	return poseidonHashNative(cfg, domainSeparator, []*big.Int{input})
}

func Poseidon377Hash2Native(domainSeparator *big.Int, inputs [2]*big.Int) (*big.Int, error) {
	cfg, err := loadPoseidonRateParams(2)
	if err != nil {
		return nil, err
	}
	return poseidonHashNative(cfg, domainSeparator, []*big.Int{inputs[0], inputs[1]})
}

func Poseidon377Hash3Native(domainSeparator *big.Int, inputs [3]*big.Int) (*big.Int, error) {
	cfg, err := loadPoseidonRateParams(3)
	if err != nil {
		return nil, err
	}
	return poseidonHashNative(cfg, domainSeparator, []*big.Int{inputs[0], inputs[1], inputs[2]})
}

func Poseidon377Hash4Native(domainSeparator *big.Int, inputs [4]*big.Int) (*big.Int, error) {
	cfg, err := loadPoseidonRateParams(4)
	if err != nil {
		return nil, err
	}
	return poseidonHashNative(cfg, domainSeparator, []*big.Int{inputs[0], inputs[1], inputs[2], inputs[3]})
}

func Poseidon377Hash5Native(domainSeparator *big.Int, inputs [5]*big.Int) (*big.Int, error) {
	cfg, err := loadPoseidonRateParams(5)
	if err != nil {
		return nil, err
	}
	return poseidonHashNative(cfg, domainSeparator, []*big.Int{inputs[0], inputs[1], inputs[2], inputs[3], inputs[4]})
}

func Poseidon377Hash6Native(domainSeparator *big.Int, inputs [6]*big.Int) (*big.Int, error) {
	cfg, err := loadPoseidonRateParams(6)
	if err != nil {
		return nil, err
	}
	return poseidonHashNative(cfg, domainSeparator, []*big.Int{inputs[0], inputs[1], inputs[2], inputs[3], inputs[4], inputs[5]})
}

func Poseidon377Hash7Native(domainSeparator *big.Int, inputs [7]*big.Int) (*big.Int, error) {
	cfg, err := loadPoseidonRateParams(7)
	if err != nil {
		return nil, err
	}
	return poseidonHashNative(cfg, domainSeparator, []*big.Int{
		inputs[0],
		inputs[1],
		inputs[2],
		inputs[3],
		inputs[4],
		inputs[5],
		inputs[6],
	})
}
