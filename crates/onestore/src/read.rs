//! Reading a revision store: root file node list → object spaces → the current revision of each
//! (default context, revision role 1) → its objects ([MS-ONESTORE] §2.1, §2.5).

use std::collections::HashMap;
use std::sync::Arc;

use notecraft_doc::Guid;

use crate::bytes::{Reader, from_utf16le, slice};
use crate::filenode::{FileNode, ListReader, id};
use crate::guid::consts;
use crate::header::{Fcr, Header};
use crate::txlog::TransactionLog;
use crate::{CompactId, Error, ExtendedGuid, Object, ObjectSpace, Result, Store, propset};

/// Most file nodes read from one file.
const MAX_NODES: usize = 4_000_000;
/// Most objects in one object space.
const MAX_OBJECTS: usize = 1_000_000;
/// Most revisions followed through `ridDependent`.
const MAX_REVISION_CHAIN: usize = 10_000;
/// Most object spaces in one file.
const MAX_SPACES: usize = 100_000;

/// JCID flag bits (§2.6.14).
pub const JCID_IS_BINARY: u32 = 1 << 16;
pub const JCID_IS_PROPERTY_SET: u32 = 1 << 17;
pub const JCID_IS_FILE_DATA: u32 = 1 << 19;
pub const JCID_IS_READ_ONLY: u32 = 1 << 20;

type GidTable = HashMap<u32, [u8; 16]>;

fn eguid(r: &mut Reader<'_>) -> Result<ExtendedGuid> {
    Ok(ExtendedGuid::from_bytes(&r.array::<20>()?))
}

fn resolve(table: &GidTable, c: CompactId) -> ExtendedGuid {
    match table.get(&c.guid_index) {
        Some(g) => ExtendedGuid::new(*g, u32::from(c.n)),
        None => ExtendedGuid::NIL,
    }
}

/// Parses a `.one` or `.onetoc2` file.
pub fn read(bytes: &[u8]) -> Result<Store> {
    let header = Header::parse(bytes)?;
    let log = TransactionLog::read(bytes, header.fcr_transaction_log, header.transactions)?;
    let mut ctx = Ctx { lr: ListReader::new(bytes, &log, MAX_NODES), files: HashMap::new(), warnings: Vec::new() };
    let root_nodes = ctx.lr.read(header.fcr_file_node_list_root)?;
    let mut spaces_refs = Vec::new();
    let mut root = None;
    let mut file_store = None;
    for n in &root_nodes {
        let mut r = Reader::new(n.rest);
        match n.id {
            id::OBJECT_SPACE_MANIFEST_LIST_REFERENCE => spaces_refs.push((eguid(&mut r)?, n.chunk()?)),
            id::OBJECT_SPACE_MANIFEST_ROOT => root = Some(eguid(&mut Reader::new(n.fnd))?),
            id::FILE_DATA_STORE_LIST_REFERENCE => file_store = Some(n.chunk()?),
            _ => {}
        }
    }
    if spaces_refs.len() > MAX_SPACES {
        return Err(Error::Limit("too many object spaces".into()));
    }
    let root = root.ok_or_else(|| Error::Corrupt("no root object space".into()))?;
    if let Some(fcr) = file_store {
        ctx.read_file_store(fcr)?;
    }
    let mut spaces = Vec::with_capacity(spaces_refs.len());
    for (gosid, fcr) in spaces_refs {
        match ctx.read_space(gosid, fcr) {
            Ok(s) => spaces.push(s),
            Err(e) if gosid == root => return Err(e),
            Err(e) => ctx.warnings.push(format!("object space skipped: {e}")),
        }
    }
    Ok(Store {
        kind: header.kind,
        file_id: Guid(header.file_guid),
        ancestor: Guid(header.ancestor),
        file_name: String::new(),
        root,
        spaces,
        warnings: ctx.warnings,
    })
}

/// A revision manifest found in a revision manifest list.
struct Manifest<'m, 'a> {
    rid: ExtendedGuid,
    dep: ExtendedGuid,
    nodes: &'m [FileNode<'a>],
}

/// Objects and roots of the revisions read so far (dependency first).
#[derive(Default)]
struct Revision {
    objects: Vec<Object>,
    index: HashMap<ExtendedGuid, usize>,
    roots: Vec<(u32, ExtendedGuid)>,
}

impl Revision {
    fn put(&mut self, obj: Object) -> Result<()> {
        if let Some(&i) = self.index.get(&obj.id) {
            if let Some(slot) = self.objects.get_mut(i) {
                *slot = obj;
            }
            return Ok(());
        }
        if self.objects.len() >= MAX_OBJECTS {
            return Err(Error::Limit("too many objects in an object space".into()));
        }
        self.index.insert(obj.id, self.objects.len());
        self.objects.push(obj);
        Ok(())
    }

