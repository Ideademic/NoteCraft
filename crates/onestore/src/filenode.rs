//! File node lists, their fragments and file node records ([MS-ONESTORE] §2.4, §2.5).

use std::collections::HashSet;

use crate::bytes::{Reader, WriteLe, slice};
use crate::header::Fcr;
use crate::txlog::TransactionLog;
use crate::{Error, Result};

/// `FileNodeListHeader.uintMagic`.
pub const LIST_MAGIC: u64 = 0xA456_7AB1_F5F7_F4C4;
/// `FileNodeListFragment.footer`.
pub const LIST_FOOTER: u64 = 0x8BC2_15C3_8233_BA4B;
/// Fragment header (16) plus `nextFragment` (12) and footer (8).
pub const FRAGMENT_OVERHEAD: usize = 16 + 12 + 8;

/// Most fragments followed in one list (hostile files can loop).
const MAX_FRAGMENTS_PER_LIST: usize = 100_000;

/// FileNodeID values ([MS-ONESTORE] §2.4.3).
pub mod id {
    pub const OBJECT_SPACE_MANIFEST_ROOT: u16 = 0x004;
    pub const OBJECT_SPACE_MANIFEST_LIST_REFERENCE: u16 = 0x008;
    pub const OBJECT_SPACE_MANIFEST_LIST_START: u16 = 0x00C;
    pub const REVISION_MANIFEST_LIST_REFERENCE: u16 = 0x010;
    pub const REVISION_MANIFEST_LIST_START: u16 = 0x014;
    pub const REVISION_MANIFEST_START4: u16 = 0x01B;
    pub const REVISION_MANIFEST_END: u16 = 0x01C;
    pub const REVISION_MANIFEST_START6: u16 = 0x01E;
    pub const REVISION_MANIFEST_START7: u16 = 0x01F;
    pub const GLOBAL_ID_TABLE_START: u16 = 0x021;
    pub const GLOBAL_ID_TABLE_START2: u16 = 0x022;
    pub const GLOBAL_ID_TABLE_ENTRY: u16 = 0x024;
    pub const GLOBAL_ID_TABLE_ENTRY2: u16 = 0x025;
    pub const GLOBAL_ID_TABLE_ENTRY3: u16 = 0x026;
    pub const GLOBAL_ID_TABLE_END: u16 = 0x028;
    pub const OBJECT_DECLARATION_WITH_REF_COUNT: u16 = 0x02D;
    pub const OBJECT_DECLARATION_WITH_REF_COUNT2: u16 = 0x02E;
    pub const OBJECT_REVISION_WITH_REF_COUNT: u16 = 0x041;
    pub const OBJECT_REVISION_WITH_REF_COUNT2: u16 = 0x042;
    pub const ROOT_OBJECT_REFERENCE2: u16 = 0x059;
    pub const ROOT_OBJECT_REFERENCE3: u16 = 0x05A;
    pub const REVISION_ROLE_DECLARATION: u16 = 0x05C;
    pub const REVISION_ROLE_AND_CONTEXT_DECLARATION: u16 = 0x05D;
    pub const OBJECT_DECLARATION_FILE_DATA3_REF_COUNT: u16 = 0x072;
    pub const OBJECT_DECLARATION_FILE_DATA3_LARGE_REF_COUNT: u16 = 0x073;
    pub const OBJECT_DATA_ENCRYPTION_KEY_V2: u16 = 0x07C;
    pub const OBJECT_INFO_DEPENDENCY_OVERRIDES: u16 = 0x084;
    pub const DATA_SIGNATURE_GROUP_DEFINITION: u16 = 0x08C;
    pub const FILE_DATA_STORE_LIST_REFERENCE: u16 = 0x090;
    pub const FILE_DATA_STORE_OBJECT_REFERENCE: u16 = 0x094;
    pub const OBJECT_DECLARATION2_REF_COUNT: u16 = 0x0A4;
    pub const OBJECT_DECLARATION2_LARGE_REF_COUNT: u16 = 0x0A5;
    pub const OBJECT_GROUP_LIST_REFERENCE: u16 = 0x0B0;
    pub const OBJECT_GROUP_START: u16 = 0x0B4;
    pub const OBJECT_GROUP_END: u16 = 0x0B8;
    pub const HASHED_CHUNK_DESCRIPTOR2: u16 = 0x0C2;
    pub const READ_ONLY_OBJECT_DECLARATION2_REF_COUNT: u16 = 0x0C4;
    pub const READ_ONLY_OBJECT_DECLARATION2_LARGE_REF_COUNT: u16 = 0x0C5;
    pub const CHUNK_TERMINATOR: u16 = 0x0FF;
}

