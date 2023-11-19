use bytes::{buf::Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::accumulator::{CobsAccumulator, FeedResult};
use crate::{Error, Result};

fn offset_from(p2: *const u8, p1: *const u8) -> usize {
    p2 as usize - p1 as usize
}
pub struct CborCobsCodec<D, S, const N: usize> {
    deser: std::marker::PhantomData<D>,
    ser: std::marker::PhantomData<S>,
    decoder: CobsAccumulator<N>,
}

impl<D, S, const N: usize> Default for CborCobsCodec<D, S, N> {
    fn default() -> Self {
        Self {
            deser: std::marker::PhantomData,
            ser: std::marker::PhantomData,
            decoder: CobsAccumulator::new(),
        }
    }
}

impl<D, S, const N: usize> Decoder for CborCobsCodec<D, S, N>
where
    D: serde::de::DeserializeOwned,
{
    type Item = D;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        let trim_at: usize;
        let ret = match self.decoder.feed::<D>(src) {
            FeedResult::Consumed => {
                trim_at = src.len();
                Ok(None)
            }
            FeedResult::OverFull(remaining) => {
                trim_at = offset_from(remaining.as_ptr(), src.as_ptr());
                Err(Error::FrameOverflow)
            }
            FeedResult::DeserError(remaining) => {
                trim_at = offset_from(remaining.as_ptr(), src.as_ptr());
                Err(Error::Deserialization)
            }
            FeedResult::Success { data, remaining } => {
                trim_at = offset_from(remaining.as_ptr(), src.as_ptr());
                Ok(Some(data))
            }
        };

        src.advance(trim_at);

        ret
    }
}

// We encode `S` and not `&S` because we do not want to deal with
// the lifetime issues (this is used in async contexts.)
impl<D, S, const N: usize> Encoder<S> for CborCobsCodec<D, S, N>
where
    S: serde::Serialize,
{
    type Error = Error;
    fn encode(&mut self, msg: S, final_buf: &mut BytesMut) -> Result<()> {
        let msg_size = std::mem::size_of::<S>();
        let alloc_size = std::cmp::max(msg_size, 16) * 4; // guess
        let mut buf = vec![0u8; alloc_size];
        let encoded = crate::to_slice_cobs(&msg, &mut buf)?;
        final_buf.extend_from_slice(&encoded);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct MyStruct {
        val1: u8,
        val2: u8,
    }

    #[test]
    fn roundtrip() -> Result<()> {
        let msg1 = MyStruct { val1: 12, val2: 34 };
        let msg2 = MyStruct { val1: 56, val2: 78 };
        let mut bytes = BytesMut::new();
        let mut codec = CborCobsCodec::<_, MyStruct, 1024>::default();
        codec.encode(msg1.clone(), &mut bytes)?;
        codec.encode(msg2.clone(), &mut bytes)?;
        let found1: Option<MyStruct> = codec.decode(&mut bytes)?;
        let found2: Option<MyStruct> = codec.decode(&mut bytes)?;
        assert_eq!(found1, Some(msg1));
        assert_eq!(found2, Some(msg2));
        Ok(())
    }
}
