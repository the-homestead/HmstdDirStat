//! Parsers for handler listing output.
//!
//! Handlers print listing text meant for humans, so parsing is best-effort:
//! sizes are [`Option`] and an unparseable line is skipped rather than failing
//! the whole listing. Names are kept verbatim (including subdirectories).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEntry {
    pub path: String,
    pub size: Option<u64>,
    pub packed: Option<u64>,
    pub directory: bool,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedListing {
    pub entries: Vec<ParsedEntry>,
    pub total_original: Option<u64>,
    pub total_compressed: Option<u64>,
    pub warnings: Vec<String>,
}

fn finish(
    entries: Vec<ParsedEntry>,
    compressed_hint: Option<u64>,
    warnings: Vec<String>,
) -> ParsedListing {
    let mut total_original = 0u64;
    let mut total_packed = 0u64;
    let mut have_original = false;
    let mut have_packed = false;
    for entry in &entries {
        if entry.directory {
            continue;
        }
        if let Some(size) = entry.size {
            total_original = total_original.saturating_add(size);
            have_original = true;
        }
        if let Some(size) = entry.packed {
            total_packed = total_packed.saturating_add(size);
            have_packed = true;
        }
    }
    let total_compressed = if have_packed {
        Some(total_packed)
    } else {
        compressed_hint
    };
    ParsedListing {
        entries,
        total_original: have_original.then_some(total_original),
        total_compressed,
        warnings,
    }
}

/// `7z l -slt`: key = value blocks separated by a `----------` rule.
pub fn parse_7z(output: &str) -> ParsedListing {
    let mut entries: Vec<ParsedEntry> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut in_entries = false;

    let mut path: Option<String> = None;
    let mut size: Option<u64> = None;
    let mut packed: Option<u64> = None;
    let mut directory = false;
    let mut encrypted = false;

    let mut flush = |path: &mut Option<String>,
                     size: &mut Option<u64>,
                     packed: &mut Option<u64>,
                     directory: &mut bool,
                     encrypted: &mut bool| {
        if let Some(path) = path.take() {
            entries.push(ParsedEntry {
                path,
                size: size.take(),
                packed: packed.take(),
                directory: *directory,
                encrypted: *encrypted,
            });
        }
        *directory = false;
        *encrypted = false;
    };

    for raw in output.lines() {
        let line = raw.trim_end_matches('\r');
        if line.starts_with("----------") {
            if in_entries {
                flush(
                    &mut path,
                    &mut size,
                    &mut packed,
                    &mut directory,
                    &mut encrypted,
                );
            }
            in_entries = true;
            continue;
        }
        if !in_entries {
            continue;
        }
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        match key {
            "Path" => {
                flush(
                    &mut path,
                    &mut size,
                    &mut packed,
                    &mut directory,
                    &mut encrypted,
                );
                path = Some(value.to_owned());
            }
            "Size" => {
                if !value.is_empty() {
                    size = value.parse().ok();
                }
            }
            "Packed Size" => {
                if !value.is_empty() {
                    packed = value.parse().ok();
                }
            }
            "Attributes" => {
                directory = value.starts_with('D');
            }
            "Encrypted" => {
                encrypted = value == "+";
                if encrypted {
                    warnings.push(format!(
                        "encrypted entry: {}",
                        path.clone().unwrap_or_default()
                    ));
                }
            }
            _ => {}
        }
    }
    flush(
        &mut path,
        &mut size,
        &mut packed,
        &mut directory,
        &mut encrypted,
    );

    finish(entries, None, warnings)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn is_iso_date(token: &str) -> bool {
    token.len() == 10
        && token.as_bytes().get(4) == Some(&b'-')
        && token.as_bytes().get(7) == Some(&b'-')
}

/// `tar -tvf` (GNU and bsdtar both print permissions, owner, size, date, name).
pub fn parse_tar_verbose(output: &str) -> ParsedListing {
    let mut entries: Vec<ParsedEntry> = Vec::new();
    for raw in output.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 4 {
            continue;
        }
        let directory = tokens[0].starts_with('d');
        let mut size: Option<u64> = None;
        let mut name: Option<String> = None;
        for (index, token) in tokens.iter().enumerate() {
            let is_month = MONTHS.contains(token);
            if is_month || is_iso_date(token) {
                if index >= 1 {
                    size = tokens
                        .get(index.saturating_sub(1))
                        .and_then(|value| value.parse::<u64>().ok());
                }
                let rest = tokens.get(index + 3..).unwrap_or(&[]);
                if !rest.is_empty() {
                    name = Some(rest.join(" "));
                }
                break;
            }
        }
        let path = name.unwrap_or_else(|| tokens[tokens.len() - 1].to_owned());
        entries.push(ParsedEntry {
            path,
            size,
            packed: None,
            directory,
            encrypted: false,
        });
    }
    finish(entries, None, Vec::new())
}

/// `unzip -l`: size in the first column, name after date and time.
pub fn parse_unzip(output: &str) -> ParsedListing {
    let mut entries: Vec<ParsedEntry> = Vec::new();
    for raw in output.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 4 {
            continue;
        }
        let Ok(size) = tokens[0].parse::<u64>() else {
            continue;
        };
        let name = tokens[3..].join(" ");
        if name == "Name" || name.starts_with("-----") {
            continue;
        }
        entries.push(ParsedEntry {
            path: name,
            size: Some(size),
            packed: None,
            directory: false,
            encrypted: false,
        });
    }
    finish(entries, None, Vec::new())
}

/// `gzip -l` / `xz -l` / `zstd -l`: report compressed and uncompressed sizes.
pub fn parse_stream_listing(output: &str) -> ParsedListing {
    let mut entries: Vec<ParsedEntry> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    for raw in output.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let mut numbers: Vec<u64> = Vec::new();
        let mut name: Option<String> = None;
        for (index, token) in tokens.iter().enumerate() {
            let cleaned = token.trim_end_matches('%');
            if let Ok(value) = cleaned.replace(',', "").parse::<u64>() {
                numbers.push(value);
            } else if index > 0
                && token.chars().any(|c| c.is_alphabetic())
                && !token.contains(':')
                && name.is_none()
                && !token.eq_ignore_ascii_case("filename")
            {
                name = Some(tokens[index..].join(" "));
            }
        }
        let Some(name) = name else {
            continue;
        };
        if name.to_ascii_lowercase().contains("filename")
            || name.to_ascii_lowercase().contains("ratio")
        {
            continue;
        }
        let (packed, original) = match numbers.as_slice() {
            [packed, original, ..] => (Some(*packed), Some(*original)),
            [packed] => (Some(*packed), None),
            _ => (None, None),
        };
        if !numbers.is_empty() && numbers.len() > 2 {
            warnings.push("stream listing had extra numeric columns".into());
        }
        entries.push(ParsedEntry {
            path: name,
            size: original,
            packed,
            directory: false,
            encrypted: false,
        });
    }
    finish(entries, None, warnings)
}

/// Gzip's listing puts the ratio between the two sizes; this picks the two
/// size columns when the simple heuristic missed them.
pub fn fallback_entry(name: &str, packed: Option<u64>, original: Option<u64>) -> ParsedEntry {
    ParsedEntry {
        path: name.to_owned(),
        size: original,
        packed,
        directory: false,
        encrypted: false,
    }
}
