//! The notebook recycle bin on disk: the `OneNote_RecycleBin` folder inside a notebook folder.
//!
//! Laid out as OneNote's local notebooks have it (observed): a deleted section keeps being a
//! `.one` file, moved into the folder; deleted pages go to a section of their own,
//! `OneNote_DeletedPages.one`. Neither MS-ONE nor MS-ONESTORE documents where an item came from
//! or when it was deleted, so NoteCraft records that in private properties OneNote ignores and
//! keeps (MS-ONE §2.1.12): on the section's metadata (`jcidSectionMetaData`) for a deleted
//! section, on each page's metadata (`jcidPageMetaData`, in the page and in its page series) for
//! a deleted page. Files without them (OneNote's) are read as deleted at their last change, from
//! the notebook's top level.

use std::collections::HashSet;

use notecraft_doc::history::MAX_RECYCLE_BIN_ENTRIES;
use notecraft_doc::{Color, DeletedPage, DeletedSection, Guid, RecycleBin, Section};
use notecraft_onestore::{ExtendedGuid, PropertySet, PropertyValue, Store};

use crate::ids::{jcid, prop, role};
use crate::read::View;
use crate::util::{from_filetime, guid, string, to_filetime, wz_bytes};
use crate::{NotebookFile, Result, SECTION_EXT, read, section_file_name, write};

/// The recycle bin folder inside a notebook folder (OneNote's name).
pub const RECYCLE_BIN_FOLDER: &str = "OneNote_RecycleBin";
/// The section holding deleted pages, inside the recycle bin folder (OneNote's name).
pub const DELETED_PAGES_FILE: &str = "OneNote_DeletedPages.one";
/// The display name written into the deleted pages section.
const DELETED_PAGES_NAME: &str = "Deleted Pages";

/// The file name of each deleted section in the recycle bin folder, in bin order: its own file
/// name made unique there (never [`DELETED_PAGES_FILE`]).
pub fn recycle_bin_names(bin: &RecycleBin) -> Vec<(Guid, String)> {
    let mut used: HashSet<String> = HashSet::new();
    used.insert(DELETED_PAGES_FILE.to_lowercase());
    bin.sections
        .iter()
        .map(|d| {
            let base = section_file_name(&d.section);
            let (stem, ext) = base.rsplit_once('.').unwrap_or((base.as_str(), SECTION_EXT));
            let mut name = base.clone();
            let mut n = 2;
            while used.contains(&name.to_lowercase()) {
                name = format!("{stem} ({n}).{ext}");
                n += 1;
            }
            used.insert(name.to_lowercase());
            (d.section.id, name)
        })
        .collect()
}

/// A deleted section's `.one` file: the section, plus when it was deleted and where it was.
pub fn write_deleted_section(d: &DeletedSection, file_name: &str) -> Result<Vec<u8>> {
    let mut store = write::section_store(&d.section, file_name, Guid::NIL)?;
    let mut extra = PropertySet::default();
    extra
        .push(prop::NC_DELETED_TIME, PropertyValue::U64(to_filetime(d.deleted)))
        .push(prop::NC_DELETED_GROUP_PATH, PropertyValue::Bytes(d.group_path.iter().flat_map(|g| g.0).collect()))
        .push(prop::NC_DELETED_INDEX, PropertyValue::U32(u32::try_from(d.index).unwrap_or(u32::MAX)));
    if let Some(meta) = root_metadata_mut(&mut store) {
        meta.props.props.extend(extra.props);
    }
    Ok(notecraft_onestore::write(&store)?)
}

/// `OneNote_DeletedPages.one`: the deleted pages as one section, each page's metadata noting
/// when it was deleted and where it was.
pub fn write_deleted_pages(pages: &[DeletedPage]) -> Result<Vec<u8>> {
    let mut section = Section::new(DELETED_PAGES_NAME, Color::section_color(0));
    section.pages = pages.iter().map(|d| d.page.clone()).collect();
    let mut store = write::section_store(&section, DELETED_PAGES_FILE, Guid::NIL)?;
    for d in pages {
        let mut extra = PropertySet::default();
        extra
            .push(prop::NC_DELETED_TIME, PropertyValue::U64(to_filetime(d.deleted)))
            .push(prop::NC_DELETED_SECTION, PropertyValue::Bytes(d.section.0.to_vec()))
            .push(prop::NC_DELETED_SECTION_NAME, wz_bytes(&d.section_name))
            .push(prop::NC_DELETED_INDEX, PropertyValue::U32(u32::try_from(d.index).unwrap_or(u32::MAX)));
        for meta in page_metadata_mut(&mut store, d.page.id) {
            meta.props.props.extend(extra.props.iter().cloned());
        }
    }
    Ok(notecraft_onestore::write(&store)?)
}

