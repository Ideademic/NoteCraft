//! Content NoteCraft keeps without editing it ([`notecraft_doc::ElementKind::Opaque`]): the
//! objects reachable from an element's root object, captured with their properties so they can
//! be written back (with fresh object ids) in a later save.
//!
//! The encoding is NoteCraft's own: `NCOP`, version 1, the object count, then per object its
//! JCID, its file data (if any) and its properties, with object references replaced by indexes
//! into the captured list. Content that refers to other object spaces or contexts isn't
//! captured (it couldn't be re-linked).

use std::collections::HashMap;
use std::sync::Arc;

use notecraft_onestore::bytes::{Reader, WriteLe, from_utf16le, utf16le};
use notecraft_onestore::propset::ty;
use notecraft_onestore::{ExtendedGuid, ObjectSpace, PropertyId, PropertySet, PropertyValue};

use crate::build::SpaceBuilder;

const MAGIC: &[u8; 4] = b"NCOP";
/// Most objects one opaque element keeps.
const MAX_OBJECTS: usize = 10_000;
/// Largest encoding kept (bytes).
const MAX_BYTES: usize = 64 << 20;
/// Deepest property-set nesting encoded.
const MAX_DEPTH: usize = 16;

/// Captures the objects reachable from `root`. `None` when they can't be kept faithfully.
pub fn capture(space: &ObjectSpace, index: &HashMap<ExtendedGuid, usize>, root: ExtendedGuid) -> Option<Vec<u8>> {
    let mut order = vec![root];
    let mut local: HashMap<ExtendedGuid, u32> = HashMap::from([(root, 0)]);
    let mut i = 0;
    while let Some(&id) = order.get(i) {
        let obj = space.objects.get(*index.get(&id)?)?;
        if has_foreign_refs(&obj.props, 0) {
            return None;
        }
        for r in obj.props.object_refs() {
            if !r.is_nil() && !local.contains_key(&r) {
                if order.len() >= MAX_OBJECTS {
                    return None;
                }
                local.insert(r, order.len() as u32);
                order.push(r);
            }
        }
        i += 1;
    }
    let mut out = Vec::new();
    out.put(MAGIC);
    out.put_u8(1);
    out.put_u32(order.len() as u32);
    for id in &order {
        let obj = space.objects.get(*index.get(id)?)?;
        out.put_u32(obj.jcid);
        match &obj.file_data {
            Some(d) => {
                out.put_u8(1);
                let ext = utf16le(obj.file_ext.as_deref().unwrap_or_default());
                out.put_u32(ext.len() as u32);
                out.put(&ext);
                out.put_u32(u32::try_from(d.len()).ok()?);
                out.put(d);
            }
            None => out.put_u8(0),
        }
        encode_set(&mut out, &obj.props, &local, 0)?;
        if out.len() > MAX_BYTES {
            return None;
        }
    }
    Some(out)
}

fn has_foreign_refs(set: &PropertySet, depth: usize) -> bool {
    depth > MAX_DEPTH
        || set.props.iter().any(|(id, v)| match v {
            PropertyValue::Ids(ids) => !ids.is_empty() && !matches!(id.ty(), ty::OBJECT_ID | ty::ARRAY_OF_OBJECT_IDS),
            PropertyValue::Sets(s) => s.iter().any(|s| has_foreign_refs(s, depth + 1)),
            _ => false,
        })
}

fn encode_set(out: &mut Vec<u8>, set: &PropertySet, local: &HashMap<ExtendedGuid, u32>, depth: usize) -> Option<()> {
    if depth > MAX_DEPTH {
        return None;
    }
    out.put_u16(u16::try_from(set.props.len()).ok()?);
    for (id, v) in &set.props {
        out.put_u32(id.0);
        match v {
            PropertyValue::NoData => out.put_u8(0),
            PropertyValue::Bool(b) => {
                out.put_u8(1);
                out.put_u8(u8::from(*b));
            }
            PropertyValue::U8(x) => {
                out.put_u8(2);
                out.put_u8(*x);
            }
            PropertyValue::U16(x) => {
                out.put_u8(3);
                out.put_u16(*x);
            }
            PropertyValue::U32(x) => {
                out.put_u8(4);
                out.put_u32(*x);
            }
            PropertyValue::U64(x) => {
                out.put_u8(5);
                out.put_u64(*x);
            }
            PropertyValue::Bytes(b) => {
                out.put_u8(6);
                out.put_u32(u32::try_from(b.len()).ok()?);
                out.put(b);
            }
            PropertyValue::Ids(ids) => {
                out.put_u8(7);
                out.put_u32(ids.len() as u32);
                for g in ids {
                    // u32::MAX: a nil reference.
                    out.put_u32(if g.is_nil() { u32::MAX } else { *local.get(g)? });
                }
            }
            PropertyValue::Sets(sets) => {
                out.put_u8(8);
                out.put_u32(sets.len() as u32);
                for s in sets {
                    encode_set(out, s, local, depth + 1)?;
                }
            }
        }
    }
    Some(())
}

