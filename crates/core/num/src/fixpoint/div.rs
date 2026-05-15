use ethnum::U256;
use ibig::UBig;

use super::Error;

/// Computes (2^128 * x) / y and its remainder.
/// TEMP HACK: need to implement this properly
pub(super) fn stub_div_rem_u384_by_u256(x: U256, y: U256) -> Result<(U256, U256), Error> {
    if y == U256::ZERO {
        return Err(Error::DivisionByZero);
    }

    let x_big = ibig::UBig::from_le_bytes(&x.to_le_bytes());
    let y_big = ibig::UBig::from_le_bytes(&y.to_le_bytes());
    // this is what we actually want to compute: 384-bit / 256-bit division.
    let x_big_128 = x_big << 128;
    let q_big = &x_big_128 / &y_big;
    let rem_big = x_big_128 - (&y_big * &q_big);

    let Some(q) = ubig_to_u256(&q_big) else {
        return Err(Error::Overflow);
    };
    let rem = ubig_to_u256(&rem_big).expect("rem < q, so we already returned on overflow");

    Ok((q, rem))
}

fn ubig_to_u256(x: &UBig) -> Option<U256> {
    let bytes = x.to_le_bytes();
    if bytes.len() <= 32 {
        let mut u256_bytes = [0; 32];
        u256_bytes[..bytes.len()].copy_from_slice(&bytes);
        Some(U256::from_le_bytes(u256_bytes))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn u256_to_ubig(x: U256) -> UBig {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(&x.to_le_bytes());
        UBig::from_le_bytes(&bytes)
    }

    fn u256_strategy() -> BoxedStrategy<U256> {
        any::<[u8; 32]>().prop_map(U256::from_le_bytes).boxed()
    }

    proptest! {
        #[test]
        fn stub_div_rem_works(
            x in u256_strategy(),
            y in u256_strategy()
        ) {
            let Ok((q, rem)) = stub_div_rem_u384_by_u256(x, y) else {
                return Ok(());
            };

            let q_big = u256_to_ubig(q);
            let rem_big = u256_to_ubig(rem);
            let x_big = u256_to_ubig(x);
            let y_big = u256_to_ubig(y);

            let rhs = x_big << 128;
            let lhs = &q_big * &y_big + &rem_big;
            assert_eq!(rhs, lhs);
        }
    }
}
