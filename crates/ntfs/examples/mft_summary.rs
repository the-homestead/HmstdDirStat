//! Reads a real NTFS volume's MFT and prints a summary. Needs administrator rights.
//! Usage: mft_summary <volume, e.g. //./C:> [output file]

use std::{fs::File, io::Read, time::Instant};

use timberstat_filesystem::NodeId;
use timberstat_ntfs::{Catalog, parse_boot_sector, read_records};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let volume = args.get(1).cloned().unwrap_or_else(|| "//./C:".to_string());
    let letter = volume.chars().rev().nth(1).unwrap_or('C');

    let mut report = String::new();
    let outcome = run(&volume, letter, &mut report);
    match outcome {
        Ok(()) => {}
        Err(error) => report.push_str(&format!("error: {error}\n")),
    }
    if let Some(path) = args.get(2) {
        std::fs::write(path, &report).expect("write summary");
    } else {
        print!("{report}");
    }
}

fn run(volume: &str, letter: char, report: &mut String) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let mut file = File::open(volume)?;

    let mut sector = [0u8; 512];
    file.read_exact(&mut sector)?;
    let boot = parse_boot_sector(&sector)?;
    report.push_str(&format!(
        "volume={volume} bytes_per_sector={} cluster={} record={} mft_lcn={}\n",
        boot.bytes_per_sector, boot.cluster_size, boot.record_size, boot.mft_lcn
    ));

    let mut catalog = Catalog::new();
    let stats = read_records(&mut file, boot, |number, record| {
        catalog.insert(number, record);
    })?;
    let read_secs = started.elapsed().as_secs_f64();

    let (tree, build) = catalog.into_tree(&format!("{letter}:"))?;
    let totals = tree.node(NodeId::ROOT).ok_or("empty tree")?.totals();
    report.push_str(&format!(
        "slots={} bad_records={} placed={} orphans={}\n",
        stats.slots, stats.bad_records, build.placed, build.orphans
    ));
    report.push_str(&format!(
        "files={} directories={} logical={} physical_known={} physical={} complete={}\n",
        totals.files,
        totals.directories,
        totals.logical,
        totals.physical_known,
        totals.physical,
        totals.complete
    ));
    report.push_str(&format!(
        "read_secs={read_secs:.2} total_secs={:.2}\n",
        started.elapsed().as_secs_f64()
    ));
    Ok(())
}
