//! The staged pipeline. Each stage narrows the candidate set, and only groups
//! that survive every stage are reported. Stages run on a bounded rayon pool
//! that this module builds for the run.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::num::NonZeroUsize;

use hmstd_fileops::{Failure, WorkItem, classify_io};
use hmstd_filesystem::EntryKind;
use rayon::prelude::*;
use same_file::Handle;

use crate::hash::{Compared, Digest, compare_bytes, full_digest};
use crate::identity::file_key;
use crate::progress::{Progress, Stage};
use crate::report::{DuplicateGroup, DuplicateMember, DuplicateReport, RunStatus, sort_groups};
use crate::sample::sample_digest;

/// Tuning for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Files smaller than this are ignored. The default of 1 leaves out
    /// zero-byte files, which reclaim nothing.
    pub min_size: u64,
    /// Worker threads in the bounded pool. Default: CPU count, clamped to 1..=4.
    pub threads: usize,
}

impl Default for Options {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(2, NonZeroUsize::get);
        Self {
            min_size: 1,
            threads: cores.clamp(1, 4),
        }
    }
}

/// Candidates that share one logical size.
struct SizeGroup {
    size: u64,
    members: Vec<DuplicateMember>,
}

/// One stage's output for one size group: each member paired with its value.
struct Tagged<T> {
    size: u64,
    members: Vec<(DuplicateMember, T)>,
}

/// Members whose full digest and sample agree. Not yet byte-verified.
pub(crate) struct Class {
    pub(crate) size: u64,
    pub(crate) digest: Digest,
    pub(crate) members: Vec<DuplicateMember>,
}

/// Finds duplicate files among `items` (the entries of a tree or selection).
///
/// Blocking: call it from a worker thread. Cancel through `progress`. A run
/// that was cancelled returns no groups, so a partial result is never shown
/// as complete. Only regular files are considered.
pub fn find_duplicates(
    items: &[WorkItem],
    options: &Options,
    progress: &Progress,
) -> DuplicateReport {
    let mut report = DuplicateReport::default();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.threads.max(1))
        .thread_name(|index| format!("hmstd-dup-{index}"))
        .build();
    match pool {
        Ok(pool) => pool.install(|| run(items, options, progress, &mut report)),
        Err(error) => report.status = RunStatus::Failed(error.to_string()),
    }
    progress.finish();
    if progress.is_cancelled() {
        report.status = RunStatus::Cancelled;
        report.groups.clear();
    }
    report
}

fn run(items: &[WorkItem], options: &Options, progress: &Progress, report: &mut DuplicateReport) {
    progress.begin(Stage::Sizing, 0);
    let groups = group_by_size(items, options.min_size);
    report.files_considered = groups.iter().map(|g| g.members.len() as u64).sum();

    let members: u64 = report.files_considered;
    progress.begin(Stage::Identity, members);
    let groups = resolve_identities(&groups, progress, report);
    if progress.is_cancelled() {
        return;
    }

    progress.begin(Stage::Sampling, members);
    let groups = sample_groups(&groups, progress, report);
    if progress.is_cancelled() {
        return;
    }

    let hash_bytes: u64 = groups
        .iter()
        .map(|g| g.size.saturating_mul(g.members.len() as u64))
        .sum();
    progress.begin(Stage::Hashing, hash_bytes);
    let classes = hash_groups(&groups, progress, report);
    if progress.is_cancelled() {
        return;
    }

    // Each extra member is compared against its class's first member, so the
    // work for a class is (count - 1) * size.
    let verify_bytes: u64 = classes
        .iter()
        .map(|c| {
            c.size
                .saturating_mul(c.members.len().saturating_sub(1) as u64)
        })
        .sum();
    progress.begin(Stage::Verifying, verify_bytes);
    let mut verified = verify_classes(classes, progress, report);
    if progress.is_cancelled() {
        return;
    }
    sort_groups(&mut verified);
    report.groups = verified;
}

/// Stage 1: group by logical size. Singletons are dropped.
fn group_by_size(items: &[WorkItem], min_size: u64) -> Vec<SizeGroup> {
    let mut by_size: BTreeMap<u64, Vec<DuplicateMember>> = BTreeMap::new();
    for item in items {
        if item.kind != EntryKind::File || item.logical < min_size {
            continue;
        }
        by_size
            .entry(item.logical)
            .or_default()
            .push(DuplicateMember {
                id: item.id,
                path: item.path.clone(),
            });
    }
    by_size
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .map(|(size, members)| SizeGroup { size, members })
        .collect()
}

/// Stage 2: resolve identity and keep one path per hard-linked object. Handles
/// are held only for one size group at a time, so at most that group's files
/// are open. The first path seen wins; later links count in `hardlinks_skipped`.
fn resolve_identities(
    groups: &[SizeGroup],
    progress: &Progress,
    report: &mut DuplicateReport,
) -> Vec<SizeGroup> {
    let mut out = Vec::new();
    for group in groups {
        let resolved = map_members(
            std::slice::from_ref(group),
            progress,
            report,
            |member, _| {
                progress.advance(1);
                file_key(&member.path).map(Some)
            },
        );
        if progress.is_cancelled() {
            break;
        }
        let Some(tagged) = resolved.into_iter().next() else {
            continue;
        };
        let mut seen: HashSet<Handle> = HashSet::new();
        let mut members = Vec::new();
        for (member, key) in tagged.members {
            if !seen.insert(key) {
                report.hardlinks_skipped += 1;
                continue;
            }
            members.push(member);
        }
        if members.len() >= 2 {
            out.push(SizeGroup {
                size: group.size,
                members,
            });
        }
    }
    out
}

