//! One MFT file record: fixups, header flags, and the attributes this reader needs
//! ($FILE_NAME, unnamed $DATA, $ATTRIBUTE_LIST presence).

use crate::{
    NtfsError, bytes,
    runlist::{self, Run},
};

const ATTR_ATTRIBUTE_LIST: u32 = 0x20;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_END: u32 = 0xFFFF_FFFF;
const FLAG_IN_USE: u16 = 0x01;
const FLAG_DIRECTORY: u16 = 0x02;

/// A name from `$FILE_NAME`, with the directory record it lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameEntry {
    pub parent: u64,
    /// 0 POSIX, 1 Win32, 2 DOS (8.3), 3 Win32 and DOS in one name.
    pub namespace: u8,
    pub name: String,
}

/// Size of the unnamed `$DATA` stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DataInfo {
    pub logical: u64,
    pub allocated: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    pub in_use: bool,
    pub is_dir: bool,
    /// Base record number for extension records; 0 for base records.
    pub base: u64,
    pub names: Vec<NameEntry>,
    pub data: Option<DataInfo>,
    /// Runs of the unnamed `$DATA` when non-resident. Needed to find the MFT itself.
    pub data_runs: Option<Vec<Run>>,
    pub has_attribute_list: bool,
}

/// Parses one record in place; fixups are undone inside `buf`.
/// Returns `Ok(None)` for slots that do not hold a `FILE` record.
pub fn parse_record(buf: &mut [u8], bytes_per_sector: usize) -> Result<Option<Record>, NtfsError> {
    if bytes::slice(buf, 0, 4, "record signature")? != b"FILE" {
        return Ok(None);
    }
    // Unused slots are most of a mostly-empty MFT; skip their fixups and attributes.
    if bytes::u16_at(buf, 0x16, "record flags")? & FLAG_IN_USE == 0 {
        return Ok(Some(Record::default()));
    }
    apply_fixups(buf, bytes_per_sector)?;

    let flags = bytes::u16_at(buf, 0x16, "record flags")?;
    let mut record = Record {
        in_use: flags & FLAG_IN_USE != 0,
        is_dir: flags & FLAG_DIRECTORY != 0,
        base: bytes::record_number(bytes::u64_at(buf, 0x20, "base record")?),
        ..Record::default()
    };

    let mut pos = usize::from(bytes::u16_at(buf, 0x14, "first attribute")?);
    loop {
        let attr_type = bytes::u32_at(buf, pos, "attribute type")?;
        if attr_type == ATTR_END {
            break;
        }
        let length = bytes::u32_at(buf, pos + 4, "attribute length")? as usize;
        if length < 16 || pos + length > buf.len() {
            return Err(NtfsError::Corrupt("attribute length"));
        }
        let attr = &buf[pos..pos + length];
        let non_resident = bytes::u8_at(attr, 8, "residency")? != 0;
        let name_len = bytes::u8_at(attr, 9, "attribute name length")?;

        match attr_type {
            ATTR_ATTRIBUTE_LIST => record.has_attribute_list = true,
            ATTR_FILE_NAME if !non_resident => {
                if let Some(name) = parse_file_name(attr)? {
                    record.names.push(name);
                }
            }
            ATTR_DATA if name_len == 0 && record.data.is_none() => {
                let (data, runs) = parse_data(attr, non_resident)?;
                record.data = Some(data);
                record.data_runs = runs;
            }
            _ => {}
        }
        pos += length;
    }
    Ok(Some(record))
}

/// Undoes the per-sector fixups. The last two bytes of every sector were overwritten
/// with the update sequence number; the originals are kept in the update array.
fn apply_fixups(buf: &mut [u8], bytes_per_sector: usize) -> Result<(), NtfsError> {
    let usa_offset = usize::from(bytes::u16_at(buf, 4, "update sequence offset")?);
    let usa_count = usize::from(bytes::u16_at(buf, 6, "update sequence count")?);
    if usa_count < 2 || bytes_per_sector < 2 {
        return Err(NtfsError::Corrupt("update sequence"));
    }
    let usn = bytes::slice(buf, usa_offset, 2, "update sequence number")?.to_vec();

    for index in 1..usa_count {
        let sector_end = index * bytes_per_sector - 2;
        let original = bytes::slice(buf, usa_offset + 2 * index, 2, "fixup entry")?.to_vec();
        if bytes::slice(buf, sector_end, 2, "sector tail")? != usn.as_slice() {
            return Err(NtfsError::FixupMismatch);
        }
        buf[sector_end..sector_end + 2].copy_from_slice(&original);
    }
    Ok(())
}

fn parse_file_name(attr: &[u8]) -> Result<Option<NameEntry>, NtfsError> {
    let value_len = bytes::u32_at(attr, 16, "name value length")? as usize;
    let value_offset = usize::from(bytes::u16_at(attr, 20, "name value offset")?);
    let value = bytes::slice(attr, value_offset, value_len, "name value")?;

    let parent = bytes::record_number(bytes::u64_at(value, 0, "name parent")?);
    let name_len = usize::from(bytes::u8_at(value, 0x40, "name length")?);
    let namespace = bytes::u8_at(value, 0x41, "name namespace")?;
    let units: Vec<u16> = bytes::slice(value, 0x42, name_len * 2, "name characters")?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    Ok(Some(NameEntry {
        parent,
        namespace,
        name: String::from_utf16_lossy(&units),
    }))
}

fn parse_data(attr: &[u8], non_resident: bool) -> Result<(DataInfo, Option<Vec<Run>>), NtfsError> {
    if !non_resident {
        let value_len = bytes::u32_at(attr, 16, "resident data length")?;
        return Ok((
            DataInfo {
                logical: u64::from(value_len),
                allocated: 0,
            },
            None,
        ));
    }
    let allocated = bytes::u64_at(attr, 40, "allocated size")?;
    let logical = bytes::u64_at(attr, 48, "real size")?;
    let runlist_offset = usize::from(bytes::u16_at(attr, 32, "runlist offset")?);
    let runs = runlist::decode(bytes::slice(
        attr,
        runlist_offset,
        attr.len() - runlist_offset,
        "runlist",
    )?)?;
    Ok((DataInfo { logical, allocated }, Some(runs)))
}