/// One file node: its type, the `fnd` bytes after the header and, for base types 1 and 2, the
/// chunk reference those bytes start with (already decompressed) and the bytes after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileNode<'a> {
    pub id: u16,
    pub base_type: u8,
    /// The whole `fnd` field.
    pub fnd: &'a [u8],
    /// The chunk reference (base types 1 and 2).
    pub fcr: Option<Fcr>,
    /// `fnd` after the chunk reference (the whole `fnd` for base type 0).
    pub rest: &'a [u8],
}

impl<'a> FileNode<'a> {
    /// Parses one file node at the start of `data`; returns it and its size.
    pub fn parse(data: &'a [u8]) -> Result<(FileNode<'a>, usize)> {
        let mut r = Reader::new(data);
        let h = r.u32()?;
        let id = (h & 0x3FF) as u16;
        let size = ((h >> 10) & 0x1FFF) as usize;
        let stp_format = ((h >> 23) & 0x3) as u8;
        let cb_format = ((h >> 25) & 0x3) as u8;
        let base_type = ((h >> 27) & 0xF) as u8;
        if size < 4 {
            return Err(Error::Corrupt(format!("file node {id:#x}: size {size} too small")));
        }
        let fnd = data.get(4..size).ok_or_else(|| Error::Corrupt(format!("file node {id:#x}: past the end of its fragment")))?;
        let (fcr, rest) = if base_type == 1 || base_type == 2 {
            let mut fr = Reader::new(fnd);
            let fcr = read_node_fcr(&mut fr, stp_format, cb_format)?;
            (Some(fcr), fr.rest())
        } else {
            (None, fnd)
        };
        Ok((FileNode { id, base_type, fnd, fcr, rest }, size))
    }

    /// The chunk reference, or an error for nodes that must have one.
    pub fn chunk(&self) -> Result<Fcr> {
        self.fcr.ok_or_else(|| Error::Corrupt(format!("file node {:#x}: missing chunk reference", self.id)))
    }
}

/// FileNodeChunkReference (§2.2.4.2) in the formats the node header gives.
fn read_node_fcr(r: &mut Reader<'_>, stp_format: u8, cb_format: u8) -> Result<Fcr> {
    let (stp_raw, stp_all_ones, stp_scale) = match stp_format {
        0 => {
            let v = r.u64()?;
            (v, v == u64::MAX, 1)
        }
        1 => {
            let v = r.u32()?;
            (u64::from(v), v == u32::MAX, 1)
        }
        2 => {
            let v = r.u16()?;
            (u64::from(v), v == u16::MAX, 8)
        }
        _ => {
            let v = r.u32()?;
            (u64::from(v), v == u32::MAX, 8)
        }
    };
    let (cb_raw, cb_scale) = match cb_format {
        0 => (u64::from(r.u32()?), 1),
        1 => (r.u64()?, 1),
        2 => (u64::from(r.u8()?), 8),
        _ => (u64::from(r.u16()?), 8),
    };
    if stp_all_ones && cb_raw == 0 {
        return Ok(Fcr::NIL);
    }
    Ok(Fcr::new(stp_raw.saturating_mul(stp_scale), cb_raw.saturating_mul(cb_scale)))
}

/// Shared limits while reading one file: lists already visited (the fragments of all lists must
/// form a tree) and a budget of nodes.
#[derive(Debug)]
pub struct ListReader<'a> {
    pub file: &'a [u8],
    pub log: &'a TransactionLog,
    visited: HashSet<u64>,
    nodes_left: usize,
}

