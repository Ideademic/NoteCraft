//! Property sets ([MS-ONESTORE] §2.6): the typed values objects carry; ids from [MS-ONE] §2.1.
//!
//! An object's data is an ObjectSpaceObjectPropSet (§2.6.1): three streams of CompactIDs (object,
//! object space and context references) followed by a PropertySet whose `rgData` holds every
//! other value. References are resolved to [`ExtendedGuid`]s when read and compacted again when
//! written.

use crate::bytes::{Reader, WriteLe};
use crate::{CompactId, Error, ExtendedGuid, Result};

/// Deepest nesting of property sets inside property sets (hostile files).
pub const MAX_DEPTH: usize = 32;

/// Property types (§2.6.6).
pub mod ty {
    pub const NO_DATA: u8 = 0x1;
    pub const BOOL: u8 = 0x2;
    pub const ONE_BYTE: u8 = 0x3;
    pub const TWO_BYTES: u8 = 0x4;
    pub const FOUR_BYTES: u8 = 0x5;
    pub const EIGHT_BYTES: u8 = 0x6;
    pub const FOUR_BYTES_OF_LENGTH_FOLLOWED_BY_DATA: u8 = 0x7;
    pub const OBJECT_ID: u8 = 0x8;
    pub const ARRAY_OF_OBJECT_IDS: u8 = 0x9;
    pub const OBJECT_SPACE_ID: u8 = 0xA;
    pub const ARRAY_OF_OBJECT_SPACE_IDS: u8 = 0xB;
    pub const CONTEXT_ID: u8 = 0xC;
    pub const ARRAY_OF_CONTEXT_IDS: u8 = 0xD;
    pub const ARRAY_OF_PROPERTY_VALUES: u8 = 0x10;
    pub const PROPERTY_SET: u8 = 0x11;
}

/// A property id: the 26-bit id plus the 5 type bits (§2.6.6), as MS-ONE lists them
/// (`0x14001C01` is PageWidth, four bytes of data). The bool value bit is not part of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PropertyId(pub u32);

impl PropertyId {
    pub const fn new(raw: u32) -> PropertyId {
        PropertyId(raw & 0x7FFF_FFFF)
    }

    /// The 26-bit id.
    pub const fn id(self) -> u32 {
        self.0 & 0x03FF_FFFF
    }

    /// The type bits.
    pub const fn ty(self) -> u8 {
        ((self.0 >> 26) & 0x1F) as u8
    }

    /// The same id with another type.
    pub const fn with_type(self, ty: u8) -> PropertyId {
        PropertyId(self.id() | ((ty as u32 & 0x1F) << 26))
    }
}

/// A property value.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    /// No data (`0x1`).
    NoData,
    /// A bool (`0x2`).
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    /// Length-prefixed bytes (`0x7`).
    Bytes(Vec<u8>),
    /// Object / object space / context id references (`0x8`–`0xD`), resolved. Single-reference
    /// types hold one element.
    Ids(Vec<ExtendedGuid>),
    /// A nested property set (`0x11`, one element) or an array of them (`0x10`).
    Sets(Vec<PropertySet>),
}

/// An ordered list of properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertySet {
    pub props: Vec<(PropertyId, PropertyValue)>,
}

impl PropertySet {
    /// The first value with this id (type bits ignored).
    pub fn get(&self, id: u32) -> Option<&PropertyValue> {
        self.props.iter().find(|(p, _)| p.0 & 0x03FF_FFFF == id & 0x03FF_FFFF).map(|(_, v)| v)
    }

    pub fn has(&self, id: u32) -> bool {
        self.get(id).is_some()
    }

    pub fn bool(&self, id: u32) -> Option<bool> {
        match self.get(id)? {
            PropertyValue::Bool(b) => Some(*b),
            PropertyValue::NoData => Some(true),
            PropertyValue::U8(v) => Some(*v != 0),
            _ => None,
        }
    }

