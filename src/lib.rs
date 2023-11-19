#![cfg_attr(not(feature = "std"), no_std)]

// The code in this module is based on
// [postcard](https://crates.io/crates/postcard).

use serde::{Deserialize, Serialize};

pub mod accumulator;

#[cfg(feature = "codec")]
pub mod codec;

#[cfg(not(feature = "std"))]
extern crate core as std;

pub type Result<T> = std::result::Result<T, Error>;

const SENTINEL: u8 = 0x00;

#[derive(Debug)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum Error {
    #[cfg(feature = "std")]
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[cfg_attr(feature = "std", error("serde_cbor error {0}"))]
    Cbor(#[cfg_attr(feature = "std", from)] serde_cbor::Error),
    #[cfg_attr(feature = "std", error("COBS error"))]
    Cobs,
    #[cfg_attr(feature = "std", error("frame overflow"))]
    FrameOverflow,
    #[cfg_attr(feature = "std", error("deserialization"))]
    Deserialization,
    #[cfg_attr(feature = "std", error("deserialize bad encoding"))]
    DeserializeBadEncoding,
}

// ------
// This is borrowed from postcard.

/// Deserialize a message of type `T` from a cobs-encoded byte slice. The
/// unused portion (if any) of the byte slice is not returned.
/// The used portion of the input slice is modified during deserialization (even if an error is returned).
/// Therefore, if this is not desired, pass a clone of the original slice.
pub fn from_bytes_cobs<'a, T>(s: &'a mut [u8]) -> Result<T>
where
    T: Deserialize<'a>,
{
    debug_assert_eq!(crate::SENTINEL, 0x00); // otherwise use cobs::decode_in_place_with_sentinel
    let sz = cobs::decode_in_place(s).map_err(|_| Error::DeserializeBadEncoding)?;
    let cbor_encoded = &mut s[..sz];
    from_bytes::<T>(cbor_encoded)
}

/// Deserialize a message of type `T` from a byte slice. The unused portion (if any)
/// of the byte slice is not returned.
/// The used portion of the input slice is modified during deserialization (even if an error is returned).
/// Therefore, if this is not desired, pass a clone of the original slice.
pub fn from_bytes<'a, T>(s: &'a mut [u8]) -> Result<T>
where
    T: Deserialize<'a>,
{
    let mut deserializer = serde_cbor::Deserializer::from_mut_slice(s);
    let t = T::deserialize(&mut deserializer).map_err(|e| Error::Cbor(e))?;
    Ok(t)
}

/// Serialize a `T` to the given slice.
///
/// When successful, this function returns the slice containing the serialized
/// (but not COBS-encoded) message.
pub fn to_slice<'a, 'b, T>(value: &'b T, buf: &'a mut [u8]) -> Result<&'a mut [u8]>
where
    T: Serialize + ?Sized,
{
    use serde_cbor::ser::SliceWrite;
    use serde_cbor::Serializer;

    let writer = SliceWrite::new(&mut buf[..]);
    let mut ser = Serializer::new(writer);
    value.serialize(&mut ser).map_err(|e| Error::Cbor(e))?;
    let writer = ser.into_inner();
    let size = writer.bytes_written();
    Ok(&mut buf[..size])
}

/// Serialize a `T` to the given slice, with the resulting slice containing data
/// in a serialized then COBS encoded format. The terminating sentinel byte
/// (0x00) is included in the output buffer.
///
/// When successful, this function returns the slice containing the serialized
/// and encoded message.
pub fn to_slice_cobs<'a, 'b, T>(value: &'b T, buf: &'a mut [u8]) -> Result<&'a mut [u8]>
where
    T: Serialize + ?Sized,
{
    // Encode to CBOR, filling first bytes of buf.
    let nbytes = to_slice(value, buf)?.len();
    let (cbor_encoded, future_use) = buf.split_at_mut(nbytes);

    // hmm, this is not very memory efficient. We simply use the rest of the
    // buffer given to us originally. Can COBS rewrite in place?

    let sz = cobs::encode_with_sentinel(cbor_encoded, &mut future_use[..], SENTINEL);

    future_use[sz] = SENTINEL;
    Ok(&mut future_use[..sz + 1])
}
