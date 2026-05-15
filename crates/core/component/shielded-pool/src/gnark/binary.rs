use std::io::{Cursor, Read};

use anyhow::{anyhow, Context, Result};

use crate::gnark::typed::{MerklePathBinary, PointAffineBytes};

pub(crate) const MAX_VEC32_LENGTH: usize = crate::transfer::TRANSFER_STATEMENT_FIELD_COUNT;
pub(crate) const MAX_TRIPLE_PATH_LENGTH: usize = 24;
pub(crate) const MAX_MERKLE_PATH_LAYERS: usize = penumbra_sdk_compliance::DEFAULT_DEPTH as usize;
pub(crate) const MAX_MERKLE_PATH_SIBLINGS: usize = 3;

pub(crate) struct BinaryCursor<'a> {
    inner: Cursor<&'a [u8]>,
}

impl<'a> BinaryCursor<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            inner: Cursor::new(bytes),
        }
    }

    pub(crate) fn finish(self, label: &str) -> Result<()> {
        let remaining = self.inner.get_ref().len() - (self.inner.position() as usize);
        anyhow::ensure!(remaining == 0, "{label} has {remaining} trailing bytes");
        Ok(())
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8> {
        let mut out = [0u8; 1];
        self.inner.read_exact(&mut out)?;
        Ok(out[0])
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32> {
        let mut out = [0u8; 4];
        self.inner.read_exact(&mut out)?;
        Ok(u32::from_le_bytes(out))
    }

    pub(crate) fn read_u64(&mut self) -> Result<u64> {
        let mut out = [0u8; 8];
        self.inner.read_exact(&mut out)?;
        Ok(u64::from_le_bytes(out))
    }

    pub(crate) fn read_fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        self.inner.read_exact(&mut out)?;
        Ok(out)
    }

    pub(crate) fn read_vec_32(&mut self) -> Result<Vec<[u8; 32]>> {
        let len = self.read_u32()? as usize;
        anyhow::ensure!(len <= MAX_VEC32_LENGTH, "vec32 length {len} exceeds max {MAX_VEC32_LENGTH}");
        (0..len).map(|_| self.read_fixed::<32>()).collect()
    }

    pub(crate) fn read_triple_path_32(&mut self) -> Result<Vec<[[u8; 32]; 3]>> {
        let len = self.read_u32()? as usize;
        anyhow::ensure!(
            len <= MAX_TRIPLE_PATH_LENGTH,
            "triple path length {len} exceeds max {MAX_TRIPLE_PATH_LENGTH}"
        );
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push([
                self.read_fixed::<32>()?,
                self.read_fixed::<32>()?,
                self.read_fixed::<32>()?,
            ]);
        }
        Ok(out)
    }

    pub(crate) fn read_merkle_path(&mut self) -> Result<MerklePathBinary> {
        let layers = self.read_u32()? as usize;
        anyhow::ensure!(
            layers <= MAX_MERKLE_PATH_LAYERS,
            "merkle path layer count {layers} exceeds max {MAX_MERKLE_PATH_LAYERS}"
        );
        let mut out = Vec::with_capacity(layers);
        for _ in 0..layers {
            let siblings = self.read_u32()? as usize;
            anyhow::ensure!(
                siblings == MAX_MERKLE_PATH_SIBLINGS,
                "merkle path sibling count {siblings} must equal {MAX_MERKLE_PATH_SIBLINGS}"
            );
            out.push([
                self.read_fixed::<32>()?,
                self.read_fixed::<32>()?,
                self.read_fixed::<32>()?,
            ]);
        }
        Ok(MerklePathBinary { layers: out })
    }

    pub(crate) fn read_point_affine(&mut self) -> Result<PointAffineBytes> {
        Ok(PointAffineBytes {
            x: self.read_fixed::<32>()?,
            y: self.read_fixed::<32>()?,
        })
    }
}

pub(crate) fn put_u8(buf: &mut Vec<u8>, value: u8) {
    buf.push(value);
}

pub(crate) fn put_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

pub(crate) fn encode_vec_32(buf: &mut Vec<u8>, values: &[[u8; 32]]) -> Result<()> {
    if values.len() > MAX_VEC32_LENGTH {
        return Err(anyhow!(
            "vec32 length {} exceeds max {MAX_VEC32_LENGTH}",
            values.len()
        ));
    }
    put_u32(
        buf,
        u32::try_from(values.len()).context("vector length exceeds u32")?,
    );
    for value in values {
        put_bytes(buf, value);
    }
    Ok(())
}

pub(crate) fn encode_triple_path_32(buf: &mut Vec<u8>, path: &[[[u8; 32]; 3]]) -> Result<()> {
    if path.len() > MAX_TRIPLE_PATH_LENGTH {
        return Err(anyhow!(
            "triple path length {} exceeds max {MAX_TRIPLE_PATH_LENGTH}",
            path.len()
        ));
    }
    put_u32(
        buf,
        u32::try_from(path.len()).context("triple path length exceeds u32")?,
    );
    for siblings in path {
        for sibling in siblings {
            put_bytes(buf, sibling);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_vec_32_rejects_oversized_length_before_allocation() {
        let bytes = ((MAX_VEC32_LENGTH + 1) as u32).to_le_bytes();
        let err = BinaryCursor::new(&bytes)
            .read_vec_32()
            .expect_err("oversized vec32 should fail");

        assert!(
            err.to_string().contains("vec32 length"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn encode_vec_32_rejects_oversized_length() {
        let values = vec![[0u8; 32]; MAX_VEC32_LENGTH + 1];
        let err = encode_vec_32(&mut Vec::new(), &values).expect_err("oversized vec32 should fail");

        assert!(
            err.to_string().contains("vec32 length"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn read_triple_path_32_rejects_oversized_length_before_allocation() {
        let bytes = ((MAX_TRIPLE_PATH_LENGTH + 1) as u32).to_le_bytes();
        let err = BinaryCursor::new(&bytes)
            .read_triple_path_32()
            .expect_err("oversized triple path should fail");

        assert!(
            err.to_string().contains("triple path length"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn encode_triple_path_32_rejects_oversized_length() {
        let path = vec![[[0u8; 32]; 3]; MAX_TRIPLE_PATH_LENGTH + 1];
        let err = encode_triple_path_32(&mut Vec::new(), &path)
            .expect_err("oversized triple path should fail");

        assert!(
            err.to_string().contains("triple path length"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn read_merkle_path_rejects_oversized_layer_count_before_allocation() {
        let bytes = ((MAX_MERKLE_PATH_LAYERS + 1) as u32).to_le_bytes();
        let err = BinaryCursor::new(&bytes)
            .read_merkle_path()
            .expect_err("oversized merkle path should fail");

        assert!(
            err.to_string().contains("merkle path layer count"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn read_merkle_path_rejects_oversized_sibling_count_before_allocation() {
        let mut bytes = Vec::new();
        put_u32(&mut bytes, 1);
        put_u32(&mut bytes, (MAX_MERKLE_PATH_SIBLINGS + 1) as u32);
        let err = BinaryCursor::new(&bytes)
            .read_merkle_path()
            .expect_err("oversized merkle sibling count should fail");

        assert!(
            err.to_string().contains("merkle path sibling count"),
            "unexpected error: {err:#}"
        );
    }
}