    /// An integer value of any width.
    pub fn uint(&self, id: u32) -> Option<u64> {
        match self.get(id)? {
            PropertyValue::U8(v) => Some(u64::from(*v)),
            PropertyValue::U16(v) => Some(u64::from(*v)),
            PropertyValue::U32(v) => Some(u64::from(*v)),
            PropertyValue::U64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn u32(&self, id: u32) -> Option<u32> {
        self.uint(id).and_then(|v| u32::try_from(v).ok())
    }

    /// A four-byte float (MS-ONE's half-inch measures). Non-finite values read as `None`.
    pub fn f32(&self, id: u32) -> Option<f32> {
        match self.get(id)? {
            PropertyValue::U32(v) => Some(f32::from_bits(*v)).filter(|f| f.is_finite()),
            _ => None,
        }
    }

    pub fn bytes(&self, id: u32) -> Option<&[u8]> {
        match self.get(id)? {
            PropertyValue::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn ids(&self, id: u32) -> &[ExtendedGuid] {
        match self.get(id) {
            Some(PropertyValue::Ids(v)) => v,
            _ => &[],
        }
    }

    pub fn sets(&self, id: u32) -> &[PropertySet] {
        match self.get(id) {
            Some(PropertyValue::Sets(v)) => v,
            _ => &[],
        }
    }

    /// Appends a property (builder style).
    pub fn push(&mut self, id: u32, value: PropertyValue) -> &mut PropertySet {
        self.props.push((PropertyId::new(id), value));
        self
    }

    /// Every object reference (type `0x8`, `0x9`) in this set and nested sets.
    pub fn object_refs(&self) -> Vec<ExtendedGuid> {
        let mut out = Vec::new();
        collect_refs(self, &mut out, 0);
        out
    }
}

fn collect_refs(set: &PropertySet, out: &mut Vec<ExtendedGuid>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for (id, v) in &set.props {
        match v {
            PropertyValue::Ids(ids) if matches!(id.ty(), ty::OBJECT_ID | ty::ARRAY_OF_OBJECT_IDS) => out.extend(ids.iter().copied()),
            PropertyValue::Sets(sets) => sets.iter().for_each(|s| collect_refs(s, out, depth + 1)),
            _ => {}
        }
    }
}

/// Which reference stream a type reads from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stream {
    Oid,
    Osid,
    Ctx,
}

fn stream_of(t: u8) -> Option<(Stream, bool)> {
    match t {
        ty::OBJECT_ID => Some((Stream::Oid, false)),
        ty::ARRAY_OF_OBJECT_IDS => Some((Stream::Oid, true)),
        ty::OBJECT_SPACE_ID => Some((Stream::Osid, false)),
        ty::ARRAY_OF_OBJECT_SPACE_IDS => Some((Stream::Osid, true)),
        ty::CONTEXT_ID => Some((Stream::Ctx, false)),
        ty::ARRAY_OF_CONTEXT_IDS => Some((Stream::Ctx, true)),
        _ => None,
    }
}

/// The three CompactID streams being consumed while the property set is read.
struct Streams<'a> {
    oids: Reader<'a>,
    osids: Reader<'a>,
    ctxs: Reader<'a>,
    resolve: &'a dyn Fn(CompactId) -> ExtendedGuid,
}

impl Streams<'_> {
    fn take(&mut self, s: Stream, n: usize) -> Result<Vec<ExtendedGuid>> {
        let r = match s {
            Stream::Oid => &mut self.oids,
            Stream::Osid => &mut self.osids,
            Stream::Ctx => &mut self.ctxs,
        };
        if n.saturating_mul(4) > r.remaining() {
            return Err(Error::Corrupt("property set references more ids than its stream holds".into()));
        }
        (0..n).map(|_| Ok((self.resolve)(CompactId::from_u32(r.u32()?)))).collect()
    }
}

/// The stream header (§2.6.5): count, ExtendedStreamsPresent, OsidStreamNotPresent.
fn stream_header(r: &mut Reader<'_>) -> Result<(usize, bool, bool)> {
    let v = r.u32()?;
    Ok(((v & 0x00FF_FFFF) as usize, v & (1 << 30) != 0, v & (1 << 31) != 0))
}

fn stream<'a>(r: &mut Reader<'a>) -> Result<(Reader<'a>, bool, bool)> {
    let (count, extended, osid_absent) = stream_header(r)?;
    let len = count.checked_mul(4).ok_or_else(|| Error::Corrupt("id stream too long".into()))?;
    Ok((Reader::new(r.bytes(len)?), extended, osid_absent))
}

