//! Ink. [MS-ONE] doesn't document ink, so this follows the structure observed in sample files
//! (black-box; see `ids.rs` and `README.md`) and the public Ink Serialized Format spec for the
//! integer encoding and packet property GUIDs:
//!
//! ```text
//! page ElementChildNodes ─▶ ink node (0x00060014: position in half-inches)
//!                             └▶ ink data (0x0002003B) ─▶ strokes (0x00020047: path, GUID)
//!                                                           └▶ drawing attributes (0x00120048:
//!                                                              packet description, colour, width)
//! ```
//!
//! A stroke path is a multi-byte encoded count of values, then the values: signed multi-byte
//! encoded deltas, all X, then all Y (then pressure when the packet description has it), in the
//! description's units (HIMETRIC, 0.01 mm, in the samples) from the ink node's position.
//!
//! NoteCraft writes ink in that structure and also keeps each stroke exactly in a private
//! property ([`prop::NC_STROKE`]), so its own files round-trip whatever OneNote makes of them.

use notecraft_doc::geom::{PT_PER_INK_UNIT, sane};
use notecraft_doc::ink::{MAX_INK_PER_PAGE, MAX_POINTS};
use notecraft_doc::{Color, Guid, InkPoint, InkStroke, InkTool, Point};
use notecraft_onestore::bytes::{Reader, WriteLe};
use notecraft_onestore::{ExtendedGuid, ObjectSpace, PropertySet, PropertyValue};

use crate::build::SpaceBuilder;
use crate::ids::{jcid, prop};
use crate::util::{f32v, from_colorref, pt_to_hi, to_colorref};

/// ISF packet property GUIDs (on-disk byte order).
const GUID_X: [u8; 16] = [0x8F, 0x6A, 0x8A, 0x59, 0xC0, 0x52, 0xA0, 0x4B, 0x93, 0xAF, 0xAF, 0x35, 0x74, 0x11, 0xA5, 0x61];
const GUID_Y: [u8; 16] = [0x75, 0x9F, 0x3F, 0xB5, 0xE0, 0x04, 0x98, 0x44, 0xA7, 0xEE, 0xC3, 0x0D, 0xBB, 0x5A, 0x90, 0x11];
const GUID_NORMAL_PRESSURE: [u8; 16] = [0x2D, 0x50, 0x07, 0x73, 0xF4, 0xF9, 0x18, 0x4E, 0xB3, 0xF2, 0x2C, 0xE1, 0xB1, 0xA3, 0x61, 0x0C];
/// Size of one packet property description.
const DESC_SIZE: usize = 32;
/// ISF metric unit for centimetres.
const UNIT_CM: u32 = 2;
/// Pressure levels NoteCraft writes (0..=1023).
const PRESSURE_MAX: u32 = 1023;
/// Most values decoded from one path (three per point, bounded).
const MAX_PATH_VALUES: usize = MAX_POINTS * 4;

/// Appends an unsigned multi-byte encoded number ([ISF] "Multi-byte Encoding").
pub fn put_mb(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Appends a signed multi-byte encoded number: the absolute value shifted left one bit, the
/// sign in the low bit.
pub fn put_smb(out: &mut Vec<u8>, v: i64) {
    let mag = v.unsigned_abs();
    put_mb(out, (mag << 1) | u64::from(v < 0));
}

/// Decodes unsigned multi-byte numbers (at most `max` of them; a truncated last number is
/// dropped).
pub fn read_mbs(data: &[u8], max: usize) -> Vec<u64> {
    let mut out = Vec::new();
    let mut v = 0u64;
    let mut shift = 0u32;
    for &b in data {
        if shift < 64 {
            v |= u64::from(b & 0x7F) << shift;
        }
        shift += 7;
        if b & 0x80 == 0 {
            out.push(v);
            if out.len() >= max {
                break;
            }
            v = 0;
            shift = 0;
        }
    }
    out
}

pub fn signed(v: u64) -> i64 {
    let mag = (v >> 1).min(i64::MAX as u64) as i64;
    if v & 1 == 1 { -mag } else { mag }
}

/// One packet property in a drawing-attributes description.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Metric {
    guid: [u8; 16],
    max: i32,
    unit: u32,
    resolution: f32,
}

fn metrics(desc: &[u8]) -> Vec<Metric> {
    desc.as_chunks::<DESC_SIZE>()
        .0
        .iter()
        .filter_map(|c| {
            let mut r = Reader::new(c);
            let guid = r.guid().ok()?;
            let _min = r.u32().ok()?;
            let max = r.u32().ok()? as i32;
            let unit = r.u32().ok()?;
            let resolution = f32::from_bits(r.u32().ok()?);
            Some(Metric { guid, max, unit, resolution })
        })
        .collect()
}