impl<'a> ListReader<'a> {
    pub fn new(file: &'a [u8], log: &'a TransactionLog, max_nodes: usize) -> ListReader<'a> {
        ListReader { file, log, visited: HashSet::new(), nodes_left: max_nodes }
    }

    /// Reads the file node list whose first fragment `first` points at: its committed nodes,
    /// in order, across every fragment.
    pub fn read(&mut self, first: Fcr) -> Result<Vec<FileNode<'a>>> {
        let mut nodes = Vec::new();
        let mut fcr = first;
        let mut list_id = None;
        let mut committed: Option<usize> = None;
        let mut seq = 0u32;
        for _ in 0..MAX_FRAGMENTS_PER_LIST {
            if !fcr.is_some() {
                return Ok(nodes);
            }
            if !self.visited.insert(fcr.stp) {
                return Err(Error::Corrupt("file node lists do not form a tree (fragment reused)".into()));
            }
            let frag = slice(self.file, fcr.stp, fcr.cb, "file node list fragment")?;
            let body_end =
                frag.len().checked_sub(20).filter(|&e| e >= 16).ok_or_else(|| Error::Corrupt("file node list fragment too small".into()))?;
            let mut r = Reader::new(frag);
            if r.u64()? != LIST_MAGIC {
                return Err(Error::Corrupt("file node list fragment: bad magic".into()));
            }
            let id = r.u32()?;
            let n_seq = r.u32()?;
            if *list_id.get_or_insert(id) != id || n_seq != seq {
                return Err(Error::Corrupt("file node list fragments out of order".into()));
            }
            if committed.is_none() {
                committed = self.log.count(id).map(|c| c as usize);
            }
            let tail = frag.get(body_end..).unwrap_or_default();
            let mut tr = Reader::new(tail);
            let next = Fcr::read64x32(&mut tr)?;
            if tr.u64()? != LIST_FOOTER {
                return Err(Error::Corrupt("file node list fragment: bad footer".into()));
            }
            let body = frag.get(16..body_end).unwrap_or_default();
            let ended = self.read_fragment(body, committed, &mut nodes)?;
            if ended || committed.is_some_and(|c| nodes.len() >= c) {
                return Ok(nodes);
            }
            fcr = next;
            seq = seq.wrapping_add(1);
        }
        Err(Error::Limit("file node list has too many fragments".into()))
    }

    /// Reads one fragment's nodes into `nodes`. Returns true when the list ends here (a stream
    /// that stops without a ChunkTerminatorFND, in a list the transaction log doesn't cover).
    fn read_fragment(&mut self, body: &'a [u8], committed: Option<usize>, nodes: &mut Vec<FileNode<'a>>) -> Result<bool> {
        let mut pos = 0usize;
        loop {
            if committed.is_some_and(|c| nodes.len() >= c) {
                return Ok(true);
            }
            let rest = body.get(pos..).unwrap_or_default();
            if rest.len() < 4 {
                return Ok(committed.is_none());
            }
            let header = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
            if header == 0 && committed.is_none() {
                // Zero padding after the last node of an uncommitted-count list.
                return Ok(true);
            }
            let (node, size) = FileNode::parse(rest)?;
            if node.id == id::CHUNK_TERMINATOR {
                return Ok(false);
            }
            self.nodes_left = self.nodes_left.checked_sub(1).ok_or_else(|| Error::Limit("too many file nodes".into()))?;
            nodes.push(node);
            pos += size;
        }
    }
}