/// Decodes an ObjectSpaceObjectPropSet, resolving CompactIDs with `resolve`.
pub fn decode(data: &[u8], resolve: &dyn Fn(CompactId) -> ExtendedGuid) -> Result<PropertySet> {
    let mut r = Reader::new(data);
    let (oids, _, osid_absent) = stream(&mut r)?;
    let empty = Reader::new(&[]);
    let (osids, ctxs) = if osid_absent {
        (empty, empty)
    } else {
        let (osids, extended, _) = stream(&mut r)?;
        let ctxs = if extended { stream(&mut r)?.0 } else { empty };
        (osids, ctxs)
    };
    let mut streams = Streams { oids, osids, ctxs, resolve };
    read_set(&mut r, &mut streams, 0)
}

fn read_set(r: &mut Reader<'_>, s: &mut Streams<'_>, depth: usize) -> Result<PropertySet> {
    if depth > MAX_DEPTH {
        return Err(Error::Limit("property sets nested too deeply".into()));
    }
    let count = usize::from(r.u16()?);
    if count.saturating_mul(4) > r.remaining() {
        return Err(Error::Corrupt("property set: more properties than bytes".into()));
    }
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        ids.push(r.u32()?);
    }
    let mut set = PropertySet { props: Vec::with_capacity(count) };
    for raw in ids {
        let id = PropertyId::new(raw);
        let value = read_value(r, s, id.ty(), raw & (1 << 31) != 0, depth)?;
        set.props.push((id, value));
    }
    Ok(set)
}

fn read_value(r: &mut Reader<'_>, s: &mut Streams<'_>, t: u8, bool_bit: bool, depth: usize) -> Result<PropertyValue> {
    if let Some((stream, array)) = stream_of(t) {
        let n = if array { r.u32()? as usize } else { 1 };
        return Ok(PropertyValue::Ids(s.take(stream, n)?));
    }
    Ok(match t {
        ty::NO_DATA => PropertyValue::NoData,
        ty::BOOL => PropertyValue::Bool(bool_bit),
        ty::ONE_BYTE => PropertyValue::U8(r.u8()?),
        ty::TWO_BYTES => PropertyValue::U16(r.u16()?),
        ty::FOUR_BYTES => PropertyValue::U32(r.u32()?),
        ty::EIGHT_BYTES => PropertyValue::U64(r.u64()?),
        ty::FOUR_BYTES_OF_LENGTH_FOLLOWED_BY_DATA => {
            let cb = r.u32()? as usize;
            PropertyValue::Bytes(r.bytes(cb)?.to_vec())
        }
        ty::ARRAY_OF_PROPERTY_VALUES => {
            let n = r.u32()? as usize;
            if n == 0 {
                return Ok(PropertyValue::Sets(Vec::new()));
            }
            let prid = PropertyId::new(r.u32()?);
            if prid.ty() != ty::PROPERTY_SET {
                return Err(Error::Corrupt(format!("array of property values of type {:#x}", prid.ty())));
            }
            if n.saturating_mul(2) > r.remaining() {
                return Err(Error::Corrupt("array of property values longer than its data".into()));
            }
            let mut sets = Vec::with_capacity(n);
            for _ in 0..n {
                sets.push(read_set(r, s, depth + 1)?);
            }
            PropertyValue::Sets(sets)
        }
        ty::PROPERTY_SET => PropertyValue::Sets(vec![read_set(r, s, depth + 1)?]),
        other => return Err(Error::Corrupt(format!("unknown property type {other:#x}"))),
    })
}

/// Which kinds of references an encoded property set holds (for the declaration's
/// `fHasOidReferences` / `fHasOsidReferences` bits).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Refs {
    pub oids: bool,
    pub osids: bool,
}

struct OutStreams<'a> {
    oids: Vec<u8>,
    osids: Vec<u8>,
    ctxs: Vec<u8>,
    n: [u32; 3],
    compact: &'a dyn Fn(ExtendedGuid) -> Result<CompactId>,
}

