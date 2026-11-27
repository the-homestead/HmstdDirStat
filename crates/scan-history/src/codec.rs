//! Compact binary encoding and checksum for persisted scans.
//!
//! File layout: `[magic(8)][schema_version u32][provenance_len u32][body_len u64][checksum(32)]`
//! followed by `[provenance: provenance_len][body: body_len]`.
//!
//! `checksum` is BLAKE3 over the concatenation of provenance and body bytes.
//! Integers are LEB128 varints, strings are a varint byte length plus UTF-8 bytes,
//! and timestamps use zigzag-encoded i64 varints with an explicit presence flag.

use std::{
    ffi::OsString,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use blake3::Hasher;
use timberstat_filesystem::{EntryKind, ScanMethod, ScanProvenance};

use crate::error::Error;
use crate::record::{Scan, ScanEntry};
use crate::schema::CURRENT_SCHEMA;

pub const MAGIC: [u8; 8] = *b"TSTSCAN\0";
pub const HEADER_SIZE: usize = 8 + 4 + 4 + 8 + 32;
const PARENT_NONE: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawScan {
    pub provenance: ScanProvenance,
    pub entries: Vec<ScanEntry>,
    pub schema_version: u32,
}

pub fn raw_from_scan(scan: &Scan) -> RawScan {
    RawScan {
        provenance: scan.provenance.clone(),
        entries: scan.entries.clone(),
        schema_version: scan.schema_version,
    }
}

fn put_uvarint(mut n: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if n == 0 {
            break;
        }
    }
}

fn get_uvarint(bytes: &[u8], pos: &mut usize) -> Result<u64, Error> {
    let mut result: u64 = 0;
    let mut shift = 0usize;
    for _ in 0..10 {
        if *pos >= bytes.len() {
            return Err(corrupt(bytes, "truncated varint"));
        }
        let byte = bytes[*pos];
        *pos += 1;
        result |= (u64::from(byte & 0x7f)) << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
    }
    Err(corrupt(bytes, "varint too long"))
}

fn put_zigzag(n: i64, out: &mut Vec<u8>) {
    let encoded = ((n << 1) ^ (n >> 63)) as u64;
    put_uvarint(encoded, out);
}

fn zigzag_decode(encoded: u64) -> i64 {
    ((encoded >> 1) as i64) ^ -((encoded & 1) as i64)
}

fn corrupt(_bytes: &[u8], reason: &str) -> Error {
    Error::Corrupt {
        path: PathBuf::new(),
        reason: reason.to_string(),
    }
}

fn kind_code(kind: &EntryKind) -> u8 {
    match kind {
        EntryKind::File => 0,
        EntryKind::Directory => 1,
        EntryKind::Symlink => 2,
        EntryKind::Other => 3,
    }
}

fn encode_provenance(provenance: &ScanProvenance) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(provenance.method.code());
    put_zigzag(provenance.timestamp_unix_secs, &mut out);
    let encoded = provenance.root.as_os_str().as_encoded_bytes();
    put_uvarint(encoded.len() as u64, &mut out);
    out.extend_from_slice(encoded);
    out
}

fn decode_provenance(bytes: &[u8]) -> Result<ScanProvenance, Error> {
    let mut pos = 0usize;
    let method_code = get_uvarint(bytes, &mut pos)? as u8;
    let method =
        ScanMethod::from_code(method_code).ok_or_else(|| corrupt(bytes, "unknown scan method"))?;
    let timestamp = zigzag_decode(get_uvarint(bytes, &mut pos)?);
    let root_len = get_uvarint(bytes, &mut pos)? as usize;
    if pos + root_len > bytes.len() {
        return Err(corrupt(bytes, "truncated scan root"));
    }
    let root_bytes = &bytes[pos..pos + root_len];
    let root = OsString::from(String::from_utf8_lossy(root_bytes).into_owned());
    Ok(ScanProvenance {
        method,
        timestamp_unix_secs: timestamp,
        root: root.into(),
    })
}

fn encode_entry(entry: &ScanEntry, version: u32) -> Vec<u8> {
    let mut out = Vec::new();
    put_uvarint(
        entry.parent.map(|p| p as u64).unwrap_or(PARENT_NONE as u64),
        &mut out,
    );
    let encoded = entry.name.as_encoded_bytes();
    put_uvarint(encoded.len() as u64, &mut out);
    out.extend_from_slice(encoded);
    out.push(kind_code(&entry.kind));
    put_uvarint(entry.logical_size, &mut out);
    put_uvarint(entry.physical_size.unwrap_or(0), &mut out);
    let mut modified_present = false;
    if let Some(timestamp) = entry.modified_unix_secs {
        modified_present = true;
        put_zigzag(timestamp, &mut out);
    }
    put_uvarint(if modified_present { 1 } else { 0 }, &mut out);
    out.push(
        entry.readonly as u8
            | (entry.hardlink_duplicate as u8) << 1
            | (entry.unreadable as u8) << 2,
    );
    if version >= 2 {
        let has_identity = entry.identity.is_some();
        put_uvarint(if has_identity { 1 } else { 0 }, &mut out);
        if has_identity {
            let identity = entry.identity.unwrap();
            put_uvarint(identity.volume, &mut out);
            put_uvarint(identity.file_id, &mut out);
        }
    }
    out
}