/// Every file of the recycle bin, paths relative to the notebook folder
/// (`OneNote_RecycleBin/…`). An empty bin has no files.
pub fn recycle_bin_files(bin: &RecycleBin) -> Result<Vec<NotebookFile>> {
    let mut out = Vec::new();
    for ((_, name), d) in recycle_bin_names(bin).into_iter().zip(&bin.sections) {
        out.push(NotebookFile { path: format!("{RECYCLE_BIN_FOLDER}/{name}"), bytes: write_deleted_section(d, &name)? });
    }
    if !bin.pages.is_empty() {
        out.push(NotebookFile { path: format!("{RECYCLE_BIN_FOLDER}/{DELETED_PAGES_FILE}"), bytes: write_deleted_pages(&bin.pages)? });
    }
    Ok(out)
}

/// Reads the recycle bin from its files: `(file name, bytes)` of the `.one` files in the
/// recycle bin folder. Unreadable files are skipped (logged).
pub fn read_recycle_bin(files: &[(&str, &[u8])]) -> RecycleBin {
    let mut bin = RecycleBin::default();
    for (name, bytes) in files {
        if bin.len() >= MAX_RECYCLE_BIN_ENTRIES {
            break;
        }
        let lower = name.to_ascii_lowercase();
        if !lower.ends_with(&format!(".{SECTION_EXT}")) || lower.starts_with('.') {
            continue;
        }
        let store = match notecraft_onestore::read(bytes) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("recycle bin: {name} skipped: {e}");
                continue;
            }
        };
        let stem = name.get(..name.len().saturating_sub(SECTION_EXT.len() + 1)).unwrap_or(name);
        let section = match read::section(&store, stem) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("recycle bin: {name} skipped: {e}");
                continue;
            }
        };
        if lower == DELETED_PAGES_FILE.to_lowercase() {
            for page in section.pages {
                let meta = page_metadata(&store, page.id);
                let time = meta.and_then(|m| m.uint(prop::NC_DELETED_TIME)).and_then(from_filetime).unwrap_or(page.modified);
                let from = meta.and_then(|m| m.bytes(prop::NC_DELETED_SECTION)).and_then(|b| <[u8; 16]>::try_from(b).ok()).map_or(Guid::NIL, Guid);
                let section_name = meta.and_then(|m| string(m, prop::NC_DELETED_SECTION_NAME)).unwrap_or_default();
                let index = meta.and_then(|m| m.u32(prop::NC_DELETED_INDEX)).unwrap_or(0) as usize;
                bin.pages.push(DeletedPage { page, deleted: time, section: from, section_name, index });
            }
        } else {
            let root = store.root_space().map(View::new);
            let meta = root.as_ref().and_then(|v| v.root(role::METADATA)).map(|o| &o.props);
            let latest = section.pages.iter().map(|p| p.modified).max().unwrap_or_else(notecraft_doc::now);
            let deleted = meta.and_then(|m| m.uint(prop::NC_DELETED_TIME)).and_then(from_filetime).unwrap_or(latest);
            let group_path = meta
                .and_then(|m| m.bytes(prop::NC_DELETED_GROUP_PATH))
                .map(|b| b.as_chunks::<16>().0.iter().map(|c| Guid(*c)).take(32).collect())
                .unwrap_or_default();
            let index = meta.and_then(|m| m.u32(prop::NC_DELETED_INDEX)).unwrap_or(u32::MAX) as usize;
            let mut section = section;
            section.source.file_name = None;
            bin.sections.push(DeletedSection { section, deleted, group_path, index });
        }
    }
    bin.sections.sort_by_key(|s| s.deleted);
    bin.pages.sort_by_key(|p| p.deleted);
    bin
}

/// The root object space's metadata root (jcidSectionMetaData).
fn root_metadata_mut(store: &mut Store) -> Option<&mut notecraft_onestore::Object> {
    let root = store.root;
    let space = store.spaces.iter_mut().find(|s| s.id == root)?;
    let id = space.root(role::METADATA)?;
    space.objects.iter_mut().find(|o| o.id == id)
}

