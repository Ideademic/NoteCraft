//! Writing a fresh revision store ([MS-ONESTORE]): one revision per object space (plus one per
//! non-default context, [`crate::ContextRevision`]), one transaction, no free chunk list, an exact `cbExpectedFileLength`.
//!
//! Layout: the header, then every block (file node list fragments, property sets, file data
//! store objects, the transaction log) appended at 8-byte aligned offsets. Each file node list is
//! a single fragment. `.one` object spaces keep their objects in one object group
//! (§2.1.13); `.onetoc2` object spaces declare them in the revision manifest (§2.1.9).

use std::collections::{HashMap, HashSet};

use notecraft_doc::Guid;

use crate::bytes::{WriteLe, utf16le};
use crate::filenode::{fragment, id, node};
use crate::guid::consts;
use crate::header::{Fcr, FileKind, HEADER_SIZE, Header};
use crate::propset::{self, PropertyValue, Refs};
use crate::read::{JCID_IS_FILE_DATA, JCID_IS_READ_ONLY};
use crate::{CompactId, Error, ExtendedGuid, Object, ObjectSpace, Result, Store, crc, md5};

struct Out {
    kind: FileKind,
    buf: Vec<u8>,
    next_list: u32,
    /// (FileNodeListID, node count) for the transaction log.
    lists: Vec<(u32, u32)>,
    /// File data store objects: (guidReference, location).
    files: Vec<([u8; 16], Fcr)>,
    /// File data already written, by the address of its shared bytes.
    shared: HashMap<usize, Guid>,
}

impl Out {
    fn alloc(&mut self, bytes: &[u8]) -> Fcr {
        while !self.buf.len().is_multiple_of(8) {
            self.buf.push(0);
        }
        let at = self.buf.len() as u64;
        self.buf.extend_from_slice(bytes);
        Fcr::new(at, bytes.len() as u64)
    }

    fn list_with_id(&mut self, list_id: u32, nodes: &[Vec<u8>]) -> Fcr {
        self.lists.push((list_id, nodes.len() as u32));
        let frag = fragment(list_id, nodes);
        self.alloc(&frag)
    }

    fn list(&mut self, nodes: &[Vec<u8>]) -> Fcr {
        let list_id = self.next_list;
        self.next_list += 1;
        self.list_with_id(list_id, nodes)
    }
}

/// The global id table of one object group / revision: GUID → index.
#[derive(Default)]
struct Table {
    order: Vec<[u8; 16]>,
    index: HashMap<[u8; 16], u32>,
}

impl Table {
    fn add(&mut self, g: ExtendedGuid) -> Result<()> {
        if g.is_nil() {
            return Ok(());
        }
        if g.n > 0xFF {
            return Err(Error::Limit(format!("id counter {} does not fit a CompactID", g.n)));
        }
        if !self.index.contains_key(&g.guid) {
            let i = self.order.len() as u32;
            if i >= 0xFF_FFFF {
                return Err(Error::Limit("too many GUIDs for one global id table".into()));
            }
            self.index.insert(g.guid, i);
            self.order.push(g.guid);
        }
        Ok(())
    }

    fn compact(&self, g: ExtendedGuid) -> Result<CompactId> {
        if g.is_nil() {
            return Ok(CompactId::default());
        }
        let i = self.index.get(&g.guid).ok_or_else(|| Error::Corrupt("reference to an id missing from the global id table".into()))?;
        Ok(CompactId { n: g.n as u8, guid_index: *i })
    }

    fn nodes(&self, start: u16) -> Result<Vec<Vec<u8>>> {
        let mut out = Vec::with_capacity(self.order.len() + 2);
        out.push(node(start, 0, None, if start == id::GLOBAL_ID_TABLE_START { &[0] } else { &[] })?);
        for (i, g) in self.order.iter().enumerate() {
            let mut d = Vec::with_capacity(20);
            d.put_u32(i as u32);
            d.put(g);
            out.push(node(id::GLOBAL_ID_TABLE_ENTRY, 0, None, &d)?);
        }
        out.push(node(id::GLOBAL_ID_TABLE_END, 0, None, &[])?);
        Ok(out)
    }
}