    fn set_root(&mut self, role: u32, oid: ExtendedGuid) {
        if let Some(slot) = self.roots.iter_mut().find(|(r, _)| *r == role) {
            slot.1 = oid;
        } else {
            self.roots.push((role, oid));
        }
    }

    fn jcid_of(&self, oid: ExtendedGuid) -> Option<u32> {
        self.index.get(&oid).and_then(|&i| self.objects.get(i)).map(|o| o.jcid)
    }
}

struct Ctx<'a> {
    lr: ListReader<'a>,
    /// File data store objects by guidReference: the FileData bytes.
    files: HashMap<[u8; 16], Arc<Vec<u8>>>,
    warnings: Vec<String>,
}

impl<'a> Ctx<'a> {
    fn file(&self) -> &'a [u8] {
        self.lr.file
    }

    fn read_file_store(&mut self, fcr: Fcr) -> Result<()> {
        let nodes = self.lr.read(fcr)?;
        for n in nodes.iter().filter(|n| n.id == id::FILE_DATA_STORE_OBJECT_REFERENCE) {
            let guid = Reader::new(n.rest).guid()?;
            match file_data_store_object(self.file(), n.chunk()?) {
                Ok(data) => {
                    self.files.insert(guid, Arc::new(data.to_vec()));
                }
                Err(e) => self.warnings.push(format!("file data object skipped: {e}")),
            }
        }
        Ok(())
    }

    fn read_space(&mut self, gosid: ExtendedGuid, fcr: Fcr) -> Result<ObjectSpace> {
        let osml = self.lr.read(fcr)?;
        let rml_ref = osml
            .iter()
            .rfind(|n| n.id == id::REVISION_MANIFEST_LIST_REFERENCE)
            .ok_or_else(|| Error::Corrupt("object space without a revision manifest list".into()))?
            .chunk()?;
        let rml = self.lr.read(rml_ref)?;
        let (manifests, labels) = split_manifests(&rml)?;
        let target = labels
            .iter()
            .rev()
            .find(|((ctx, role), _)| ctx.is_nil() && *role == 1)
            .map(|(_, rid)| *rid)
            .or_else(|| manifests.last().map(|m| m.rid))
            .ok_or_else(|| Error::Corrupt("object space without revisions".into()))?;
        let chain = revision_chain(&manifests, target)?;
        let mut rev = Revision::default();
        let mut table = GidTable::new();
        for m in chain {
            table = self.apply_manifest(m, &table, &mut rev)?;
        }
        Ok(ObjectSpace { id: gosid, roots: rev.roots, objects: rev.objects })
    }

    /// Applies one revision manifest on top of `rev`; returns its global id table (used by the
    /// next revision's GlobalIdTableEntry2/3FNDX).
    fn apply_manifest(&mut self, m: &Manifest<'_, 'a>, dep_table: &GidTable, rev: &mut Revision) -> Result<GidTable> {
        let mut table = GidTable::new();
        let mut pending_roots: Vec<(CompactId, u32)> = Vec::new();
        for n in m.nodes {
            let mut r = Reader::new(n.rest);
            match n.id {
                id::REVISION_MANIFEST_START6 | id::REVISION_MANIFEST_START7 => {
                    let mut r = Reader::new(n.fnd);
                    r.skip(44)?;
                    if r.u16()? != 0 {
                        return Err(Error::Unsupported("password-protected section".into()));
                    }
                }
                id::OBJECT_DATA_ENCRYPTION_KEY_V2 => return Err(Error::Unsupported("password-protected section".into())),
                id::GLOBAL_ID_TABLE_START | id::GLOBAL_ID_TABLE_START2 => table.clear(),
                id::GLOBAL_ID_TABLE_ENTRY | id::GLOBAL_ID_TABLE_ENTRY2 | id::GLOBAL_ID_TABLE_ENTRY3 => table_entry(n, &mut table, dep_table)?,
                id::OBJECT_GROUP_LIST_REFERENCE => self.apply_group(n.chunk()?, rev)?,
                id::ROOT_OBJECT_REFERENCE3 => {
                    let mut r = Reader::new(n.fnd);
                    let oid = eguid(&mut r)?;
                    rev.set_root(r.u32()?, oid);
                }
                id::ROOT_OBJECT_REFERENCE2 => {
                    let mut r = Reader::new(n.fnd);
                    pending_roots.push((CompactId::from_u32(r.u32()?), r.u32()?));
                }
                _ if is_declaration(n.id) => self.declare(n, &mut r, &table, rev)?,
                _ => {}
            }
        }
        // RootObjectReference2FNDX may come before the table it refers to.
        let lookup = if table.is_empty() { dep_table } else { &table };
        for (c, role) in pending_roots {
            rev.set_root(role, resolve(lookup, c));
        }
        Ok(table)
    }

    fn apply_group(&mut self, fcr: Fcr, rev: &mut Revision) -> Result<()> {
        let nodes = self.lr.read(fcr)?;
        let mut table = GidTable::new();
        let empty = GidTable::new();
        for n in &nodes {
            let mut r = Reader::new(n.rest);
            match n.id {
                id::GLOBAL_ID_TABLE_START | id::GLOBAL_ID_TABLE_START2 => table.clear(),
                id::GLOBAL_ID_TABLE_ENTRY | id::GLOBAL_ID_TABLE_ENTRY2 | id::GLOBAL_ID_TABLE_ENTRY3 => table_entry(n, &mut table, &empty)?,
                _ if is_declaration(n.id) => self.declare(n, &mut r, &table, rev)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// One object declaration or revision node (§2.5.13–§2.5.30).
    fn declare(&mut self, n: &FileNode<'a>, r: &mut Reader<'a>, table: &GidTable, rev: &mut Revision) -> Result<()> {
        if matches!(n.id, id::OBJECT_DECLARATION_FILE_DATA3_REF_COUNT | id::OBJECT_DECLARATION_FILE_DATA3_LARGE_REF_COUNT) {
            return self.declare_file_data(n, table, rev);
        }
        let oid = resolve(table, CompactId::from_u32(r.u32()?));
        let jcid = match n.id {
            id::OBJECT_DECLARATION_WITH_REF_COUNT | id::OBJECT_DECLARATION_WITH_REF_COUNT2 => {
                let jci = u32::from(r.u16()? & 0x3FF);
                JCID_IS_PROPERTY_SET | jci
            }
            id::OBJECT_REVISION_WITH_REF_COUNT | id::OBJECT_REVISION_WITH_REF_COUNT2 => rev.jcid_of(oid).unwrap_or(JCID_IS_PROPERTY_SET | 1),
            _ => r.u32()?,
        };
        let props = if jcid & JCID_IS_PROPERTY_SET != 0 || jcid & 0xFFFF_0000 == 0 {
            let fcr = n.chunk()?;
            let data = slice(self.file(), fcr.stp, fcr.cb, "object property set")?;
            propset::decode(data, &|c| resolve(table, c))?
        } else {
            propset::PropertySet::default()
        };
        // Only JCID.index given: the other fields are implied (§2.1.5).
        let jcid = if jcid & 0xFFFF_0000 == 0 { jcid | JCID_IS_PROPERTY_SET } else { jcid };
        rev.put(Object { id: oid, jcid, props, file_data: None, file_ext: None })
    }

    fn declare_file_data(&mut self, n: &FileNode<'a>, table: &GidTable, rev: &mut Revision) -> Result<()> {
        let mut r = Reader::new(n.fnd);
        let oid = resolve(table, CompactId::from_u32(r.u32()?));
        let jcid = r.u32()?;
        if n.id == id::OBJECT_DECLARATION_FILE_DATA3_REF_COUNT {
            r.u8()?;
        } else {
            r.u32()?;
        }
        let reference = storage_string(&mut r)?;
        let ext = storage_string(&mut r)?;
        let file_data = if let Some(g) = reference.strip_prefix("<ifndf>") {
            let found = Guid::parse(g).and_then(|g| self.files.get(&g.0).cloned());
            if found.is_none() {
                self.warnings.push(format!("file data {g} not found"));
            }
            found
        } else {
            if !reference.starts_with("<invfdo>") {
                self.warnings.push(format!("file data kept outside the section ({reference}) not read"));
            }
            None
        };
        let file_ext = Some(ext).filter(|e| !e.is_empty());
        rev.put(Object { id: oid, jcid, props: propset::PropertySet::default(), file_data, file_ext })
    }
}

fn is_declaration(node_id: u16) -> bool {
    matches!(
        node_id,
        id::OBJECT_DECLARATION_WITH_REF_COUNT
            | id::OBJECT_DECLARATION_WITH_REF_COUNT2
            | id::OBJECT_REVISION_WITH_REF_COUNT
            | id::OBJECT_REVISION_WITH_REF_COUNT2
            | id::OBJECT_DECLARATION2_REF_COUNT
            | id::OBJECT_DECLARATION2_LARGE_REF_COUNT
            | id::READ_ONLY_OBJECT_DECLARATION2_REF_COUNT
            | id::READ_ONLY_OBJECT_DECLARATION2_LARGE_REF_COUNT
            | id::OBJECT_DECLARATION_FILE_DATA3_REF_COUNT
            | id::OBJECT_DECLARATION_FILE_DATA3_LARGE_REF_COUNT
    )
}

/// StringInStorageBuffer (§2.2.3).
fn storage_string(r: &mut Reader<'_>) -> Result<String> {
    let cch = r.u32()? as usize;
    let len = cch.checked_mul(2).ok_or_else(|| Error::Corrupt("string too long".into()))?;
    Ok(from_utf16le(r.bytes(len)?))
}

/// One entry of a global id table (§2.5.10–§2.5.12).
fn table_entry(n: &FileNode<'_>, table: &mut GidTable, dep: &GidTable) -> Result<()> {
    let mut r = Reader::new(n.fnd);
    match n.id {
        id::GLOBAL_ID_TABLE_ENTRY => {
            let index = r.u32()?;
            table.insert(index, r.guid()?);
        }
        id::GLOBAL_ID_TABLE_ENTRY2 => {
            let from = r.u32()?;
            let to = r.u32()?;
            if let Some(g) = dep.get(&from) {
                table.insert(to, *g);
            }
        }
        _ => {
            let from = r.u32()?;
            let count = r.u32()?;
            let to = r.u32()?;
            if count as usize > dep.len() {
                return Err(Error::Corrupt("global id table copies more entries than exist".into()));
            }
            for i in 0..count {
                if let Some(g) = dep.get(&from.wrapping_add(i)) {
                    table.insert(to.wrapping_add(i), *g);
                }
            }
        }
    }
    Ok(())
}

type Labels = Vec<((ExtendedGuid, u32), ExtendedGuid)>;

/// Splits a revision manifest list into manifests and the (context, role) → revision labels.
fn split_manifests<'m, 'a>(nodes: &'m [FileNode<'a>]) -> Result<(Vec<Manifest<'m, 'a>>, Labels)> {
    let mut manifests = Vec::new();
    let mut labels = Labels::new();
    let mut i = 0usize;
    while let Some(n) = nodes.get(i) {
        let mut r = Reader::new(n.fnd);
        match n.id {
            id::REVISION_MANIFEST_START4 | id::REVISION_MANIFEST_START6 | id::REVISION_MANIFEST_START7 => {
                let rid = eguid(&mut r)?;
                let dep = eguid(&mut r)?;
                if n.id == id::REVISION_MANIFEST_START4 {
                    r.skip(8)?; // timeCreation
                }
                let role = r.u32()?;
                let ctx = if n.id == id::REVISION_MANIFEST_START7 {
                    r.skip(2)?;
                    eguid(&mut r)?
                } else {
                    ExtendedGuid::NIL
                };
                let end = nodes.iter().skip(i).position(|x| x.id == id::REVISION_MANIFEST_END).map_or(nodes.len(), |p| i + p + 1);
                manifests.push(Manifest { rid, dep, nodes: nodes.get(i..end).unwrap_or_default() });
                labels.push(((ctx, role), rid));
                i = end;
                continue;
            }
            id::REVISION_ROLE_DECLARATION | id::REVISION_ROLE_AND_CONTEXT_DECLARATION => {
                let rid = eguid(&mut r)?;
                let role = r.u32()?;
                let ctx = if n.id == id::REVISION_ROLE_AND_CONTEXT_DECLARATION { eguid(&mut r)? } else { ExtendedGuid::NIL };
                labels.push(((ctx, role), rid));
            }
            _ => {}
        }
        i += 1;
    }
    Ok((manifests, labels))
}

/// The manifests that make up revision `target`: its dependencies first.
fn revision_chain<'v, 'm, 'a>(manifests: &'v [Manifest<'m, 'a>], target: ExtendedGuid) -> Result<Vec<&'v Manifest<'m, 'a>>> {
    let mut chain = Vec::new();
    let mut rid = target;
    while !rid.is_nil() {
        if chain.len() >= MAX_REVISION_CHAIN {
            return Err(Error::Limit("revision dependencies too deep (or circular)".into()));
        }
        let m = manifests.iter().rev().find(|m| m.rid == rid).ok_or_else(|| Error::Corrupt("revision depends on a missing revision".into()))?;
        chain.push(m);
        rid = m.dep;
    }
    chain.reverse();
    Ok(chain)
}

/// FileDataStoreObject (§2.6.13): the FileData bytes.
fn file_data_store_object(file: &[u8], fcr: Fcr) -> Result<&[u8]> {
    let data = slice(file, fcr.stp, fcr.cb, "file data store object")?;
    let mut r = Reader::new(data);
    if r.guid()? != consts::FILE_DATA_HEADER {
        return Err(Error::Corrupt("file data store object: bad header".into()));
    }
    let len = r.u64()?;
    r.skip(12)?; // unused, reserved
    let len = usize::try_from(len).map_err(|_| Error::Corrupt("file data too large".into()))?;
    r.bytes(len)
}