/// Page `id`'s metadata objects: in its object space and the copy in its page series.
fn page_metadata_mut(store: &mut Store, id: Guid) -> Vec<&mut notecraft_onestore::Object> {
    let mut out = Vec::new();
    let space_id = ExtendedGuid::from_guid(id, 1);
    for space in &mut store.spaces {
        let meta_root = space.root(role::METADATA);
        let is_page = space.id == space_id;
        for o in &mut space.objects {
            let ours = o.jcid == jcid::PAGE_METADATA && guid(&o.props, prop::NOTEBOOK_MANAGEMENT_ENTITY_GUID) == Some(id);
            if ours || (is_page && Some(o.id) == meta_root) {
                out.push(o);
            }
        }
    }
    out
}

/// Page `id`'s metadata, read from its own object space.
fn page_metadata(store: &Store, id: Guid) -> Option<&PropertySet> {
    store.spaces.iter().find_map(|s| {
        let meta_id = s.root(role::METADATA)?;
        let o = s.objects.iter().find(|o| o.id == meta_id && o.jcid == jcid::PAGE_METADATA)?;
        (guid(&o.props, prop::NOTEBOOK_MANAGEMENT_ENTITY_GUID) == Some(id)).then_some(&o.props)
    })
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use notecraft_doc::{Page, SectionGroup};

    use super::*;

    fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    #[test]
    fn bin_round_trips_through_files() {
        let mut s = Section::new("Plans", Color::section_color(2));
        s.pages.push(Page::new_at("Roadmap", at(1_700_000_000)));
        let g = SectionGroup::new("Work");
        let mut bin = RecycleBin::default();
        bin.sections.push(DeletedSection { section: s.clone(), deleted: at(1_750_000_000), group_path: vec![g.id], index: 3 });
        let mut same_name = Section::new("Plans", Color::WHITE);
        same_name.pages.push(Page::new_at("Other", at(1_700_000_100)));
        bin.sections.push(DeletedSection { section: same_name, deleted: at(1_750_000_100), group_path: vec![], index: 0 });
        let mut p = Page::new_at("Gone", at(1_700_000_000));
        p.level = 1;
        let sid = Guid::new();
        bin.pages.push(DeletedPage { page: p.clone(), deleted: at(1_760_000_000), section: sid, section_name: "Quick Notes".into(), index: 2 });
        let files = recycle_bin_files(&bin).unwrap();
        let names: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(names, vec!["OneNote_RecycleBin/Plans.one", "OneNote_RecycleBin/Plans (2).one", "OneNote_RecycleBin/OneNote_DeletedPages.one"]);
        let pairs: Vec<(&str, &[u8])> = files.iter().map(|f| (f.path.rsplit('/').next().unwrap(), f.bytes.as_slice())).collect();
        let back = read_recycle_bin(&pairs);
        assert_eq!(back.sections.len(), 2);
        let d = &back.sections[0];
        assert_eq!((d.section.id, d.section.name.as_str(), d.deleted, d.index), (s.id, "Plans", at(1_750_000_000), 3));
        assert_eq!(d.group_path, vec![g.id]);
        assert_eq!(d.section.pages[0].title, "Roadmap");
        let dp = &back.pages[0];
        assert_eq!((dp.page.id, dp.page.title.as_str(), dp.page.level), (p.id, "Gone", 1));
        assert_eq!((dp.deleted, dp.section, dp.section_name.as_str(), dp.index), (at(1_760_000_000), sid, "Quick Notes", 2));
        assert!(recycle_bin_files(&RecycleBin::default()).unwrap().is_empty());
    }

    #[test]
    fn foreign_bin_files_get_defaults_and_junk_is_skipped() {
        let mut s = Section::new("Old", Color::WHITE);
        s.pages.push(Page::new_at("p", at(1_700_000_000)));
        let plain = crate::write_section(&s).unwrap();
        let bin = read_recycle_bin(&[("Old.one", &plain), ("junk.one", b"nope"), ("notes.txt", b"x")]);
        assert_eq!(bin.sections.len(), 1);
        assert_eq!(bin.sections[0].deleted, at(1_700_000_000));
        assert!(bin.sections[0].group_path.is_empty());
        let pages = crate::write_section(&s).unwrap();
        let bin = read_recycle_bin(&[(DELETED_PAGES_FILE, &pages)]);
        assert_eq!(bin.pages.len(), 1);
        assert!(bin.pages[0].section.is_nil());
    }
}