fn desc_bytes(metrics: &[Metric]) -> Vec<u8> {
    let mut out = Vec::with_capacity(metrics.len() * DESC_SIZE);
    for m in metrics {
        out.put(&m.guid);
        out.put_u32(if m.guid == GUID_NORMAL_PRESSURE { 0 } else { 0x8000_0000 });
        out.put_u32(m.max as u32);
        out.put_u32(m.unit);
        out.put_u32(m.resolution.to_bits());
    }
    out
}

/// Points per unit of a coordinate metric (HIMETRIC when the description doesn't say).
fn pt_per_unit(m: Option<&Metric>) -> f32 {
    match m {
        Some(m) if m.unit == UNIT_CM && m.resolution.is_finite() && m.resolution > 0.0 => 72.0 / 2.54 / m.resolution,
        _ => PT_PER_INK_UNIT,
    }
}

/// The exact stroke in NoteCraft's private form.
fn encode_private(s: &InkStroke) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + s.points.len() * 12);
    out.put_u8(1); // version
    out.put(&s.id.0);
    out.put_u8(match s.tool {
        InkTool::Pen => 0,
        InkTool::Pencil => 1,
        InkTool::Highlighter => 2,
    });
    out.put(&[s.color.r, s.color.g, s.color.b, s.color.a]);
    out.put_u32(s.width.to_bits());
    out.put_u32(s.points.len().min(MAX_POINTS) as u32);
    for p in s.points.iter().take(MAX_POINTS) {
        out.put_u32(p.x.to_bits());
        out.put_u32(p.y.to_bits());
        out.put_u32(p.pressure.to_bits());
    }
    out
}

fn decode_private(b: &[u8]) -> Option<InkStroke> {
    let mut r = Reader::new(b);
    if r.u8().ok()? != 1 {
        return None;
    }
    let id = Guid(r.guid().ok()?);
    let tool = match r.u8().ok()? {
        1 => InkTool::Pencil,
        2 => InkTool::Highlighter,
        _ => InkTool::Pen,
    };
    let [cr, cg, cb, ca] = r.array::<4>().ok()?;
    let width = f32::from_bits(r.u32().ok()?);
    let n = (r.u32().ok()? as usize).min(MAX_POINTS);
    if n.saturating_mul(12) > r.remaining() {
        return None;
    }
    let mut points = Vec::with_capacity(n);
    for _ in 0..n {
        let x = f32::from_bits(r.u32().ok()?);
        let y = f32::from_bits(r.u32().ok()?);
        let p = f32::from_bits(r.u32().ok()?);
        points.push(InkPoint::new(x, y, p));
    }
    Some(InkStroke { id, points, color: Color::rgba(cr, cg, cb, ca), width, tool }.sanitized())
}

/// Writes one stroke as an ink node on the page; returns the node's id (for the page's
/// ElementChildNodes).
pub fn write_stroke(b: &mut SpaceBuilder, s: &InkStroke, modified: u32) -> Option<ExtendedGuid> {
    let s = s.clone().sanitized();
    let bbox = s.bbox()?;
    let origin = Point::new(bbox.x, bbox.y);
    let to_units = |v: f32| (sane(v) / PT_PER_INK_UNIT).round() as i64;
    let (mut xs, mut ys, mut ps) = (Vec::new(), Vec::new(), Vec::new());
    let (mut lx, mut ly, mut lp) = (0i64, 0i64, 0i64);
    for p in &s.points {
        let (x, y) = (to_units(p.x - origin.x), to_units(p.y - origin.y));
        let pr = (p.pressure.clamp(0.0, 1.0) * PRESSURE_MAX as f32).round() as i64;
        xs.push(x - lx);
        ys.push(y - ly);
        ps.push(pr - lp);
        (lx, ly, lp) = (x, y, pr);
    }
    let mut path = Vec::with_capacity(s.points.len() * 4 + 4);
    put_smb(&mut path, (xs.len() * 3) as i64);
    for v in xs.iter().chain(&ys).chain(&ps) {
        put_smb(&mut path, *v);
    }
    let coord = |guid| Metric { guid, max: i32::MAX, unit: UNIT_CM, resolution: 1000.0 };
    let desc = desc_bytes(&[coord(GUID_X), coord(GUID_Y), Metric { guid: GUID_NORMAL_PRESSURE, max: PRESSURE_MAX as i32, unit: 0, resolution: 1.0 }]);
    let width_units = sane(s.width) / PT_PER_INK_UNIT;
    let mut attrs = PropertySet::default();
    attrs
        .push(prop::INK_PACKET_DESCRIPTION, PropertyValue::Bytes(desc))
        .push(prop::INK_FLAG, PropertyValue::Bool(true))
        .push(prop::INK_COLOR, PropertyValue::U32(to_colorref(s.color)))
        .push(prop::INK_WIDTH, f32v(width_units))
        .push(prop::INK_HEIGHT, f32v(width_units));
    let attrs_id = b.add(jcid::INK_STROKE_PROPERTIES, attrs);
    let mut stroke = PropertySet::default();
    stroke
        .push(prop::INK_PATH, PropertyValue::Bytes(path))
        .push(prop::INK_STROKE_PROPERTIES, PropertyValue::Ids(vec![attrs_id]))
        .push(prop::INK_STROKE_GUID, PropertyValue::Bytes(s.id.0.to_vec()))
        .push(prop::NC_STROKE, PropertyValue::Bytes(encode_private(&s)));
    let stroke_id = b.add(jcid::INK_STROKE, stroke);
    let mut data = PropertySet::default();
    data.push(prop::INK_STROKES, PropertyValue::Ids(vec![stroke_id]));
    let data_id = b.add(jcid::INK_DATA, data);
    let mut node = PropertySet::default();
    node.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(modified))
        .push(prop::OFFSET_FROM_PARENT_HORIZ, pt_to_hi(origin.x))
        .push(prop::OFFSET_FROM_PARENT_VERT, pt_to_hi(origin.y))
        .push(prop::INK_DATA, PropertyValue::Ids(vec![data_id]))
        .push(prop::LAYOUT_ALIGNMENT_IN_PARENT, PropertyValue::U32(0));
    Some(b.add(jcid::INK_NODE, node))
}

