use std::io::{Cursor, Read};

use anyhow::{anyhow, Context, Result};

use crate::gnark::typed::{MerklePathBinary, PointAffineBytes};

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
        if remaining != 0 {
            return Err(anyhow!("{label} has {remaining} trailing bytes"));
        }
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
        (0..len).map(|_| self.read_fixed::<32>()).collect()
    }

    pub(crate) fn read_merkle_path(&mut self) -> Result<MerklePathBinary> {
        let layers = self.read_u32()? as usize;
        let mut out = Vec::with_capacity(layers);
        for _ in 0..layers {
            let siblings = self.read_u32()? as usize;
            if siblings != 3 {
                return Err(anyhow!("expected 3 merkle siblings, got {siblings}"));
            }
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
    put_u32(
        buf,
        u32::try_from(values.len()).context("vector length exceeds u32")?,
    );
    for value in values {
        put_bytes(buf, value);
    }
    Ok(())
}