/// Encodes `set` as an ObjectSpaceObjectPropSet (padded to a multiple of 8 bytes), compacting
/// references with `compact`.
pub fn encode(set: &PropertySet, compact: &dyn Fn(ExtendedGuid) -> Result<CompactId>) -> Result<(Vec<u8>, Refs)> {
    let mut out = OutStreams { oids: Vec::new(), osids: Vec::new(), ctxs: Vec::new(), n: [0; 3], compact };
    let mut body = Vec::new();
    write_set(&mut body, set, &mut out, 0)?;
    let has_ctx = out.n[2] > 0;
    let has_osid = out.n[1] > 0 || has_ctx;
    let mut data = Vec::with_capacity(16 + out.oids.len() + out.osids.len() + out.ctxs.len() + body.len());
    let header = |count: u32, extended: bool, osid_absent: bool| (count & 0x00FF_FFFF) | (u32::from(extended) << 30) | (u32::from(osid_absent) << 31);
    data.put_u32(header(out.n[0], has_ctx, !has_osid));
    data.put(&out.oids);
    if has_osid {
        data.put_u32(header(out.n[1], has_ctx, false));
        data.put(&out.osids);
        if has_ctx {
            data.put_u32(header(out.n[2], false, false));
            data.put(&out.ctxs);
        }
    }
    data.put(&body);
    while !data.len().is_multiple_of(8) {
        data.push(0);
    }
    Ok((data, Refs { oids: out.n[0] > 0, osids: has_osid }))
}

fn write_set(out: &mut Vec<u8>, set: &PropertySet, s: &mut OutStreams<'_>, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(Error::Limit("property sets nested too deeply".into()));
    }
    let count = u16::try_from(set.props.len()).map_err(|_| Error::Limit("more than 65535 properties in a set".into()))?;
    out.put_u16(count);
    for (id, v) in &set.props {
        let bool_bit = matches!(v, PropertyValue::Bool(true)) && id.ty() == ty::BOOL;
        out.put_u32(id.0 & 0x7FFF_FFFF | (u32::from(bool_bit) << 31));
    }
    for (id, v) in &set.props {
        write_value(out, *id, v, s, depth)?;
    }
    Ok(())
}

fn mismatch(id: PropertyId) -> Error {
    Error::Corrupt(format!("property {:#010x}: value does not match its type", id.0))
}