/// Every id an object's properties mention (objects, object spaces, contexts).
fn all_ids(set: &propset::PropertySet, out: &mut Vec<ExtendedGuid>, depth: usize) {
    if depth > propset::MAX_DEPTH {
        return;
    }
    for (_, v) in &set.props {
        match v {
            PropertyValue::Ids(ids) => out.extend(ids.iter().copied()),
            PropertyValue::Sets(sets) => sets.iter().for_each(|s| all_ids(s, out, depth + 1)),
            _ => {}
        }
    }
}

/// Reference counts (§2.1.5): distinct referencing objects, plus one for roots.
fn ref_counts(objects: &[Object], roots: &[(u32, ExtendedGuid)]) -> HashMap<ExtendedGuid, u32> {
    let mut counts: HashMap<ExtendedGuid, u32> = HashMap::new();
    for o in objects {
        let distinct: HashSet<ExtendedGuid> = o.props.object_refs().into_iter().collect();
        for r in distinct {
            *counts.entry(r).or_default() += 1;
        }
    }
    for (_, r) in roots {
        *counts.entry(*r).or_default() += 1;
    }
    counts
}

/// Writes a store as a desktop-format file.
pub fn write(store: &Store) -> Result<Vec<u8>> {
    if !store.spaces.iter().any(|s| s.id == store.root) {
        return Err(Error::Corrupt("the root object space is missing".into()));
    }
    let mut out =
        Out { kind: store.kind, buf: vec![0u8; HEADER_SIZE], next_list: 0x11, lists: Vec::new(), files: Vec::new(), shared: HashMap::new() };
    let mut root_nodes = Vec::with_capacity(store.spaces.len() + 2);
    let mut seen = HashSet::new();
    for space in &store.spaces {
        if space.id.is_nil() || !seen.insert(space.id) {
            return Err(Error::Corrupt("object space ids must be unique and not nil".into()));
        }
        let osml = write_space(&mut out, space)?;
        root_nodes.push(node(id::OBJECT_SPACE_MANIFEST_LIST_REFERENCE, 2, Some(osml), &space.id.to_bytes())?);
    }
    root_nodes.push(node(id::OBJECT_SPACE_MANIFEST_ROOT, 0, None, &store.root.to_bytes())?);
    if !out.files.is_empty() {
        let files = std::mem::take(&mut out.files);
        let nodes = files.iter().map(|(g, fcr)| node(id::FILE_DATA_STORE_OBJECT_REFERENCE, 1, Some(*fcr), g)).collect::<Result<Vec<_>>>()?;
        let fcr = out.list(&nodes);
        root_nodes.push(node(id::FILE_DATA_STORE_LIST_REFERENCE, 2, Some(fcr), &[])?);
    }
    let root = out.list_with_id(0x10, &root_nodes);
    // The root list first in the log, then the others in the order they were written.
    out.lists.rotate_right(1);
    let log_bytes = crate::txlog::write_fragment(store.kind, &out.lists);
    let log = out.alloc(&log_bytes);
    while !out.buf.len().is_multiple_of(8) {
        out.buf.push(0);
    }
    let header = Header {
        kind: store.kind,
        file_guid: store.file_id.0,
        ancestor: store.ancestor.0,
        crc_name: crc::name_crc(&store.file_name),
        transactions: 1,
        fcr_hashed_chunk_list: Fcr::ZERO,
        fcr_transaction_log: log,
        fcr_file_node_list_root: root,
        fcr_free_chunk_list: Fcr::NIL,
        expected_length: out.buf.len() as u64,
        file_version: Guid::new().0,
        file_version_generation: 1,
        deny_read_file_version: Guid::new().0,
    };
    let head = header.to_bytes();
    if let Some(slot) = out.buf.get_mut(..HEADER_SIZE) {
        slot.copy_from_slice(&head);
    }
    Ok(out.buf)
}

/// Writes one object space; returns its object space manifest list. The default context's
/// revision comes first, then one self-contained revision per [`ObjectSpace::contexts`] entry
/// (`.one` only), each with its own object group.
fn write_space(out: &mut Out, space: &ObjectSpace) -> Result<Fcr> {
    let mut rml = vec![node(id::REVISION_MANIFEST_LIST_START, 0, None, &[space.id.to_bytes().as_slice(), &0u32.to_le_bytes()].concat())?];
    rml.extend(revision(out, &space.objects, &space.roots, ExtendedGuid::NIL)?);
    if out.kind == FileKind::Section {
        let mut seen = HashSet::new();
        for c in &space.contexts {
            if c.context.is_nil() || !seen.insert(c.context) {
                return Err(Error::Corrupt("revision contexts must be unique and not nil".into()));
            }
            rml.extend(revision(out, &c.objects, &c.roots, c.context)?);
        }
    }
    let rml_fcr = out.list(&rml);
    let osml = [
        node(id::OBJECT_SPACE_MANIFEST_LIST_START, 0, None, &space.id.to_bytes())?,
        node(id::REVISION_MANIFEST_LIST_REFERENCE, 2, Some(rml_fcr), &[])?,
    ];
    Ok(out.list(&osml))
}

