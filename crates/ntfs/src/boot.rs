//! NTFS boot sector: the handful of fields needed to find the Master File Table.

use crate::{NtfsError, bytes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootSector {
    pub bytes_per_sector: u64,
    pub cluster_size: u64,
    /// First cluster of the MFT's first record.
    pub mft_lcn: u64,
    pub record_size: u64,
}

pub fn parse_boot_sector(sector: &[u8]) -> Result<BootSector, NtfsError> {
    if sector.len() < 512 {
        return Err(NtfsError::Truncated("boot sector"));
    }
    if bytes::slice(sector, 3, 8, "OEM id")? != b"NTFS    " {
        return Err(NtfsError::NotNtfs);
    }

    let bytes_per_sector = u64::from(bytes::u16_at(sector, 0x0B, "sector size")?);
    let sectors_per_cluster = u64::from(bytes::u8_at(sector, 0x0D, "cluster size")?);
    if !(512..=4096).contains(&bytes_per_sector) || sectors_per_cluster == 0 {
        return Err(NtfsError::BadBootSector("sector or cluster size"));
    }
    let cluster_size = bytes_per_sector * sectors_per_cluster;

    let mft_lcn = bytes::u64_at(sector, 0x30, "MFT location")?;

    // Positive values count clusters; negative values are a power-of-two exponent.
    let raw = bytes::u8_at(sector, 0x40, "record size")? as i8;
    let record_size = if raw > 0 {
        u64::from(raw.unsigned_abs()) * cluster_size
    } else if (-31..0).contains(&raw) {
        1u64 << raw.unsigned_abs()
    } else {
        return Err(NtfsError::BadBootSector("file record size"));
    };
    if !(1024..=65_536).contains(&record_size) {
        return Err(NtfsError::BadBootSector("file record size out of range"));
    }

    Ok(BootSector {
        bytes_per_sector,
        cluster_size,
        mft_lcn,
        record_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot(bps: u16, spc: u8, raw_record: i8) -> Vec<u8> {
        let mut sector = vec![0u8; 512];
        sector[3..11].copy_from_slice(b"NTFS    ");
        sector[0x0B..0x0D].copy_from_slice(&bps.to_le_bytes());
        sector[0x0D] = spc;
        sector[0x30..0x38].copy_from_slice(&786_432u64.to_le_bytes());
        sector[0x40] = raw_record as u8;
        sector
    }

    #[test]
    fn typical_volume_parses() {
        let parsed = parse_boot_sector(&boot(512, 8, -10)).unwrap();
        assert_eq!(parsed.cluster_size, 4096);
        assert_eq!(parsed.record_size, 1024);
        assert_eq!(parsed.mft_lcn, 786_432);
    }

    #[test]
    fn positive_record_size_counts_clusters() {
        let parsed = parse_boot_sector(&boot(512, 8, 1)).unwrap();
        assert_eq!(parsed.record_size, 4096);
    }

    #[test]
    fn non_ntfs_is_rejected() {
        let mut sector = boot(512, 8, -10);
        sector[3..11].copy_from_slice(b"EXFAT   ");
        assert!(matches!(
            parse_boot_sector(&sector),
            Err(NtfsError::NotNtfs)
        ));
    }

    #[test]
    fn zero_cluster_factor_is_rejected() {
        assert!(parse_boot_sector(&boot(512, 0, -10)).is_err());
    }
}
