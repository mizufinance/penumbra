package primitives

import "math/big"

func LittleEndianBytesToBigInt(le []byte) *big.Int {
	be := append([]byte(nil), le...)
	for i, j := 0, len(be)-1; i < j; i, j = i+1, j-1 {
		be[i], be[j] = be[j], be[i]
	}
	return new(big.Int).SetBytes(be)
}