/// Writes captured objects into `b` with fresh ids; returns the root's new id.
pub fn emit(b: &mut SpaceBuilder, data: &[u8]) -> Option<ExtendedGuid> {
    let mut r = Reader::new(data);
    if r.bytes(4).ok()? != MAGIC || r.u8().ok()? != 1 {
        return None;
    }
    let n = r.u32().ok()? as usize;
    if n == 0 || n > MAX_OBJECTS {
        return None;
    }
    let ids: Vec<ExtendedGuid> = (0..n).map(|_| b.new_id()).collect();
    let mut objects = Vec::with_capacity(n);
    for id in &ids {
        let jcid = r.u32().ok()?;
        let file = match r.u8().ok()? {
            0 => None,
            _ => {
                let elen = r.u32().ok()? as usize;
                let ext = from_utf16le(r.bytes(elen).ok()?);
                let dlen = r.u32().ok()? as usize;
                Some((Arc::new(r.bytes(dlen).ok()?.to_vec()), ext))
            }
        };
        let props = decode_set(&mut r, &ids, 0)?;
        let mut obj = notecraft_onestore::Object::new(*id, jcid, props);
        if let Some((d, ext)) = file {
            obj.file_data = Some(d);
            obj.file_ext = Some(ext).filter(|e| !e.is_empty());
        }
        objects.push(obj);
    }
    b.objects.extend(objects);
    ids.first().copied()
}

fn decode_set(r: &mut Reader<'_>, ids: &[ExtendedGuid], depth: usize) -> Option<PropertySet> {
    if depth > MAX_DEPTH {
        return None;
    }
    let count = usize::from(r.u16().ok()?);
    let mut set = PropertySet::default();
    for _ in 0..count {
        let id = PropertyId::new(r.u32().ok()?);
        let v = match r.u8().ok()? {
            0 => PropertyValue::NoData,
            1 => PropertyValue::Bool(r.u8().ok()? != 0),
            2 => PropertyValue::U8(r.u8().ok()?),
            3 => PropertyValue::U16(r.u16().ok()?),
            4 => PropertyValue::U32(r.u32().ok()?),
            5 => PropertyValue::U64(r.u64().ok()?),
            6 => {
                let n = r.u32().ok()? as usize;
                PropertyValue::Bytes(r.bytes(n).ok()?.to_vec())
            }
            7 => {
                let n = r.u32().ok()? as usize;
                if n.saturating_mul(4) > r.remaining() {
                    return None;
                }
                let refs =
                    (0..n).map(|_| r.u32().ok().map(|i| ids.get(i as usize).copied().unwrap_or(ExtendedGuid::NIL))).collect::<Option<Vec<_>>>()?;
                PropertyValue::Ids(refs)
            }
            8 => {
                let n = r.u32().ok()? as usize;
                if n.saturating_mul(2) > r.remaining() {
                    return None;
                }
                PropertyValue::Sets((0..n).map(|_| decode_set(r, ids, depth + 1)).collect::<Option<Vec<_>>>()?)
            }
            _ => return None,
        };
        set.props.push((id, v));
    }
    Some(set)
}

/// What kept content is, as far as NoteCraft can tell, for the placeholder the canvas draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpaqueKind {
    Table,
    AudioRecording,
    VideoRecording,
    /// Something else with a file in it.
    File,
    /// Anything else.
    Other,
}

/// Kept content's kind, and where it sits and how big it is when its root object says (points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpaqueInfo {
    pub kind: OpaqueKind,
    /// Offset from the page's top-left corner.
    pub pos: Option<(f32, f32)>,
    pub size: Option<(f32, f32)>,
}

const AUDIO: &[&str] = &["wav", "wma", "mp3", "m4a", "aac", "ogg", "oga", "opus", "amr", "flac"];
const VIDEO: &[&str] = &["mp4", "m4v", "wmv", "mov", "avi", "mkv", "webm", "3gp"];