/// Builds file nodes for the writer. Chunk references are written uncompressed: an 8-byte
/// `stp` (StpFormat 0) and a 4-byte `cb` (CbFormat 0).
pub fn node(id: u16, base_type: u8, fcr: Option<Fcr>, data: &[u8]) -> Result<Vec<u8>> {
    let mut fnd = Vec::with_capacity(12 + data.len());
    if let Some(f) = fcr {
        f.write64x32(&mut fnd);
    }
    fnd.put(data);
    let size = 4 + fnd.len();
    if size > 0x1FFF {
        return Err(Error::Limit(format!("file node {id:#x} is {size} bytes (most is 8191)")));
    }
    // StpFormat 0 and CbFormat 0 for references; nodes without one use the values OneNote
    // writes (StpFormat 1, CbFormat 0), which readers ignore. Reserved bit 31 is 1.
    let stp_format: u32 = if fcr.is_some() { 0 } else { 1 };
    let header = u32::from(id & 0x3FF) | ((size as u32) << 10) | (stp_format << 23) | (u32::from(base_type & 0xF) << 27) | (1 << 31);
    let mut out = Vec::with_capacity(size);
    out.put_u32(header);
    out.put(&fnd);
    Ok(out)
}

/// One whole-list fragment: header, the nodes, a nil `nextFragment` and the footer.
pub fn fragment(list_id: u32, nodes: &[Vec<u8>]) -> Vec<u8> {
    let body: usize = nodes.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(FRAGMENT_OVERHEAD + body);
    out.put_u64(LIST_MAGIC);
    out.put_u32(list_id);
    out.put_u32(0);
    for n in nodes {
        out.put(n);
    }
    Fcr::NIL.write64x32(&mut out);
    out.put_u64(LIST_FOOTER);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_example_headers_decode() {
        // [MS-ONESTORE] §3.3: 0x95001C10 is a RevisionManifestListReferenceFND of 7 bytes with a
        // compressed 2-byte stp and 1-byte cb.
        let mut b = 0x9500_1C10u32.to_le_bytes().to_vec();
        b.extend_from_slice(&0x01A4u16.to_le_bytes());
        b.push(0x24);
        let (n, size) = FileNode::parse(&b).unwrap();
        assert_eq!((n.id, n.base_type, size), (id::REVISION_MANIFEST_LIST_REFERENCE, 2, 7));
        assert_eq!(n.fcr, Some(Fcr::new(0xD20, 0x120)));
        // 0x8080101C: RevisionManifestEndFND, 4 bytes, no data.
        let end = 0x8080_101Cu32.to_le_bytes();
        let (n, size) = FileNode::parse(&end).unwrap();
        assert_eq!((n.id, n.base_type, size, n.fnd.len()), (id::REVISION_MANIFEST_END, 0, 4, 0));
    }

    #[test]
    fn written_nodes_parse_back() {
        let raw = node(id::OBJECT_GROUP_LIST_REFERENCE, 2, Some(Fcr::new(0x1234, 0x56)), &[9; 20]).unwrap();
        let (n, size) = FileNode::parse(&raw).unwrap();
        assert_eq!(size, raw.len());
        assert_eq!(n.fcr, Some(Fcr::new(0x1234, 0x56)));
        assert_eq!(n.rest, &[9; 20]);
        assert!(node(1, 0, None, &[0; 9000]).is_err());
    }

    #[test]
    fn list_round_trip_and_loops() {
        let nodes = vec![node(id::GLOBAL_ID_TABLE_START2, 0, None, &[]).unwrap(), node(id::GLOBAL_ID_TABLE_END, 0, None, &[]).unwrap()];
        let frag = fragment(0x10, &nodes);
        let mut log = TransactionLog::default();
        log.counts.insert(0x10, 2);
        let mut lr = ListReader::new(&frag, &log, 100);
        let got = lr.read(Fcr::new(0, frag.len() as u64)).unwrap();
        assert_eq!(got.iter().map(|n| n.id).collect::<Vec<_>>(), vec![id::GLOBAL_ID_TABLE_START2, id::GLOBAL_ID_TABLE_END]);
        // Reading the same fragment again is a loop.
        assert!(lr.read(Fcr::new(0, frag.len() as u64)).is_err());
        // Without a log entry the list is read up to the end of its stream.
        let empty = TransactionLog::default();
        assert_eq!(ListReader::new(&frag, &empty, 100).read(Fcr::new(0, frag.len() as u64)).unwrap().len(), 2);
        // The node budget is enforced.
        assert!(ListReader::new(&frag, &empty, 1).read(Fcr::new(0, frag.len() as u64)).is_err());
    }
}
