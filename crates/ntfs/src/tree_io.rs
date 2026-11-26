//! Binary save format for a `FileTree`. The elevated Fast scan helper writes the tree
//! with [`write_tree`]; the window reads it back with [`read_tree`]. Records are written
//! parents first, so a single pass rebuilds the tree.

use std::{
    ffi::OsString,
    io::{self, BufRead, BufWriter, Read, Write},
};

use timberstat_filesystem::{EntryKind, FileTree, Metadata, NewEntry, NodeId};

const MAGIC: &[u8; 8] = b"HMSTDT01";
const NO_PARENT: u32 = u32::MAX;
const FLAG_PHYSICAL: u8 = 1;
const FLAG_HARDLINK_DUPLICATE: u8 = 2;
const FLAG_UNREADABLE: u8 = 4;

fn kind_code(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::File => 0,
        EntryKind::Directory => 1,
        EntryKind::Symlink => 2,
        EntryKind::Other => 3,
    }
}

fn kind_from_code(code: u8) -> io::Result<EntryKind> {
    Ok(match code {
        0 => EntryKind::File,
        1 => EntryKind::Directory,
        2 => EntryKind::Symlink,
        3 => EntryKind::Other,
        _ => return Err(invalid("unknown entry kind")),
    })
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Writes every node of `tree`, parents first. Names are stored as UTF-8; a name that is
/// not valid Unicode is written with replacement characters. Returns the node count.
pub fn write_tree<W: Write>(tree: &FileTree, output: W) -> io::Result<u64> {
    let mut out = BufWriter::with_capacity(1 << 20, output);
    out.write_all(MAGIC)?;
    out.write_all(&(tree.len() as u64).to_le_bytes())?;

    let mut stack: Vec<(NodeId, u32)> = vec![(tree.root(), NO_PARENT)];
    let mut written: u32 = 0;
    while let Some((id, parent)) = stack.pop() {
        let node = tree
            .node(id)
            .ok_or_else(|| io::Error::other("tree lists a node that does not exist"))?;

        let mut flags = 0u8;
        if node.physical_size().is_some() {
            flags |= FLAG_PHYSICAL;
        }
        if node.is_hardlink_duplicate() {
            flags |= FLAG_HARDLINK_DUPLICATE;
        }
        if node.is_unreadable() {
            flags |= FLAG_UNREADABLE;
        }
        let name = node.name().to_string_lossy();

        out.write_all(&parent.to_le_bytes())?;
        out.write_all(&[kind_code(node.kind()), flags])?;
        out.write_all(&node.logical_size().to_le_bytes())?;
        out.write_all(&node.physical_size().unwrap_or(0).to_le_bytes())?;
        out.write_all(&(name.len() as u32).to_le_bytes())?;
        out.write_all(name.as_bytes())?;

        let me = written;
        written += 1;
        // Children pushed in reverse so they pop in insertion order (parents still first).
        let kids: Vec<NodeId> = tree.children(id).collect();
        for kid in kids.into_iter().rev() {
            stack.push((kid, me));
        }
    }
    if u64::from(written) != tree.len() as u64 {
        return Err(io::Error::other("tree has nodes unreachable from the root"));
    }
    out.flush()?;
    Ok(u64::from(written))
}

/// Rebuilds a tree written by [`write_tree`] and rolls up its totals.
pub fn read_tree<R: BufRead>(mut input: R) -> io::Result<FileTree> {
    let mut magic = [0u8; 8];
    input.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid("not a saved folder tree"));
    }
    let count = read_u64(&mut input)?;

    let mut ids: Vec<NodeId> = Vec::new();
    let mut tree: Option<FileTree> = None;
    for index in 0..count {
        let parent = read_u32(&mut input)?;
        let mut head = [0u8; 2];
        input.read_exact(&mut head)?;
        let kind = kind_from_code(head[0])?;
        let flags = head[1];
        let logical = read_u64(&mut input)?;
        let physical = read_u64(&mut input)?;
        let name_len = read_u32(&mut input)? as usize;
        let mut name = vec![0u8; name_len];
        input.read_exact(&mut name)?;
        let name = String::from_utf8(name).map_err(|_| invalid("name is not UTF-8"))?;

        let entry = NewEntry {
            name: OsString::from(name),
            kind,
            logical_size: logical,
            physical_size: (flags & FLAG_PHYSICAL != 0).then_some(physical),
            identity: None,
            hardlink_duplicate: flags & FLAG_HARDLINK_DUPLICATE != 0,
            unreadable: flags & FLAG_UNREADABLE != 0,
            metadata: Metadata::default(),
        };

        if index == 0 {
            if parent != NO_PARENT {
                return Err(invalid("first record is not the root"));
            }
            tree = Some(FileTree::with_root(entry));
            ids.push(NodeId::ROOT);
        } else {
            let parent_id = *ids
                .get(parent as usize)
                .ok_or_else(|| invalid("record names a parent that has not been read"))?;
            let current = tree.as_mut().ok_or_else(|| invalid("missing root"))?;
            let id = current
                .add_child(parent_id, entry)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            ids.push(id);
        }
    }

    let mut tree = tree.ok_or_else(|| invalid("empty tree"))?;
    tree.roll_up();
    Ok(tree)
}

fn read_u32<R: Read>(input: &mut R) -> io::Result<u32> {
    let mut bytes = [0u8; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64<R: Read>(input: &mut R) -> io::Result<u64> {
    let mut bytes = [0u8; 8];
    input.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn file(name: &str, size: u64) -> NewEntry {
        NewEntry {
            name: OsString::from(name),
            kind: EntryKind::File,
            logical_size: size,
            physical_size: Some(size + 4096),
            identity: None,
            hardlink_duplicate: false,
            unreadable: false,
            metadata: Metadata::default(),
        }
    }

    fn dir(name: &str) -> NewEntry {
        NewEntry::directory(OsString::from(name), Metadata::default())
    }

    #[test]
    fn tree_round_trips_with_totals() {
        let mut tree = FileTree::with_root(dir("C:"));
        let docs = tree.add_child(NodeId::ROOT, dir("docs")).unwrap();
        tree.add_child(docs, file("a.txt", 10)).unwrap();
        tree.add_child(docs, file("b.txt", 20)).unwrap();
        tree.add_child(NodeId::ROOT, file("top.bin", 300)).unwrap();
        tree.roll_up();

        let mut bytes = Vec::new();
        assert_eq!(write_tree(&tree, &mut bytes).unwrap(), 5);
        let back = read_tree(Cursor::new(bytes)).unwrap();

        assert_eq!(back.len(), tree.len());
        let root = back.node(NodeId::ROOT).unwrap().totals();
        assert_eq!(root.logical, 330);
        assert_eq!(root.files, 3);
        assert_eq!(root.directories, 2);
        let names: Vec<String> = back
            .children(NodeId::ROOT)
            .map(|id| back.node(id).unwrap().name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["docs", "top.bin"], "sibling order is kept");
    }

    #[test]
    fn wrong_magic_is_rejected() {
        assert!(read_tree(Cursor::new(b"NOTATREE________".to_vec())).is_err());
    }
}