fn decode_entry(bytes: &[u8], pos: &mut usize, version: u32) -> Result<ScanEntry, Error> {
    let parent_raw = get_uvarint(bytes, pos)? as u32;
    let parent = if parent_raw == PARENT_NONE {
        None
    } else {
        Some(parent_raw)
    };
    let name_len = get_uvarint(bytes, pos)? as usize;
    if *pos + name_len > bytes.len() {
        return Err(corrupt(bytes, "truncated scan entry name"));
    }
    let name_bytes = &bytes[*pos..*pos + name_len];
    *pos += name_len;
    let name = OsString::from(String::from_utf8_lossy(name_bytes).into_owned());
    let kind = match get_uvarint(bytes, pos)? {
        0 => EntryKind::File,
        1 => EntryKind::Directory,
        2 => EntryKind::Symlink,
        _ => return Err(corrupt(bytes, "unknown entry kind")),
    };
    let logical_size = get_uvarint(bytes, pos)?;
    let physical_size = get_uvarint(bytes, pos)?;
    let modified_present = get_uvarint(bytes, pos)? == 1;
    let modified_unix_secs = if modified_present {
        Some(zigzag_decode(get_uvarint(bytes, pos)?))
    } else {
        None
    };
    let flags = get_uvarint(bytes, pos)? as u8;
    let mut entry = ScanEntry {
        parent,
        name,
        kind,
        logical_size,
        physical_size: Some(physical_size),
        identity: None,
        hardlink_duplicate: (flags & 0x02) != 0,
        unreadable: (flags & 0x04) != 0,
        modified_unix_secs,
        readonly: (flags & 0x01) != 0,
        windows_attributes: None,
    };
    if version >= 2 {
        let has_identity = get_uvarint(bytes, pos)? == 1;
        if has_identity {
            let volume = get_uvarint(bytes, pos)?;
            let file_id = get_uvarint(bytes, pos)?;
            entry.identity = Some(timberstat_filesystem::FileIdentity { volume, file_id });
        }
    }
    Ok(entry)
}

pub fn encode(raw: &RawScan) -> Result<Vec<u8>, Error> {
    let provenance_bytes = encode_provenance(&raw.provenance);
    let mut body = Vec::new();
    for entry in &raw.entries {
        body.extend_from_slice(&encode_entry(entry, raw.schema_version));
    }
    let mut hasher = Hasher::new();
    hasher.update(&provenance_bytes);
    hasher.update(&body);
    let checksum = hasher.finalize().as_bytes().to_vec();
    let mut file_bytes = Vec::new();
    file_bytes.extend_from_slice(&MAGIC);
    file_bytes.extend_from_slice(&u32::to_le_bytes(raw.schema_version));
    file_bytes.extend_from_slice(&(provenance_bytes.len() as u32).to_le_bytes());
    file_bytes.extend_from_slice(&(body.len() as u64).to_le_bytes());
    file_bytes.extend_from_slice(&checksum);
    file_bytes.extend_from_slice(&provenance_bytes);
    file_bytes.extend_from_slice(&body);
    Ok(file_bytes)
}

pub fn decode_file(path: &Path) -> Result<RawScan, Error> {
    let mut file = File::open(path).map_err(|source| Error::Io {
        operation: "open scan file",
        path: path.to_path_buf(),
        source,
    })?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|source| Error::Io {
        operation: "read scan file",
        path: path.to_path_buf(),
        source,
    })?;

    if buffer.len() < HEADER_SIZE || buffer[..8] != MAGIC {
        return Err(Error::Corrupt {
            path: path.to_path_buf(),
            reason: "invalid header".into(),
        });
    }
    let schema_version = u32::from_le_bytes(buffer[8..12].try_into().unwrap());
    let provenance_len = u32::from_le_bytes(buffer[12..16].try_into().unwrap()) as usize;
    let body_len = u64::from_le_bytes(buffer[16..24].try_into().unwrap()) as usize;
    let checksum: [u8; 32] = buffer[24..56].try_into().unwrap();

    if HEADER_SIZE + provenance_len + body_len > buffer.len() {
        return Err(Error::Corrupt {
            path: path.to_path_buf(),
            reason: "file truncated".into(),
        });
    }
    let provenance_bytes = buffer[HEADER_SIZE..HEADER_SIZE + provenance_len].to_vec();
    let body =
        buffer[HEADER_SIZE + provenance_len..HEADER_SIZE + provenance_len + body_len].to_vec();

    let mut hasher = Hasher::new();
    hasher.update(&provenance_bytes);
    hasher.update(&body);
    if hasher.finalize().as_bytes() != &checksum {
        return Err(Error::Corrupt {
            path: path.to_path_buf(),
            reason: "checksum mismatch".into(),
        });
    }

    let provenance = decode_provenance(&provenance_bytes)?;
    let entries = decode_entries(schema_version, &body)?;
    Ok(RawScan {
        provenance,
        entries,
        schema_version,
    })
}

fn decode_entries(schema_version: u32, bytes: &[u8]) -> Result<Vec<ScanEntry>, Error> {
    let mut pos = 0usize;
    let mut entries = Vec::new();
    while pos < bytes.len() {
        entries.push(decode_entry(bytes, &mut pos, schema_version)?);
    }
    Ok(entries)
}

pub fn to_scan(raw: RawScan) -> Result<Scan, Error> {
    let mut scan = Scan::new(raw.provenance, raw.entries);
    if scan.schema_version != CURRENT_SCHEMA {
        crate::schema::migrate(scan.schema_version, &mut scan)?;
    }
    Ok(scan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_method_codes_match_the_persistent_format() {
        for (method, code) in [(ScanMethod::GenericTraversal, 1), (ScanMethod::NtfsMft, 2)] {
            let provenance = ScanProvenance {
                method,
                timestamp_unix_secs: 0,
                root: PathBuf::new(),
            };
            let encoded = encode_provenance(&provenance);
            assert_eq!(encoded.first(), Some(&code));
            assert_eq!(decode_provenance(&encoded).unwrap(), provenance);
        }
    }
}