/// Reads the strokes of an ink node at `origin` (page points).
pub fn read_ink_node(
    space: &ObjectSpace,
    index: &std::collections::HashMap<ExtendedGuid, usize>,
    node: &PropertySet,
    origin: Point,
    out: &mut Vec<InkStroke>,
) {
    let get = |id: &ExtendedGuid| index.get(id).and_then(|&i| space.objects.get(i));
    for data in node.ids(prop::INK_DATA).iter().filter_map(get) {
        for stroke in data.props.ids(prop::INK_STROKES).iter().filter_map(get) {
            if out.len() >= MAX_INK_PER_PAGE {
                return;
            }
            if let Some(s) = stroke.props.bytes(prop::NC_STROKE).and_then(decode_private) {
                out.push(s);
                continue;
            }
            let attrs = stroke.props.ids(prop::INK_STROKE_PROPERTIES).first().and_then(get).map(|o| &o.props);
            if let Some(s) = read_onenote_stroke(&stroke.props, attrs, origin, stroke.id) {
                out.push(s);
            }
        }
    }
}

fn read_onenote_stroke(stroke: &PropertySet, attrs: Option<&PropertySet>, origin: Point, oid: ExtendedGuid) -> Option<InkStroke> {
    let path = stroke.bytes(prop::INK_PATH)?;
    let metrics = attrs.and_then(|a| a.bytes(prop::INK_PACKET_DESCRIPTION)).map(metrics).unwrap_or_default();
    let dims = metrics.len().max(2);
    let values = read_mbs(path, MAX_PATH_VALUES + 1);
    let (count, rest) = values.split_first()?;
    let n = rest.len();
    // The count is the number of values that follow, as observed (written signed, so doubled).
    let n = [*count >> 1, *count].into_iter().map(|c| c as usize).find(|&c| c <= n && c % dims == 0 && c > 0).unwrap_or(n - n % dims);
    let points = (n / dims).min(MAX_POINTS);
    let find = |g: [u8; 16], default: usize| metrics.iter().position(|m| m.guid == g).unwrap_or(default);
    let (ix, iy) = (find(GUID_X, 0), find(GUID_Y, 1));
    let ip = metrics.iter().position(|m| m.guid == GUID_NORMAL_PRESSURE);
    let column = |i: usize| -> Vec<i64> {
        let mut acc = 0i64;
        rest.iter()
            .skip(i * (n / dims))
            .take(points)
            .map(|&v| {
                acc = acc.saturating_add(signed(v));
                acc
            })
            .collect()
    };
    let (xs, ys) = (column(ix), column(iy));
    let pressure_max = ip.and_then(|i| metrics.get(i)).map_or(1.0, |m| if m.max > 0 && m.max <= 65_535 { m.max as f32 } else { 1023.0 });
    let ps = ip.map(column);
    let sx = pt_per_unit(metrics.get(ix));
    let sy = pt_per_unit(metrics.get(iy));
    let pts: Vec<InkPoint> = xs
        .iter()
        .zip(&ys)
        .enumerate()
        .map(|(i, (&x, &y))| {
            let p = ps.as_ref().and_then(|p| p.get(i)).map_or(1.0, |&v| v as f32 / pressure_max);
            InkPoint::new(origin.x + x as f32 * sx, origin.y + y as f32 * sy, p)
        })
        .collect();
    if pts.is_empty() {
        return None;
    }
    let color = attrs.and_then(|a| a.u32(prop::INK_COLOR)).and_then(from_colorref).unwrap_or(Color::BLACK);
    let width = attrs.and_then(|a| a.f32(prop::INK_WIDTH)).map_or(InkTool::Pen.default_width(), |w| w * PT_PER_INK_UNIT);
    let id = stroke.bytes(prop::INK_STROKE_GUID).and_then(|b| <[u8; 16]>::try_from(b).ok()).map_or_else(|| crate::build::element_id(oid), Guid);
    Some(InkStroke { id, points: pts, color, width, tool: InkTool::Pen }.sanitized())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isf_multibyte_numbers() {
        let mut b = Vec::new();
        put_mb(&mut b, 127);
        put_mb(&mut b, 128);
        put_mb(&mut b, 300);
        assert_eq!(b, [0x7F, 0x80, 0x01, 0xAC, 0x02]);
        assert_eq!(read_mbs(&b, 10), vec![127, 128, 300]);
        let mut s = Vec::new();
        for v in [0i64, 1, -1, 653, -635, i64::MIN + 1] {
            put_smb(&mut s, v);
        }
        let back: Vec<i64> = read_mbs(&s, 10).into_iter().map(signed).collect();
        assert_eq!(back, vec![0, 1, -1, 653, -635, i64::MIN + 1]);
        // Truncated and endless input stays bounded.
        assert_eq!(read_mbs(&[0x80; 100], 10), Vec::<u64>::new());
        assert_eq!(read_mbs(&[0x01; 100], 10).len(), 10);
    }

    #[test]
    fn decodes_the_observed_sample_path() {
        // A two-point stroke observed in a sample file: 4 values, X then Y deltas.
        let path = [0x08, 0x9A, 0x0A, 0xF7, 0x09, 0x9A, 0x0A, 0xF7, 0x09];
        let mut stroke = PropertySet::default();
        stroke.push(prop::INK_PATH, PropertyValue::Bytes(path.to_vec()));
        let s = read_onenote_stroke(&stroke, None, Point::ZERO, ExtendedGuid::new([1; 16], 2)).unwrap();
        assert_eq!(s.points.len(), 2);
        assert!((s.points[0].x - 653.0 * PT_PER_INK_UNIT).abs() < 1e-3);
        assert!((s.points[1].y - 18.0 * PT_PER_INK_UNIT).abs() < 1e-3);
    }

    #[test]
    fn written_strokes_read_back_both_ways() {
        let stroke = InkStroke {
            id: Guid::new(),
            points: vec![InkPoint::new(100.0, 200.0, 0.25), InkPoint::new(150.5, 210.0, 1.0), InkPoint::new(160.0, 190.0, 0.5)],
            color: Color::rgb(0xED, 0x1C, 0x24),
            width: 2.0,
            tool: InkTool::Highlighter,
        };
        let mut b = SpaceBuilder::new(ExtendedGuid::from_guid(Guid::new(), 1));
        let node_id = write_stroke(&mut b, &stroke, 0).unwrap();
        let space = b.finish();
        let index = space.index();
        let node = &space.objects[index[&node_id]].props.clone();
        let origin = Point::new(
            crate::util::hi_to_pt(node.f32(prop::OFFSET_FROM_PARENT_HORIZ).unwrap()),
            crate::util::hi_to_pt(node.f32(prop::OFFSET_FROM_PARENT_VERT).unwrap()),
        );
        let mut exact = Vec::new();
        read_ink_node(&space, &index, node, origin, &mut exact);
        assert_eq!(exact, vec![stroke.clone()]);
        // Without the private copy: OneNote's form, within HIMETRIC rounding.
        let mut space = space;
        for o in &mut space.objects {
            o.props.props.retain(|(id, _)| id.0 != prop::NC_STROKE);
        }
        let mut approx = Vec::new();
        read_ink_node(&space, &index, node, origin, &mut approx);
        let a = &approx[0];
        assert_eq!((a.id, a.color, a.points.len()), (stroke.id, stroke.color, 3));
        for (p, q) in a.points.iter().zip(&stroke.points) {
            assert!((p.x - q.x).abs() < 0.05 && (p.y - q.y).abs() < 0.05 && (p.pressure - q.pressure).abs() < 0.01, "{p:?} {q:?}");
        }
        assert!((a.width - 2.0).abs() < 0.01);
    }

    #[test]
    fn hostile_private_strokes_are_ignored() {
        assert!(decode_private(&[]).is_none());
        assert!(decode_private(&[2; 64]).is_none());
        let mut b = vec![1u8];
        b.extend_from_slice(&[0; 16 + 1 + 4 + 4]);
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_private(&b).is_none());
    }
}