/// Reads what [`capture`] kept of an element (never fails: junk is [`OpaqueKind::Other`] with no
/// geometry).
pub fn describe(jcid: u32, data: &[u8]) -> OpaqueInfo {
    let none = OpaqueInfo { kind: if jcid == crate::ids::jcid::TABLE_NODE { OpaqueKind::Table } else { OpaqueKind::Other }, pos: None, size: None };
    let mut r = Reader::new(data);
    if r.bytes(4).ok() != Some(&MAGIC[..]) || r.u8().ok() != Some(1) {
        return none;
    }
    let Some(n) = r.u32().ok().map(|n| n as usize).filter(|n| (1..=MAX_OBJECTS).contains(n)) else { return none };
    let ids = vec![ExtendedGuid::NIL; n];
    let mut info = none;
    let mut exts: Vec<String> = Vec::new();
    for i in 0..n {
        let Ok(_jcid) = r.u32() else { break };
        let has_file = r.u8().ok();
        if has_file.is_some_and(|f| f != 0) {
            let Some(elen) = r.u32().ok().map(|x| x as usize) else { break };
            let Some(ext) = r.bytes(elen).ok().map(from_utf16le) else { break };
            let Some(dlen) = r.u32().ok().map(|x| x as usize) else { break };
            if r.bytes(dlen).is_err() {
                break;
            }
            exts.push(ext.trim_start_matches('.').to_ascii_lowercase());
        } else if has_file.is_none() {
            break;
        }
        let Some(props) = decode_set(&mut r, &ids, 0) else { break };
        if i == 0 {
            let f = |id: u32| props.f32(id).filter(|v| v.is_finite()).map(crate::util::hi_to_pt);
            use crate::ids::prop;
            info.pos = f(prop::OFFSET_FROM_PARENT_HORIZ)
                .zip(f(prop::OFFSET_FROM_PARENT_VERT))
                .filter(|(x, y)| (0.0..1e6).contains(x) && (0.0..1e6).contains(y));
            info.size = f(prop::LAYOUT_MAX_WIDTH).zip(f(prop::LAYOUT_MAX_HEIGHT)).filter(|(w, h)| (1.0..1e5).contains(w) && (1.0..1e5).contains(h));
        }
    }
    if info.kind == OpaqueKind::Other {
        info.kind = if exts.iter().any(|e| AUDIO.contains(&e.as_str())) {
            OpaqueKind::AudioRecording
        } else if exts.iter().any(|e| VIDEO.contains(&e.as_str())) {
            OpaqueKind::VideoRecording
        } else if !exts.is_empty() {
            OpaqueKind::File
        } else {
            OpaqueKind::Other
        };
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_doc::Guid;

    #[test]
    fn describes_kept_content() {
        let g = |n| ExtendedGuid::new([4; 16], n);
        let mut root = PropertySet::default();
        root.push(crate::ids::prop::OFFSET_FROM_PARENT_HORIZ, PropertyValue::U32(2.0f32.to_bits()))
            .push(crate::ids::prop::OFFSET_FROM_PARENT_VERT, PropertyValue::U32(4.0f32.to_bits()))
            .push(crate::ids::prop::LAYOUT_MAX_WIDTH, PropertyValue::U32(3.0f32.to_bits()))
            .push(crate::ids::prop::LAYOUT_MAX_HEIGHT, PropertyValue::U32(1.0f32.to_bits()))
            .push(0x2000_1D9B, PropertyValue::Ids(vec![g(2)]));
        let mut file = Object::new(g(2), 0x0008_0036, PropertySet::default());
        file.file_data = Some(Arc::new(vec![1, 2, 3]));
        file.file_ext = Some(".wma".into());
        let space = ObjectSpace { id: g(100), roots: vec![], objects: vec![Object::new(g(1), 0x0006_0099, root), file], ..ObjectSpace::default() };
        let data = capture(&space, &space.index(), g(1)).unwrap();
        let info = describe(0x0006_0099, &data);
        assert_eq!(info.kind, OpaqueKind::AudioRecording);
        assert_eq!(info.pos, Some((72.0, 144.0)));
        assert_eq!(info.size, Some((108.0, 36.0)));
        assert_eq!(describe(crate::ids::jcid::TABLE_NODE, b"junk").kind, OpaqueKind::Table);
        for cut in 0..data.len() {
            let _ = describe(1, &data[..cut]);
        }
    }
    use notecraft_onestore::Object;

    #[test]
    fn capture_and_emit_round_trip() {
        let g = |n| ExtendedGuid::new([3; 16], n);
        let mut table = PropertySet::default();
        table.push(0x24001C20, PropertyValue::Ids(vec![g(2), g(2)])).push(0x14001D57, PropertyValue::U32(1));
        let mut row = PropertySet::default();
        row.push(0x1C001C22, PropertyValue::Bytes(vec![1, 2]));
        let space = ObjectSpace {
            id: g(100),
            roots: vec![],
            objects: vec![Object::new(g(1), 0x0006_0022, table), Object::new(g(2), 0x0006_0023, row), Object::new(g(9), 1, PropertySet::default())],
            ..ObjectSpace::default()
        };
        let data = capture(&space, &space.index(), g(1)).unwrap();
        let mut b = SpaceBuilder::new(ExtendedGuid::from_guid(Guid::new(), 1));
        let root = emit(&mut b, &data).unwrap();
        let out = b.finish();
        assert_eq!(out.objects.len(), 2, "only reachable objects are kept");
        assert_eq!(out.objects[0].id, root);
        assert_eq!(out.objects[0].jcid, 0x0006_0022);
        let child = out.objects[1].id;
        assert_eq!(out.objects[0].props.ids(0x24001C20), &[child, child]);
        assert_eq!(out.objects[1].props.bytes(0x1C001C22), Some(&[1u8, 2][..]));
        // Junk never panics.
        for cut in 0..data.len() {
            let _ = emit(&mut SpaceBuilder::new(ExtendedGuid::NIL), &data[..cut]);
        }
    }
}
