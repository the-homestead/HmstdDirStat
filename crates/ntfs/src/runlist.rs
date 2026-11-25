//! Runlists: how a non-resident attribute maps onto clusters on disk.

use crate::{NtfsError, bytes};

/// A contiguous piece of an attribute. `lcn: None` is a sparse run (no clusters on disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub lcn: Option<u64>,
    pub clusters: u64,
}

pub fn decode(bytes_in: &[u8]) -> Result<Vec<Run>, NtfsError> {
    let mut runs = Vec::new();
    let mut pos = 0usize;
    let mut lcn: i64 = 0;

    while pos < bytes_in.len() {
        let header = bytes_in[pos];
        pos += 1;
        if header == 0 {
            break;
        }
        let length_size = usize::from(header & 0x0F);
        let offset_size = usize::from(header >> 4);
        if length_size == 0 || length_size > 8 || offset_size > 8 {
            return Err(NtfsError::Corrupt("runlist header"));
        }

        let clusters = read_unsigned(bytes_in, pos, length_size)?;
        pos += length_size;
        if offset_size == 0 {
            runs.push(Run {
                lcn: None,
                clusters,
            });
            continue;
        }

        let delta = read_signed(bytes_in, pos, offset_size)?;
        pos += offset_size;
        lcn = lcn
            .checked_add(delta)
            .filter(|value| *value >= 0)
            .ok_or(NtfsError::Corrupt("runlist offset"))?;
        runs.push(Run {
            lcn: Some(lcn.unsigned_abs()),
            clusters,
        });
    }
    Ok(runs)
}

fn read_unsigned(buf: &[u8], pos: usize, size: usize) -> Result<u64, NtfsError> {
    let bytes = bytes::slice(buf, pos, size, "runlist")?;
    Ok(bytes
        .iter()
        .rev()
        .fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte)))
}

fn read_signed(buf: &[u8], pos: usize, size: usize) -> Result<i64, NtfsError> {
    let unsigned = read_unsigned(buf, pos, size)?;
    let shift = 64 - 8 * size as u32;
    // Sign-extend from the top bit of the last byte.
    Ok(((unsigned << shift) as i64) >> shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_absolute_and_relative_offsets() {
        // Run 1: 0x18 clusters at LCN 0x3000. Run 2: 0x10 clusters, delta -0x1000 (LCN 0x2000).
        let encoded = [0x21, 0x18, 0x00, 0x30, 0x21, 0x10, 0x00, 0xF0, 0x00];
        let runs = decode(&encoded).unwrap();
        assert_eq!(
            runs,
            vec![
                Run {
                    lcn: Some(0x3000),
                    clusters: 0x18
                },
                Run {
                    lcn: Some(0x2000),
                    clusters: 0x10
                },
            ]
        );
    }

    #[test]
    fn sparse_runs_have_no_lcn() {
        let runs = decode(&[0x01, 0x05, 0x00]).unwrap();
        assert_eq!(
            runs,
            vec![Run {
                lcn: None,
                clusters: 5
            }]
        );
    }

    #[test]
    fn truncated_run_is_an_error() {
        assert!(decode(&[0x21, 0x18]).is_err());
    }
}