fn write_value(out: &mut Vec<u8>, id: PropertyId, v: &PropertyValue, s: &mut OutStreams<'_>, depth: usize) -> Result<()> {
    let t = id.ty();
    if let Some((stream, array)) = stream_of(t) {
        let PropertyValue::Ids(ids) = v else { return Err(mismatch(id)) };
        if !array && ids.len() != 1 {
            return Err(mismatch(id));
        }
        if array {
            out.put_u32(u32::try_from(ids.len()).map_err(|_| mismatch(id))?);
        }
        let idx = match stream {
            Stream::Oid => 0,
            Stream::Osid => 1,
            Stream::Ctx => 2,
        };
        for g in ids {
            let c = (s.compact)(*g)?.to_u32();
            match stream {
                Stream::Oid => s.oids.put_u32(c),
                Stream::Osid => s.osids.put_u32(c),
                Stream::Ctx => s.ctxs.put_u32(c),
            }
            s.n[idx] += 1;
        }
        return Ok(());
    }
    match (t, v) {
        (ty::NO_DATA, _) | (ty::BOOL, PropertyValue::Bool(_)) => {}
        (ty::ONE_BYTE, PropertyValue::U8(x)) => out.put_u8(*x),
        (ty::TWO_BYTES, PropertyValue::U16(x)) => out.put_u16(*x),
        (ty::FOUR_BYTES, PropertyValue::U32(x)) => out.put_u32(*x),
        (ty::EIGHT_BYTES, PropertyValue::U64(x)) => out.put_u64(*x),
        (ty::FOUR_BYTES_OF_LENGTH_FOLLOWED_BY_DATA, PropertyValue::Bytes(b)) => {
            let cb = u32::try_from(b.len()).ok().filter(|&n| n < 0x4000_0000).ok_or_else(|| Error::Limit("property data too large".into()))?;
            out.put_u32(cb);
            out.put(b);
        }
        (ty::ARRAY_OF_PROPERTY_VALUES, PropertyValue::Sets(sets)) => {
            out.put_u32(u32::try_from(sets.len()).map_err(|_| mismatch(id))?);
            if !sets.is_empty() {
                out.put_u32(u32::from(ty::PROPERTY_SET) << 26);
                for set in sets {
                    write_set(out, set, s, depth + 1)?;
                }
            }
        }
        (ty::PROPERTY_SET, PropertyValue::Sets(sets)) => match sets.as_slice() {
            [one] => write_set(out, one, s, depth + 1)?,
            _ => return Err(mismatch(id)),
        },
        _ => return Err(mismatch(id)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eg(b: u8, n: u32) -> ExtendedGuid {
        ExtendedGuid::new([b; 16], n)
    }

    fn sample() -> PropertySet {
        let mut nested = PropertySet::default();
        nested.push(0x20003488, PropertyValue::Ids(vec![eg(2, 7)])).push(0x1400346E, PropertyValue::U32(42));
        let mut s = PropertySet::default();
        s.push(0x08001C04, PropertyValue::Bool(true))
            .push(0x08001C05, PropertyValue::Bool(false))
            .push(0x04001C00, PropertyValue::NoData)
            .push(0x0C001C03, PropertyValue::U8(1))
            .push(0x10001C0B, PropertyValue::U16(22))
            .push(0x14001C01, PropertyValue::U32(17.0f32.to_bits()))
            .push(0x18001C65, PropertyValue::U64(0x01D9_0000_0000_0000))
            .push(0x1C001C22, PropertyValue::Bytes(b"h\0i\0".to_vec()))
            .push(0x24001C20, PropertyValue::Ids(vec![eg(1, 1), eg(1, 2)]))
            .push(0x2C001D63, PropertyValue::Ids(vec![eg(3, 1)]))
            .push(0x3400347B, PropertyValue::Ids(vec![eg(4, 1)]))
            .push(0x40003489, PropertyValue::Sets(vec![nested.clone(), nested]))
            .push(0x44000001, PropertyValue::Sets(vec![PropertySet::default()]));
        s
    }

    #[test]
    fn encode_decode_round_trip() {
        let set = sample();
        let table = [eg(1, 0).guid, eg(2, 0).guid, eg(3, 0).guid, eg(4, 0).guid];
        let compact = |g: ExtendedGuid| -> Result<CompactId> {
            let i = table.iter().position(|t| *t == g.guid).unwrap() as u32;
            Ok(CompactId { n: g.n as u8, guid_index: i })
        };
        let (bytes, refs) = encode(&set, &compact).unwrap();
        assert_eq!(bytes.len() % 8, 0);
        assert_eq!(refs, Refs { oids: true, osids: true });
        let resolve = |c: CompactId| ExtendedGuid::new(table[c.guid_index as usize], u32::from(c.n));
        let back = decode(&bytes, &resolve).unwrap();
        assert_eq!(back, set);
        assert_eq!(back.object_refs(), vec![eg(1, 1), eg(1, 2), eg(2, 7), eg(2, 7)]);
        assert_eq!(back.f32(0x14001C01), Some(17.0));
        assert_eq!(back.bool(0x08001C04), Some(true));
        assert_eq!(back.uint(0x10001C0B), Some(22));
    }

    #[test]
    fn hostile_sets_are_errors() {
        let resolve = |_| ExtendedGuid::NIL;
        // Claims 3 oids with no data.
        assert!(decode(&[3, 0, 0, 0x80], &resolve).is_err());
        // 0xFFFF properties in 4 bytes.
        assert!(decode(&[0, 0, 0, 0x80, 0xFF, 0xFF], &resolve).is_err());
        // Unknown type 0x1F.
        assert!(decode(&[0, 0, 0, 0x80, 1, 0, 0, 0, 0, 0x7C], &resolve).is_err());
        // Deep nesting: a property set holding a property set ... 100 levels.
        let mut deep = vec![0, 0, 0, 0x80];
        for _ in 0..100 {
            deep.extend_from_slice(&[1, 0, 0, 0, 0, 0x44]);
        }
        assert!(matches!(decode(&deep, &resolve), Err(Error::Limit(_))));
        // Value / type mismatch on write.
        let mut bad = PropertySet::default();
        bad.push(0x14001C01, PropertyValue::Bool(true));
        assert!(encode(&bad, &|_| Ok(CompactId::default())).is_err());
    }
}
