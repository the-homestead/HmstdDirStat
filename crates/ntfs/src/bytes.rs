//! Little-endian field readers that return errors instead of panicking.

use crate::NtfsError;

pub fn slice<'a>(
    buf: &'a [u8],
    offset: usize,
    len: usize,
    what: &'static str,
) -> Result<&'a [u8], NtfsError> {
    buf.get(offset..offset.checked_add(len).ok_or(NtfsError::Truncated(what))?)
        .ok_or(NtfsError::Truncated(what))
}

pub fn u8_at(buf: &[u8], offset: usize, what: &'static str) -> Result<u8, NtfsError> {
    Ok(slice(buf, offset, 1, what)?[0])
}

pub fn u16_at(buf: &[u8], offset: usize, what: &'static str) -> Result<u16, NtfsError> {
    let bytes = slice(buf, offset, 2, what)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

pub fn u32_at(buf: &[u8], offset: usize, what: &'static str) -> Result<u32, NtfsError> {
    let bytes = slice(buf, offset, 4, what)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub fn u64_at(buf: &[u8], offset: usize, what: &'static str) -> Result<u64, NtfsError> {
    let bytes = slice(buf, offset, 8, what)?;
    let mut array = [0u8; 8];
    array.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(array))
}

/// A file reference is 48 bits of record number and 16 bits of sequence number.
pub fn record_number(reference: u64) -> u64 {
    reference & 0x0000_FFFF_FFFF_FFFF
}
