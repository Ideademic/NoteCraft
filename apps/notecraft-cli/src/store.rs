//! `dump --store`: the low-level MS-ONESTORE objects of a `.one` / `.onetoc2` (or of every file
//! in a `.onepkg`), as JSON.

use notecraft_doc::Guid;
use notecraft_one::ids::jcid;
use notecraft_onestore::{ExtendedGuid, PropertySet, PropertyValue, Store};
use serde_json::{Value, json};

/// Bytes shown of a byte property (the length is always given).
const SHOWN_BYTES: usize = 64;
/// Deepest nested property set dumped.
const MAX_DEPTH: usize = 32;

/// JCID names from [MS-ONE] (and the ink objects NoteCraft observed).
fn jcid_name(id: u32) -> Option<&'static str> {
    let names: &[(u32, &str)] = &[
        (jcid::TOC_CONTAINER, "jcidPersistablePropertyContainerForTOC"),
        (jcid::AUTHOR_CONTAINER, "jcidAuthorContainer"),
        (jcid::SECTION_NODE, "jcidSectionNode"),
        (jcid::PAGE_SERIES_NODE, "jcidPageSeriesNode"),
        (jcid::PAGE_NODE, "jcidPageNode"),
        (jcid::OUTLINE_NODE, "jcidOutlineNode"),
        (jcid::OUTLINE_ELEMENT_NODE, "jcidOutlineElementNode"),
        (jcid::RICH_TEXT_OE_NODE, "jcidRichTextOENode"),
        (jcid::IMAGE_NODE, "jcidImageNode"),
        (jcid::NUMBER_LIST_NODE, "jcidNumberListNode"),
        (jcid::OUTLINE_GROUP, "jcidOutlineGroup"),
        (jcid::TABLE_NODE, "jcidTableNode"),
        (jcid::TABLE_ROW_NODE, "jcidTableRowNode"),
        (jcid::TABLE_CELL_NODE, "jcidTableCellNode"),
        (jcid::TITLE_NODE, "jcidTitleNode"),
        (jcid::PAGE_METADATA, "jcidPageMetaData"),
        (jcid::SECTION_METADATA, "jcidSectionMetaData"),
        (jcid::EMBEDDED_FILE_NODE, "jcidEmbeddedFileNode"),
        (jcid::EMBEDDED_FILE_CONTAINER, "jcidEmbeddedFileContainer"),
        (jcid::PAGE_MANIFEST_NODE, "jcidPageManifestNode"),
        (jcid::PICTURE_CONTAINER, "jcidPictureContainer"),
        (jcid::NOTE_TAG_DEFINITION, "jcidNoteTagSharedDefinitionContainer"),
        (jcid::REVISION_METADATA, "jcidRevisionMetaData"),
        (jcid::PARAGRAPH_STYLE, "jcidParagraphStyleObject"),
        (jcid::INK_NODE, "ink node (observed)"),
        (jcid::INK_DATA, "ink data (observed)"),
        (jcid::INK_STROKE, "ink stroke (observed)"),
        (jcid::INK_STROKE_PROPERTIES, "ink stroke properties (observed)"),
    ];
    names.iter().find(|(j, _)| *j == id).map(|(_, n)| *n)
}

fn eguid(g: ExtendedGuid) -> String {
    format!("{}:{}", Guid(g.guid), g.n)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A wz string (UTF-16LE, maybe NUL-terminated) when the bytes look like readable text.
fn utf16_text(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|c| u16::from_le_bytes(*c)).collect();
    let units = units.strip_suffix(&[0]).unwrap_or(&units);
    let text = String::from_utf16(units).ok()?;
    let readable = !text.is_empty() && text.chars().all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t' || c == '\u{b}');
    readable.then_some(text)
}

