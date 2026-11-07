//! `.onetoc2` notebook tables of contents ([MS-ONE] §2.1.15).
//!
//! MS-ONE names the table-of-contents properties (TOCEntryIndex_OidIndex, FolderChildFilename,
//! NotebookElementOrderingID…) but doesn't give their PropertyIDs, and the sample corpus has no
//! `.onetoc2`. So NoteCraft writes the entries in properties of its own (`NC_TOC_*`) next to the
//! documented FileIdentityGuid and the NotebookColor observed in section files, and reads real
//! OneNote tables of contents heuristically (entries are objects carrying a FileIdentityGuid and
//! a file-name-like string). A notebook folder is read from its files either way; the table of
//! contents only adds order, colours and the notebook's identity.

use notecraft_doc::{Color, Guid};
use notecraft_onestore::bytes::from_utf16le;
use notecraft_onestore::{ExtendedGuid, FileKind, PropertySet, PropertyValue, Store};

use crate::build::SpaceBuilder;
use crate::ids::{jcid, prop, role};
use crate::read::View;
use crate::util::{from_colorref, guid, string, to_colorref, wz_bytes};
use crate::{Error, NotebookToc, Result, TOC_FILE_NAME, TocEntry};

/// Most entries read from one table of contents.
const MAX_ENTRIES: usize = 10_000;
/// "No colour" in a NotebookColor.
const NO_COLOR: u32 = 0xFFFF_FFFF;

/// The store for a table of contents. `entry_ids` are the FileIdentityGuids of the entries (the
/// sections' and section groups' ids), in order.
pub fn toc_store(toc: &NotebookToc, entry_ids: &[Guid], ancestor: Guid) -> Store {
    let root_id = ExtendedGuid::from_guid(Guid::new(), 1);
    let mut b = SpaceBuilder::new(root_id);
    let mut entries = Vec::with_capacity(toc.entries.len());
    for (i, e) in toc.entries.iter().enumerate() {
        let (name, color, group) = match e {
            TocEntry::Section { file_name, color } => (file_name.as_str(), color.map_or(NO_COLOR, to_colorref), 0u8),
            TocEntry::Group { folder_name } => (folder_name.as_str(), NO_COLOR, 1u8),
        };
        let mut p = PropertySet::default();
        p.push(prop::FILE_IDENTITY_GUID, PropertyValue::Bytes(entry_ids.get(i).copied().unwrap_or_else(Guid::new).0.to_vec()))
            .push(prop::NC_TOC_FILE_NAME, wz_bytes(name))
            .push(prop::NOTEBOOK_COLOR, PropertyValue::U32(color))
            .push(prop::NC_TOC_IS_GROUP, PropertyValue::U8(group))
            .push(prop::NC_TOC_ORDER, PropertyValue::U32(i as u32));
        entries.push(b.add(jcid::TOC_CONTAINER, p));
    }
    let mut p = PropertySet::default();
    if !entries.is_empty() {
        p.push(prop::NC_TOC_ENTRIES, PropertyValue::Ids(entries));
    }
    p.push(prop::NOTEBOOK_COLOR, PropertyValue::U32(to_colorref(toc.color))).push(prop::NC_TOC_NAME, wz_bytes(&toc.name));
    let root = b.add(jcid::TOC_CONTAINER, p);
    b.root(role::CONTENT, root);
    Store {
        kind: FileKind::Toc,
        file_id: if toc.id.is_nil() { Guid::new() } else { toc.id },
        ancestor,
        file_name: TOC_FILE_NAME.to_owned(),
        root: root_id,
        spaces: vec![b.finish()],
        warnings: Vec::new(),
    }
}

/// Reads a table of contents store; also returns each entry's FileIdentityGuid.
pub fn toc(store: &Store) -> Result<(NotebookToc, Vec<Option<Guid>>)> {
    if store.kind != FileKind::Toc {
        return Err(Error::Format("not a notebook table of contents (.onetoc2)".into()));
    }
    let space = store.root_space().ok_or_else(|| Error::Format("root object space missing".into()))?;
    let v = View::new(space);
    let root = v.root(role::CONTENT);
    let color = root.and_then(|r| r.props.u32(prop::NOTEBOOK_COLOR)).and_then(from_colorref).unwrap_or(Color::NOTECRAFT);
    let name = root.and_then(|r| string(&r.props, prop::NC_TOC_NAME)).unwrap_or_default();
    let candidates: Vec<&PropertySet> = match root {
        Some(r) if r.props.has(prop::NC_TOC_ENTRIES) => v.children(&r.props, prop::NC_TOC_ENTRIES).into_iter().map(|o| &o.props).collect(),
        _ => space.objects.iter().filter(|o| Some(o.id) != space.root(role::CONTENT)).map(|o| &o.props).collect(),
    };
    let mut entries = Vec::new();
    let mut ids = Vec::new();
    for p in candidates.into_iter().take(MAX_ENTRIES) {
        let Some(e) = entry(p) else { continue };
        ids.push(guid(p, prop::FILE_IDENTITY_GUID));
        entries.push(e);
    }
    Ok((NotebookToc { id: store.file_id, name, color, entries }, ids))
}

fn entry(p: &PropertySet) -> Option<TocEntry> {
    let name = string(p, prop::NC_TOC_FILE_NAME).or_else(|| file_name_like(p))?;
    if name.trim().is_empty() || name.contains(['/', '\\']) || name == ".." {
        return None;
    }
    let group = match p.uint(prop::NC_TOC_IS_GROUP) {
        Some(g) => g != 0,
        None => !name.to_ascii_lowercase().ends_with(".one"),
    };
    if group {
        return Some(TocEntry::Group { folder_name: name });
    }
    let color = p.u32(prop::NOTEBOOK_COLOR).and_then(from_colorref);
    Some(TocEntry::Section { file_name: name, color })
}

/// For tables of contents NoteCraft didn't write: the first property that reads as a
/// null-terminated file name.
fn file_name_like(p: &PropertySet) -> Option<String> {
    if !p.has(prop::FILE_IDENTITY_GUID) {
        return None;
    }
    p.props.iter().find_map(|(id, v)| match v {
        PropertyValue::Bytes(b) if id.0 != prop::FILE_IDENTITY_GUID && b.len() >= 4 && b.len().is_multiple_of(2) && b.ends_with(&[0, 0]) => {
            let s = from_utf16le(b.get(..b.len() - 2)?);
            (!s.is_empty() && s.chars().all(|c| !c.is_control())).then_some(s)
        }
        _ => None,
    })
}

/// Writes a table of contents file.
pub fn write(toc: &NotebookToc, entry_ids: &[Guid], ancestor: Guid) -> Result<Vec<u8>> {
    Ok(notecraft_onestore::write(&toc_store(toc, entry_ids, ancestor))?)
}