/// Stage 3: keep members whose first and last windows agree.
fn sample_groups(
    groups: &[SizeGroup],
    progress: &Progress,
    report: &mut DuplicateReport,
) -> Vec<SizeGroup> {
    let sampled = map_members(groups, progress, report, |member, size| {
        progress.advance(1);
        sample_digest(&member.path, size).map(Some)
    });

    let mut out = Vec::new();
    for tagged in sampled {
        let size = tagged.size;
        let mut buckets: BTreeMap<Digest, Vec<DuplicateMember>> = BTreeMap::new();
        for (member, digest) in tagged.members {
            buckets.entry(digest).or_default().push(member);
        }
        for members in buckets.into_values() {
            if members.len() >= 2 {
                out.push(SizeGroup { size, members });
            }
        }
    }
    out
}

/// Stage 4: full BLAKE3 digest. Classes are keyed by (size, digest).
fn hash_groups(
    groups: &[SizeGroup],
    progress: &Progress,
    report: &mut DuplicateReport,
) -> Vec<Class> {
    let hashed = map_members(groups, progress, report, |member, _size| {
        full_digest(&member.path, progress)
    });

    let mut buckets: BTreeMap<(u64, Digest), Vec<DuplicateMember>> = BTreeMap::new();
    for tagged in hashed {
        let size = tagged.size;
        for (member, digest) in tagged.members {
            buckets.entry((size, digest)).or_default().push(member);
        }
    }
    buckets
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .map(|((size, digest), members)| Class {
            size,
            digest,
            members,
        })
        .collect()
}

/// Stage 5: byte-verify every class. Equal digests are not trusted alone.
fn verify_classes(
    classes: Vec<Class>,
    progress: &Progress,
    report: &mut DuplicateReport,
) -> Vec<DuplicateGroup> {
    let parts: Vec<(Vec<DuplicateGroup>, Vec<Failure>)> = classes
        .into_par_iter()
        .map(|class| verify_class(class, progress))
        .collect();

    let mut groups = Vec::new();
    for (mut found, failures) in parts {
        groups.append(&mut found);
        report.failures.extend(failures);
    }
    groups
}

/// Splits one class into byte-identical partitions. Only partitions of two or
/// more members become groups.
pub(crate) fn verify_class(
    class: Class,
    progress: &Progress,
) -> (Vec<DuplicateGroup>, Vec<Failure>) {
    let mut failures = Vec::new();
    let mut partitions: Vec<Vec<DuplicateMember>> = Vec::new();
    'member: for member in class.members {
        for partition in partitions.iter_mut() {
            match compare_bytes(&partition[0].path, &member.path, progress) {
                Ok(Compared::Same) => {
                    partition.push(member);
                    continue 'member;
                }
                Ok(Compared::Different) => {}
                Ok(Compared::Cancelled) => return (Vec::new(), failures),
                Err((path, error)) => {
                    failures.push(Failure::new(path, classify_io(&error), error.to_string()));
                    continue 'member;
                }
            }
        }
        partitions.push(vec![member]);
    }

    let groups = partitions
        .into_iter()
        .filter(|members| members.len() >= 2)
        .map(|members| DuplicateGroup {
            size: class.size,
            digest: class.digest,
            members,
        })
        .collect();
    (groups, failures)
}

/// Runs `work` on every member in parallel, keeping group order. Members that
/// fail are recorded in the report and dropped. Once cancelled, the remaining
/// members are dropped without running `work`.
fn map_members<T, F>(
    groups: &[SizeGroup],
    progress: &Progress,
    report: &mut DuplicateReport,
    work: F,
) -> Vec<Tagged<T>>
where
    T: Send,
    F: Fn(&DuplicateMember, u64) -> io::Result<Option<T>> + Sync,
{
    let jobs: Vec<(usize, &DuplicateMember)> = groups
        .iter()
        .enumerate()
        .flat_map(|(index, group)| group.members.iter().map(move |member| (index, member)))
        .collect();

    let results: Vec<(usize, &DuplicateMember, io::Result<Option<T>>)> = jobs
        .into_par_iter()
        .map(|(index, member)| {
            if progress.is_cancelled() {
                return (index, member, Ok(None));
            }
            (index, member, work(member, groups[index].size))
        })
        .collect();

    let mut tagged: Vec<Tagged<T>> = groups
        .iter()
        .map(|group| Tagged {
            size: group.size,
            members: Vec::new(),
        })
        .collect();
    for (index, member, result) in results {
        match result {
            Ok(Some(value)) => tagged[index].members.push((member.clone(), value)),
            Ok(None) => {}
            Err(error) => report.failures.push(Failure::new(
                member.path.clone(),
                classify_io(&error),
                error.to_string(),
            )),
        }
    }
    tagged
}