/// One revision manifest (start … end) holding `objects`, labelled with revision role 1 and
/// context `ctx` (nil: the default context, `RevisionManifestStart6FND`; otherwise
/// `RevisionManifestStart7FND` with its `gctxid`).
fn revision(out: &mut Out, objects: &[Object], roots: &[(u32, ExtendedGuid)], ctx: ExtendedGuid) -> Result<Vec<Vec<u8>>> {
    let mut table = Table::default();
    let mut ids = Vec::new();
    for o in objects {
        ids.push(o.id);
        all_ids(&o.props, &mut ids, 0);
    }
    for (_, r) in roots {
        ids.push(*r);
    }
    for g in ids {
        table.add(g)?;
    }
    let counts = ref_counts(objects, roots);
    let mut decls = Vec::with_capacity(objects.len());
    let mut group_crc = 0u32;
    for o in objects {
        let c_ref = counts.get(&o.id).copied().unwrap_or(0);
        group_crc = crc::crc32_update(group_crc, &c_ref.to_le_bytes());
        decls.push(declaration(out, o, &table, c_ref)?);
    }
    let rid = ExtendedGuid::from_guid(Guid::new(), 1);
    let mut rml = Vec::new();
    match out.kind {
        FileKind::Section => {
            let ogid = ExtendedGuid::from_guid(Guid::new(), 1);
            let mut group = vec![node(id::OBJECT_GROUP_START, 0, None, &ogid.to_bytes())?];
            group.extend(table.nodes(id::GLOBAL_ID_TABLE_START2)?);
            group.extend(decls);
            group.push(node(id::OBJECT_GROUP_END, 0, None, &[])?);
            let group_fcr = out.list(&group);
            let mut start = Vec::with_capacity(66);
            start.put(&rid.to_bytes());
            start.put(&ExtendedGuid::NIL.to_bytes());
            start.put_u32(1); // RevisionRole: the active content
            start.put_u16(0); // odcsDefault: not encrypted
            if ctx.is_nil() {
                rml.push(node(id::REVISION_MANIFEST_START6, 0, None, &start)?);
            } else {
                start.put(&ctx.to_bytes());
                rml.push(node(id::REVISION_MANIFEST_START7, 0, None, &start)?);
            }
            rml.push(node(id::OBJECT_GROUP_LIST_REFERENCE, 2, Some(group_fcr), &ogid.to_bytes())?);
            // ObjectInfoDependencyOverrideData with no overrides (§2.6.10), inline (ref nil).
            let mut ovr = Vec::with_capacity(12);
            ovr.put_u32(0);
            ovr.put_u32(0);
            ovr.put_u32(group_crc);
            rml.push(node(id::OBJECT_INFO_DEPENDENCY_OVERRIDES, 1, Some(Fcr::NIL), &ovr)?);
            for (role, oid) in roots {
                rml.push(node(id::ROOT_OBJECT_REFERENCE3, 0, None, &[oid.to_bytes().as_slice(), &role.to_le_bytes()].concat())?);
            }
        }
        FileKind::Toc => {
            let mut start = Vec::with_capacity(54);
            start.put(&rid.to_bytes());
            start.put(&ExtendedGuid::NIL.to_bytes());
            start.put_u64(0); // timeCreation
            start.put_u32(1);
            start.put_u16(0);
            rml.push(node(id::REVISION_MANIFEST_START4, 0, None, &start)?);
            rml.extend(table.nodes(id::GLOBAL_ID_TABLE_START)?);
            rml.extend(decls);
            for (role, oid) in roots {
                let c = table.compact(*oid)?;
                rml.push(node(id::ROOT_OBJECT_REFERENCE2, 0, None, &[c.to_u32().to_le_bytes(), role.to_le_bytes()].concat())?);
            }
        }
    }
    rml.push(node(id::REVISION_MANIFEST_END, 0, None, &[])?);
    Ok(rml)
}