fn value(v: &PropertyValue, depth: usize) -> Value {
    match v {
        PropertyValue::NoData => Value::Null,
        PropertyValue::Bool(b) => json!(b),
        PropertyValue::U8(n) => json!(n),
        PropertyValue::U16(n) => json!(n),
        PropertyValue::U32(n) => json!(n),
        PropertyValue::U64(n) => json!(n),
        PropertyValue::Bytes(b) => {
            let shown = b.get(..SHOWN_BYTES.min(b.len())).unwrap_or_default();
            let mut o = json!({"bytes": b.len(), "hex": hex(shown)});
            if let (Some(t), Some(m)) = (utf16_text(b), o.as_object_mut()) {
                m.insert("text".into(), json!(t));
            }
            o
        }
        PropertyValue::Ids(ids) => json!(ids.iter().map(|g| eguid(*g)).collect::<Vec<_>>()),
        PropertyValue::Sets(sets) => json!(sets.iter().map(|s| props(s, depth + 1)).collect::<Vec<_>>()),
    }
}

fn props(set: &PropertySet, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return json!("(nested too deeply)");
    }
    Value::Array(set.props.iter().map(|(id, v)| json!({"id": format!("0x{:08X}", id.0), "type": id.ty(), "value": value(v, depth)})).collect())
}

/// One revision store as JSON.
pub fn store_json(name: &str, store: &Store) -> Value {
    let spaces: Vec<Value> = store
        .spaces
        .iter()
        .map(|sp| {
            let objects: Vec<Value> = sp
                .objects
                .iter()
                .map(|o| {
                    let mut v = json!({"id": eguid(o.id), "jcid": format!("0x{:08X}", o.jcid), "props": props(&o.props, 0)});
                    if let Some(m) = v.as_object_mut() {
                        if let Some(n) = jcid_name(o.jcid) {
                            m.insert("name".into(), json!(n));
                        }
                        if let Some(d) = &o.file_data {
                            m.insert("fileData".into(), json!({"bytes": d.len(), "ext": o.file_ext}));
                        }
                    }
                    v
                })
                .collect();
            json!({
                "id": eguid(sp.id),
                "roots": sp.roots.iter().map(|(role, id)| json!({"role": role, "object": eguid(*id)})).collect::<Vec<_>>(),
                "objects": objects,
            })
        })
        .collect();
    json!({
        "file": name,
        "kind": format!("{:?}", store.kind),
        "fileId": store.file_id,
        "ancestor": store.ancestor,
        "rootSpace": eguid(store.root),
        "warnings": store.warnings,
        "spaces": spaces,
    })
}

/// `dump --store` of file bytes: one store for `.one`/`.onetoc2`, every store in a `.onepkg`.
pub fn dump_store(name: &str, bytes: &[u8]) -> Result<Value, String> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".onepkg") {
        let files = notecraft_onepkg::read_files(bytes).map_err(|e| format!("{name}: {e}"))?;
        let stores: Vec<Value> = files
            .iter()
            .map(|f| match notecraft_onestore::read(&f.bytes) {
                Ok(s) => store_json(&f.path, &s),
                Err(e) => json!({"file": f.path, "error": e.to_string()}),
            })
            .collect();
        return Ok(json!({"package": name, "files": stores}));
    }
    let store = notecraft_onestore::read(bytes).map_err(|e| format!("{name}: {e}"))?;
    Ok(store_json(name, &store))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_section_dumps() {
        let nb = notecraft_doc::Notebook::new_default();
        let bytes = notecraft_one::write_section(nb.first_section().unwrap()).unwrap();
        let v = dump_store("Quick Notes.one", &bytes).unwrap();
        assert_eq!(v["kind"], "Section");
        let text = v.to_string();
        assert!(text.contains("jcidPageNode"), "{}", &text[..text.len().min(400)]);
        assert!(dump_store("x.one", b"junk").is_err());
    }

    #[test]
    fn utf16() {
        assert_eq!(utf16_text(&[b'h', 0, b'i', 0, 0, 0]).as_deref(), Some("hi"));
        assert_eq!(utf16_text(&[1, 0]), None);
        assert_eq!(utf16_text(&[1]), None);
    }
}