/// The declaration node for one object (its data written first).
fn declaration(out: &mut Out, o: &Object, table: &Table, c_ref: u32) -> Result<Vec<u8>> {
    let oid = table.compact(o.id)?.to_u32();
    if o.jcid & JCID_IS_FILE_DATA != 0 || o.file_data.is_some() {
        return file_data_declaration(out, o, oid, c_ref);
    }
    let (data, refs) = propset::encode(&o.props, &|g| table.compact(g))?;
    let fcr = out.alloc(&data);
    let small = c_ref <= 0xFF;
    match out.kind {
        FileKind::Toc => {
            let mut d = Vec::with_capacity(14);
            d.put_u32(oid);
            d.put_u16((o.jcid & 0x3FF) as u16); // jci; odcs 0
            d.put_u32(u32::from(refs.oids)); // fHasOidReferences; fHasOsidReferences MUST be 0
            if small {
                d.put_u8(c_ref as u8);
            } else {
                d.put_u32(c_ref);
            }
            let nid = if small { id::OBJECT_DECLARATION_WITH_REF_COUNT } else { id::OBJECT_DECLARATION_WITH_REF_COUNT2 };
            node(nid, 1, Some(fcr), &d)
        }
        FileKind::Section => {
            let mut d = Vec::with_capacity(30);
            d.put_u32(oid);
            d.put_u32(o.jcid);
            d.put_u8(flags(refs));
            if small {
                d.put_u8(c_ref as u8);
            } else {
                d.put_u32(c_ref);
            }
            let read_only = o.jcid & JCID_IS_READ_ONLY != 0;
            if read_only {
                d.put(&md5::md5(&data));
            }
            let nid = match (read_only, small) {
                (true, true) => id::READ_ONLY_OBJECT_DECLARATION2_REF_COUNT,
                (true, false) => id::READ_ONLY_OBJECT_DECLARATION2_LARGE_REF_COUNT,
                (false, true) => id::OBJECT_DECLARATION2_REF_COUNT,
                (false, false) => id::OBJECT_DECLARATION2_LARGE_REF_COUNT,
            };
            node(nid, 1, Some(fcr), &d)
        }
    }
}

fn flags(refs: Refs) -> u8 {
    u8::from(refs.oids) | (u8::from(refs.osids) << 1)
}

/// ObjectDeclarationFileData3(Large)RefCountFND (§2.5.27) plus its FileDataStoreObject.
fn file_data_declaration(out: &mut Out, o: &Object, oid: u32, c_ref: u32) -> Result<Vec<u8>> {
    if out.kind == FileKind::Toc {
        return Err(Error::Corrupt("file data objects can't be stored in a .onetoc2".into()));
    }
    // The same bytes (one `Arc`: a picture shared by a page and its versions) are stored once.
    let key = o.file_data.as_ref().map(|a| std::sync::Arc::as_ptr(a) as usize);
    let guid = match key.and_then(|k| out.shared.get(&k).copied()) {
        Some(g) => g,
        None => {
            let data = o.file_data.as_deref().map(Vec::as_slice).unwrap_or_default();
            let guid = Guid::new();
            let mut fdso = Vec::with_capacity(data.len() + 64);
            fdso.put(&consts::FILE_DATA_HEADER);
            fdso.put_u64(data.len() as u64);
            fdso.put_u32(0);
            fdso.put_u64(0);
            fdso.put(data);
            while (fdso.len() + 16) % 8 != 0 {
                fdso.push(0);
            }
            fdso.put(&consts::FILE_DATA_FOOTER);
            let fcr = out.alloc(&fdso);
            out.files.push((guid.0, fcr));
            if let Some(k) = key {
                out.shared.insert(k, guid);
            }
            guid
        }
    };
    let reference = format!("<ifndf>{guid}");
    let ext = o.file_ext.clone().unwrap_or_default();
    let mut d = Vec::new();
    d.put_u32(oid);
    d.put_u32(o.jcid);
    let small = c_ref <= 0xFF;
    if small {
        d.put_u8(c_ref as u8);
    } else {
        d.put_u32(c_ref);
    }
    for s in [&reference, &ext] {
        d.put_u32(s.encode_utf16().count() as u32);
        d.put(&utf16le(s));
    }
    let nid = if small { id::OBJECT_DECLARATION_FILE_DATA3_REF_COUNT } else { id::OBJECT_DECLARATION_FILE_DATA3_LARGE_REF_COUNT };
    node(nid, 0, None, &d)
}
