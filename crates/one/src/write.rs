//! Writing a section: the model → [MS-ONE] objects, following the structures and the required
//! properties MS-ONE gives (and its §3.1 example).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use notecraft_doc::page::{MAX_INDENT, TITLE_ORIGIN};
use notecraft_doc::{Align, ElementKind, Guid, ImageElement, ListStyle, Outline, Page, PageElement, ParaStyle, Paragraph, Point, Run, Section, Tag};
use notecraft_onestore::{ExtendedGuid, FileKind, ObjectSpace, PropertySet, PropertyValue, Store};

use crate::build::{SpaceBuilder, element_object_id};
use crate::ids::{LCID_EN_US, METADATA_SALT, SCHEMA_REVISION, jcid, prop, role};
use crate::util::{f32v, pt_to_hi, to_colorref, to_filetime, to_time32, wz_bytes};
use crate::{Result, ink, opaque};

/// What the writer records as the producer.
pub const WRITER_NAME: &str = concat!("NoteCraft ", env!("CARGO_PKG_VERSION"));

/// Page size written (half-inches): 8.5 × 11 in, with OneNote's default margins.
const PAGE_WIDTH: f32 = 17.0;
const PAGE_HEIGHT: f32 = 22.0;

/// Builds the revision store for a section.
pub fn section_store(section: &Section, file_name: &str, ancestor: Guid) -> Result<Store> {
    let now = notecraft_doc::now();
    let root_id = ExtendedGuid::from_guid(Guid::new(), 1);
    let mut root = SpaceBuilder::new(root_id);
    let mut spaces = Vec::with_capacity(section.pages.len() + 1);
    // Page series: a top-level page and the subpages after it.
    let mut series: Vec<Vec<(ExtendedGuid, PropertySet, DateTime<Utc>)>> = Vec::new();
    for page in &section.pages {
        let (space, meta) = page_space(page)?;
        if page.level == 0 || series.is_empty() {
            series.push(Vec::new());
        }
        if let Some(s) = series.last_mut() {
            s.push((space.id, meta, page.created));
        }
        spaces.push(space);
    }
    let mut series_ids = Vec::with_capacity(series.len());
    for s in &series {
        let mut metas = Vec::with_capacity(s.len());
        for (space_id, meta, _) in s {
            // A copy of the page's metadata, its id the page space's GUID xor the salt (§2.2.81).
            let mut g = space_id.guid;
            for (b, salt) in g.iter_mut().zip(METADATA_SALT) {
                *b ^= salt;
            }
            metas.push(root.add_as(ExtendedGuid::new(g, space_id.n), jcid::PAGE_METADATA, meta.clone()));
        }
        let created = s.first().map_or(now, |(_, _, c)| *c);
        let mut p = PropertySet::default();
        p.push(prop::NOTEBOOK_MANAGEMENT_ENTITY_GUID, PropertyValue::Bytes(Guid::new().0.to_vec()))
            .push(prop::CHILD_GRAPH_SPACE_ELEMENT_NODES, PropertyValue::Ids(s.iter().map(|(id, _, _)| *id).collect()))
            .push(prop::TOPOLOGY_CREATION_TIME_STAMP, PropertyValue::U64(to_filetime(created)))
            .push(prop::META_DATA_OBJECTS_ABOVE_GRAPH_SPACE, PropertyValue::Ids(metas));
        series_ids.push(root.add(jcid::PAGE_SERIES_NODE, p));
    }
    let mut node = PropertySet::default();
    node.push(prop::NOTEBOOK_MANAGEMENT_ENTITY_GUID, PropertyValue::Bytes(section.id.0.to_vec()));
    if !series_ids.is_empty() {
        node.push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(series_ids));
    }
    node.push(prop::TOPOLOGY_CREATION_TIME_STAMP, PropertyValue::U64(to_filetime(section.pages.first().map_or(now, |p| p.created))));
    let node_id = root.add(jcid::SECTION_NODE, node);
    let mut meta = PropertySet::default();
    meta.push(prop::SCHEMA_REVISION_IN_ORDER_TO_READ, PropertyValue::U32(SCHEMA_REVISION))
        .push(prop::SCHEMA_REVISION_IN_ORDER_TO_WRITE, PropertyValue::U32(SCHEMA_REVISION))
        .push(prop::SECTION_DISPLAY_NAME, wz_bytes(&section.name))
        .push(prop::NOTEBOOK_COLOR, PropertyValue::U32(to_colorref(section.color)))
        .push(prop::NC_WRITER, wz_bytes(WRITER_NAME));
    let meta_id = root.add(jcid::SECTION_METADATA, meta);
    root.root(role::CONTENT, node_id);
    root.root(role::METADATA, meta_id);
    spaces.insert(0, root.finish());
    Ok(Store {
        kind: FileKind::Section,
        file_id: if section.id.is_nil() { Guid::new() } else { section.id },
        ancestor,
        file_name: file_name.to_owned(),
        root: root_id,
        spaces,
        warnings: Vec::new(),
    })
}

/// Shared objects of one page: paragraph styles, text formats and note tag definitions.
struct PageWriter {
    b: SpaceBuilder,
    modified: u32,
    created: u32,
    styles: HashMap<&'static str, ExtendedGuid>,
    formats: HashMap<String, ExtendedGuid>,
    /// The language of the paragraph being written (LanguageID of its runs).
    lcid: u32,
    /// Tag definitions by NoteCraft tag name.
    tags: HashMap<&'static str, ExtendedGuid>,
}

/// Writes one page as its object space; also returns the page metadata (copied into the
/// section's page series).
pub(crate) fn page_space(page: &Page) -> Result<(ObjectSpace, PropertySet)> {
    let id = ExtendedGuid::from_guid(if page.id.is_nil() { Guid::new() } else { page.id }, 1);
    let mut w = PageWriter {
        b: SpaceBuilder::new(id),
        modified: to_time32(page.modified),
        created: to_time32(page.created),
        styles: HashMap::new(),
        formats: HashMap::new(),
        lcid: LCID_EN_US,
        tags: HashMap::new(),
    };
    let title = w.title(page);
    let mut children = Vec::new();
    for e in &page.elements {
        w.element(e, &mut children);
    }
    for s in &page.ink {
        if let Some(n) = ink::write_stroke(&mut w.b, s, w.modified) {
            children.push(n);
        }
    }
    let mut p = PropertySet::default();
    p.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(w.modified));
    if !children.is_empty() {
        p.push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(children));
    }
    p.push(prop::STRUCTURE_ELEMENT_CHILD_NODES, PropertyValue::Ids(vec![title]))
        .push(prop::PAGE_WIDTH, f32v(PAGE_WIDTH))
        .push(prop::PAGE_HEIGHT, f32v(PAGE_HEIGHT))
        .push(prop::PAGE_MARGIN_LEFT, f32v(2.0))
        .push(prop::PAGE_MARGIN_RIGHT, f32v(2.0))
        .push(prop::PAGE_MARGIN_TOP, f32v(1.0))
        .push(prop::PAGE_MARGIN_BOTTOM, f32v(1.0))
        // Offsets are from the page's top-left corner, like the model's positions.
        .push(prop::PAGE_MARGIN_ORIGIN_X, f32v(0.0))
        .push(prop::PAGE_MARGIN_ORIGIN_Y, f32v(0.0))
        .push(prop::CACHED_TITLE_STRING_FROM_PAGE, wz_bytes(if page.title.trim().is_empty() { "Untitled page" } else { "" }))
        .push(prop::LAYOUT_RESOLVE_CHILD_COLLISIONS, PropertyValue::Bool(true))
        .push(prop::PAGE_SIZE, PropertyValue::U32(0));
    if let Some(c) = page.background.color {
        p.push(prop::NC_PAGE_COLOR, PropertyValue::U32(u32::from(c.r) | (u32::from(c.g) << 8) | (u32::from(c.b) << 16) | (u32::from(c.a) << 24)));
    }
    if page.background.rule != notecraft_doc::RuleLines::None {
        p.push(prop::NC_RULE_LINES, wz_bytes(page.background.rule.name()));
    }
    let node = w.b.add(jcid::PAGE_NODE, p);
    let mut manifest = PropertySet::default();
    manifest.push(prop::CONTENT_CHILD_NODES, PropertyValue::Ids(vec![node]));
    let manifest_id = w.b.add(jcid::PAGE_MANIFEST_NODE, manifest);
    let mut meta = PropertySet::default();
    meta.push(prop::NOTEBOOK_MANAGEMENT_ENTITY_GUID, PropertyValue::Bytes(id.guid.to_vec()))
        .push(prop::CACHED_TITLE_STRING, wz_bytes(&page.title))
        .push(prop::SCHEMA_REVISION_IN_ORDER_TO_READ, PropertyValue::U32(SCHEMA_REVISION))
        .push(prop::SCHEMA_REVISION_IN_ORDER_TO_WRITE, PropertyValue::U32(SCHEMA_REVISION))
        .push(prop::PAGE_LEVEL, PropertyValue::U32(u32::from(page.level.min(notecraft_doc::page::MAX_PAGE_LEVEL)) + 1))
        .push(prop::TOPOLOGY_CREATION_TIME_STAMP, PropertyValue::U64(to_filetime(page.created)));
    let meta_id = w.b.add(jcid::PAGE_METADATA, meta.clone());
    let mut rev = PropertySet::default();
    rev.push(prop::LAST_MODIFIED_TIME_STAMP, PropertyValue::U64(to_filetime(page.modified)));
    let rev_id = w.b.add(jcid::REVISION_METADATA, rev);
    w.b.root(role::CONTENT, manifest_id);
    w.b.root(role::METADATA, meta_id);
    w.b.root(role::VERSION_METADATA, rev_id);
    let mut space = w.b.finish();
    crate::versions::attach(&mut space, manifest_id, page)?;
    Ok((space, meta))
}

fn indents() -> PropertyValue {
    // RgOutlineIndentDistance (§2.2.2): count, 3 unused bytes, a float per level.
    let n = usize::from(MAX_INDENT) + 1;
    let mut b = vec![n as u8, 0, 0, 0];
    for _ in 0..n {
        b.extend_from_slice(&0.5f32.to_le_bytes());
    }
    PropertyValue::Bytes(b)
}

/// OneNote's name for a NoteCraft paragraph style, and its font size (points).
fn style_name(s: ParaStyle) -> &'static str {
    match s {
        ParaStyle::Normal => "p",
        ParaStyle::Heading1 => "h1",
        ParaStyle::Heading2 => "h2",
        ParaStyle::Heading3 => "h3",
        ParaStyle::Heading4 => "h4",
        ParaStyle::Heading5 => "h5",
        ParaStyle::Heading6 => "h6",
        ParaStyle::Title => "PageTitle",
        ParaStyle::Quote => "blockquote",
        ParaStyle::Code => "code",
    }
}

impl PageWriter {
    /// A jcidParagraphStyleObject, shared by name.
    fn style(&mut self, name: &'static str, size: f32, gray: bool) -> ExtendedGuid {
        if let Some(id) = self.styles.get(name) {
            return *id;
        }
        let mut p = PropertySet::default();
        p.push(prop::FONT, wz_bytes(if name == "code" { "Consolas" } else { "Calibri" }))
            .push(prop::FONT_SIZE, PropertyValue::U16((size * 2.0).round() as u16))
            .push(prop::FONT_COLOR, PropertyValue::U32(if gray { 0x0080_8080 } else { 0xFF00_0000 }))
            .push(prop::HIGHLIGHT, PropertyValue::U32(0xFF00_0000))
            .push(prop::PARAGRAPH_STYLE_ID, wz_bytes(name));
        let id = self.b.add(jcid::PARAGRAPH_STYLE, p);
        self.styles.insert(name, id);
        id
    }

    /// A jcidParagraphStyleObjectForText for a run's formatting, shared between equal formats.
    fn format(&mut self, run: &Run, hyperlink: bool, hidden: bool) -> ExtendedGuid {
        let key = format!(
            "{}{}{}{}|{:?}|{:?}|{:?}|{:?}|{hyperlink}{hidden}|{}",
            run.bold as u8, run.italic as u8, run.underline as u8, run.strike as u8, run.font, run.size, run.color, run.highlight, self.lcid
        );
        if let Some(id) = self.formats.get(&key) {
            return *id;
        }
        let mut p = PropertySet::default();
        for (on, id) in [(run.bold, prop::BOLD), (run.italic, prop::ITALIC), (run.underline, prop::UNDERLINE), (run.strike, prop::STRIKETHROUGH)] {
            if on {
                p.push(id, PropertyValue::Bool(true));
            }
        }
        if run.underline {
            p.push(prop::UNDERLINE_TYPE, PropertyValue::U8(1));
        }
        if let Some(f) = run.font.as_deref().filter(|f| !f.is_empty()) {
            p.push(prop::FONT, wz_bytes(f));
        }
        if let Some(s) = run.size.filter(|s| s.is_finite() && *s > 0.0) {
            let half_points = (s * 2.0).round().clamp(12.0, 288.0);
            p.push(prop::FONT_SIZE, PropertyValue::U16(half_points as u16));
            if (half_points / 2.0 - s).abs() > f32::EPSILON {
                p.push(prop::NC_FONT_SIZE, f32v(s));
            }
        }
        if let Some(c) = run.color {
            p.push(prop::FONT_COLOR, PropertyValue::U32(to_colorref(c)));
        }
        if let Some(c) = run.highlight {
            p.push(prop::HIGHLIGHT, PropertyValue::U32(to_colorref(c)));
        }
        let (ca, ha) = (run.color.map_or(0xFF, |c| c.a), run.highlight.map_or(0xFF, |c| c.a));
        if ca != 0xFF || ha != 0xFF {
            p.push(prop::NC_RUN_ALPHA, PropertyValue::U16((u16::from(ca) << 8) | u16::from(ha)));
        }
        p.push(prop::LANGUAGE_ID, PropertyValue::U32(self.lcid));
        if hyperlink {
            p.push(prop::HYPERLINK, PropertyValue::Bool(true));
            if !hidden {
                p.push(prop::HYPERLINK_PROTECTED, PropertyValue::Bool(true));
            }
        }
        if hidden {
            p.push(prop::HIDDEN, PropertyValue::Bool(true));
        }
        let id = self.b.add(jcid::PARAGRAPH_STYLE, p);
        self.formats.insert(key, id);
        id
    }

    /// The title: a jcidTitleNode holding the title text outline and the date/time outline.
    fn title(&mut self, page: &Page) -> ExtendedGuid {
        let m = self.modified;
        let title_style = self.style("PageTitle", 20.0, false);
        let date_style = self.style("PageDateTime", 10.0, true);
        let plain = self.format(&Run::default(), false, false);
        let mut rt = PropertySet::default();
        rt.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(m))
            .push(prop::TEXT_RUN_FORMATTING, PropertyValue::Ids(vec![plain]))
            .push(prop::PARAGRAPH_STYLE, PropertyValue::Ids(vec![title_style]))
            .push(prop::RICH_EDIT_TEXT_UNICODE, wz_bytes(&page.title.replace(['\n', '\r'], " ")))
            .push(prop::IS_TITLE_TEXT, PropertyValue::Bool(true))
            .push(prop::RICH_EDIT_TEXT_LANG_ID, PropertyValue::U16(LCID_EN_US as u16));
        let rt = self.b.add(jcid::RICH_TEXT_OE_NODE, rt);
        let mut oe = self.oe_props(rt, &[], None);
        oe.push(prop::CANNOT_BE_SELECTED, PropertyValue::Bool(true)).push(prop::IS_TITLE_TEXT, PropertyValue::Bool(true));
        let oe = self.b.add(jcid::OUTLINE_ELEMENT_NODE, oe);
        let mut text = PropertySet::default();
        text.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(m))
            .push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(vec![oe]))
            .push(prop::ENFORCE_OUTLINE_STRUCTURE, PropertyValue::Bool(true))
            .push(prop::OUTLINE_ELEMENT_CHILD_LEVEL, PropertyValue::U8(1))
            .push(prop::LAYOUT_MAX_HEIGHT, f32v(0.6))
            .push(prop::RG_OUTLINE_INDENT_DISTANCE, indents())
            .push(prop::BODY_TEXT_ALIGNMENT, PropertyValue::U8(0))
            .push(prop::CANNOT_BE_SELECTED, PropertyValue::Bool(true))
            .push(prop::IS_TITLE_TEXT, PropertyValue::Bool(true))
            .push(prop::DESCENDANTS_CANNOT_BE_MOVED, PropertyValue::Bool(true))
            .push(prop::LAYOUT_MINIMUM_OUTLINE_WIDTH, f32v(4.5))
            .push(prop::LAYOUT_TIGHT_ALIGNMENT, PropertyValue::Bool(true));
        let text = self.b.add(jcid::OUTLINE_NODE, text);
        let date_oes = [
            (page.created.format("%A, %B %d, %Y").to_string(), prop::IS_TITLE_DATE),
            (page.created.format("%-I:%M %p").to_string(), prop::IS_TITLE_TIME),
        ]
        .into_iter()
        .map(|(s, flag)| {
            let mut rt = PropertySet::default();
            rt.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(m))
                .push(prop::LAYOUT_TIGHT_LAYOUT, PropertyValue::Bool(true))
                .push(prop::TEXT_RUN_FORMATTING, PropertyValue::Ids(vec![plain]))
                .push(prop::PARAGRAPH_STYLE, PropertyValue::Ids(vec![date_style]))
                .push(prop::RICH_EDIT_TEXT_UNICODE, wz_bytes(&s))
                .push(prop::IS_BOILER_TEXT, PropertyValue::Bool(true))
                .push(flag, PropertyValue::Bool(true))
                .push(prop::DELETABLE, PropertyValue::Bool(true))
                .push(prop::IS_READ_ONLY, PropertyValue::Bool(true))
                .push(prop::RICH_EDIT_TEXT_LANG_ID, PropertyValue::U16(LCID_EN_US as u16));
            let rt = self.b.add(jcid::RICH_TEXT_OE_NODE, rt);
            let mut oe = self.oe_props(rt, &[], None);
            oe.push(prop::DELETABLE, PropertyValue::Bool(true)).push(prop::CANNOT_BE_SELECTED, PropertyValue::Bool(true));
            self.b.add(jcid::OUTLINE_ELEMENT_NODE, oe)
        })
        .collect::<Vec<_>>();
        let mut date = PropertySet::default();
        date.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(m))
            .push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(date_oes))
            .push(prop::OUTLINE_ELEMENT_CHILD_LEVEL, PropertyValue::U8(1))
            .push(prop::LAYOUT_MAX_HEIGHT, f32v(0.6))
            .push(prop::RG_OUTLINE_INDENT_DISTANCE, indents())
            .push(prop::BODY_TEXT_ALIGNMENT, PropertyValue::U8(0))
            .push(prop::DELETABLE, PropertyValue::Bool(true))
            .push(prop::LAYOUT_TIGHT_LAYOUT, PropertyValue::Bool(true))
            .push(prop::IS_TITLE_DATE, PropertyValue::Bool(true))
            .push(prop::CANNOT_BE_SELECTED, PropertyValue::Bool(true))
            .push(prop::IS_READ_ONLY, PropertyValue::Bool(true));
        let date = self.b.add(jcid::OUTLINE_NODE, date);
        let mut t = PropertySet::default();
        t.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(m))
            .push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(vec![text, date]))
            .push(prop::OFFSET_FROM_PARENT_HORIZ, pt_to_hi(TITLE_ORIGIN.x))
            .push(prop::OFFSET_FROM_PARENT_VERT, pt_to_hi(TITLE_ORIGIN.y))
            .push(prop::LAYOUT_COLLISION_PRIORITY, PropertyValue::U32(0));
        self.b.add(jcid::TITLE_NODE, t)
    }

    /// An outline element's properties.
    fn oe_props(&mut self, content: ExtendedGuid, children: &[ExtendedGuid], list: Option<ExtendedGuid>) -> PropertySet {
        let mut oe = PropertySet::default();
        oe.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(self.modified));
        if !children.is_empty() {
            oe.push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(children.to_vec()));
        }
        oe.push(prop::CONTENT_CHILD_NODES, PropertyValue::Ids(vec![content]));
        if let Some(l) = list {
            oe.push(prop::LIST_NODES, PropertyValue::Ids(vec![l]));
        }
        oe.push(prop::OUTLINE_ELEMENT_CHILD_LEVEL, PropertyValue::U8(1)).push(prop::CREATION_TIME_STAMP, PropertyValue::U32(self.created));
        oe
    }

    fn element(&mut self, e: &PageElement, children: &mut Vec<ExtendedGuid>) {
        match &e.kind {
            ElementKind::Outline(o) => children.push(self.outline(e.id, o)),
            ElementKind::Image(img) => children.push(self.image(e.id, img, None)),
            ElementKind::EmbeddedFile { name, bytes, pos } => children.push(self.embedded_file(e.id, name, bytes, *pos, None)),
            ElementKind::Printout(p) => {
                let group = e.id;
                children.push(self.embedded_file(e.id, &p.file_name, &p.file_bytes, p.pos, Some((group, 0))));
                for (i, page) in p.pages.iter().enumerate() {
                    children.push(self.image(Guid::new(), page, Some((group, i as u32))));
                }
            }
            ElementKind::Opaque { data, .. } => {
                if let Some(id) = opaque::emit(&mut self.b, data) {
                    children.push(id);
                }
            }
        }
    }

    fn outline(&mut self, id: Guid, o: &Outline) -> ExtendedGuid {
        let tops = self.outline_tree(&o.paragraphs);
        let mut p = PropertySet::default();
        p.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(self.modified))
            .push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(tops))
            .push(prop::OUTLINE_ELEMENT_CHILD_LEVEL, PropertyValue::U8(1))
            .push(prop::LAYOUT_MAX_HEIGHT, f32v(0.4))
            .push(prop::IS_LAYOUT_SIZE_SET_BY_USER, PropertyValue::Bool(true))
            .push(prop::RG_OUTLINE_INDENT_DISTANCE, indents())
            .push(prop::BODY_TEXT_ALIGNMENT, PropertyValue::U8(0))
            .push(prop::OFFSET_FROM_PARENT_HORIZ, pt_to_hi(o.pos.x))
            .push(prop::OFFSET_FROM_PARENT_VERT, pt_to_hi(o.pos.y))
            .push(prop::LAYOUT_MAX_WIDTH, pt_to_hi(o.width.max(36.0)));
        self.b.add_as(element_object_id(id), jcid::OUTLINE_NODE, p)
    }

    /// Outline elements for `paragraphs`, nested by indent (an indent more than one deeper than
    /// the paragraph above is treated as one deeper); returns the top-level ones.
    fn outline_tree(&mut self, paragraphs: &[Paragraph]) -> Vec<ExtendedGuid> {
        let paragraphs: Vec<&Paragraph> = if paragraphs.is_empty() { Vec::new() } else { paragraphs.iter().collect() };
        let mut levels = Vec::with_capacity(paragraphs.len());
        let mut prev: Option<u8> = None;
        for p in &paragraphs {
            let l = match prev {
                None => 0,
                Some(pl) => p.indent.min(pl.saturating_add(1)).min(MAX_INDENT),
            };
            levels.push(l);
            prev = Some(l);
        }
        let empty = Paragraph::plain("");
        if paragraphs.is_empty() {
            return vec![self.paragraph(&empty, &[])];
        }
        self.subtree(&paragraphs, &levels, 0, paragraphs.len(), 0)
    }

    /// Outline elements for paragraphs `from..to`, whose top level is `level`.
    fn subtree(&mut self, paras: &[&Paragraph], levels: &[u8], from: usize, to: usize, level: u8) -> Vec<ExtendedGuid> {
        let mut out = Vec::new();
        let mut i = from;
        while i < to {
            let mut j = i + 1;
            while j < to && levels.get(j).copied().unwrap_or(0) > level {
                j += 1;
            }
            let children = if j > i + 1 { self.subtree(paras, levels, i + 1, j, level.saturating_add(1)) } else { Vec::new() };
            if let Some(p) = paras.get(i) {
                out.push(self.paragraph(p, &children));
            }
            i = j;
        }
        out
    }

    /// One paragraph: its rich text, list and tag, as an outline element.
    fn paragraph(&mut self, para: &Paragraph, children: &[ExtendedGuid]) -> ExtendedGuid {
        let style_id = style_name(para.style);
        let style = self.style(style_id, para.style.font_size(), false);
        let mut text: Vec<u16> = Vec::new();
        let mut ends = Vec::new();
        let mut formats = Vec::new();
        let runs: Vec<&Run> = para.runs.iter().filter(|r| !r.text.is_empty()).collect();
        let default = Run::default();
        let runs = if runs.is_empty() { vec![para.runs.first().unwrap_or(&default)] } else { runs };
        // The paragraph's proofing language (LanguageID on its runs, RichEditTextLangID on it).
        self.lcid = para.lang.as_deref().and_then(crate::lcid::from_tag).unwrap_or(LCID_EN_US);
        for run in runs {
            if let Some(url) = run.link.as_deref() {
                let field = format!("\u{FDDF}HYPERLINK \"{}\"", url.replace('"', "%22"));
                text.extend(field.encode_utf16());
                ends.push(text.len());
                formats.push(self.format(run, true, true));
            }
            text.extend(run.text.replace("\r\n", "\u{b}").replace(['\n', '\r'], "\u{b}").encode_utf16());
            ends.push(text.len());
            formats.push(self.format(run, run.link.is_some(), false));
        }
        ends.pop();
        let mut rt = PropertySet::default();
        rt.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(self.modified)).push(prop::TEXT_RUN_FORMATTING, PropertyValue::Ids(formats));
        if !ends.is_empty() {
            rt.push(prop::TEXT_RUN_INDEX, PropertyValue::Bytes(ends.iter().flat_map(|e| (*e as u32).to_le_bytes()).collect()));
        }
        let mut bytes: Vec<u8> = text.iter().flat_map(|u| u.to_le_bytes()).collect();
        bytes.extend_from_slice(&[0, 0]);
        rt.push(prop::PARAGRAPH_STYLE, PropertyValue::Ids(vec![style]))
            .push(prop::RICH_EDIT_TEXT_UNICODE, PropertyValue::Bytes(bytes))
            .push(prop::RICH_EDIT_TEXT_LANG_ID, PropertyValue::U16(u16::try_from(self.lcid).unwrap_or(LCID_EN_US as u16)));
        self.lcid = LCID_EN_US;
        if let Some(lang) = para.lang.as_deref() {
            rt.push(prop::NC_PARA_LANG, wz_bytes(lang));
        }
        if let Some(tag) = para.tag {
            let state = self.tag_state(tag);
            rt.push(prop::NOTE_TAG_STATES, PropertyValue::Sets(vec![state]));
        }
        // ParagraphAlignment (§2.3.94) knows left, center and right; justified text says left
        // there and keeps the rest in a private property.
        match para.align {
            Align::Left => {}
            Align::Center => {
                rt.push(prop::PARAGRAPH_ALIGNMENT, PropertyValue::U8(1));
            }
            Align::Right => {
                rt.push(prop::PARAGRAPH_ALIGNMENT, PropertyValue::U8(2));
            }
            Align::Justify => {
                rt.push(prop::PARAGRAPH_ALIGNMENT, PropertyValue::U8(0)).push(prop::NC_PARA_ALIGN, PropertyValue::U8(3));
            }
        }
        let rt = self.b.add(jcid::RICH_TEXT_OE_NODE, rt);
        let list = para.marker_style().map(|st| self.list_node(st));
        let oe = self.oe_props(rt, children, list);
        self.b.add(jcid::OUTLINE_ELEMENT_NODE, oe)
    }

    /// A jcidNumberListNode for a marker style. NumberListFormat (§2.3.20) is the character
    /// count, then the characters: a bullet character, or U+FFFD and an MSONFC number format
    /// ([MS-OSHARED] §2.2.1.3) followed by a full stop.
    fn list_node(&mut self, style: ListStyle) -> ExtendedGuid {
        let mut p = PropertySet::default();
        p.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(self.modified));
        match style.bullet_char() {
            Some(c) => {
                let mut units = [0u16; 2];
                let enc = c.encode_utf16(&mut units);
                let mut f: Vec<u16> = vec![enc.len() as u16];
                f.extend_from_slice(enc);
                // Calibri has • and –; the other shapes come from a symbol face.
                let font = if matches!(style, ListStyle::Disc | ListStyle::Dash) { "Calibri" } else { "Segoe UI Symbol" };
                p.push(prop::NUMBER_LIST_FORMAT, PropertyValue::Bytes(f.iter().flat_map(|u| u.to_le_bytes()).collect()))
                    .push(prop::LIST_FONT, wz_bytes(font))
                    .push(prop::LIST_MSAA_INDEX, PropertyValue::U16(1));
            }
            None => {
                let nfc: u16 = match style {
                    ListStyle::UpperRoman => 1,
                    ListStyle::LowerRoman => 2,
                    ListStyle::UpperAlpha => 3,
                    ListStyle::LowerAlpha => 4,
                    _ => 0,
                };
                let f = [3u16, 0xFFFD, nfc, u16::from(b'.')];
                p.push(prop::NUMBER_LIST_FORMAT, PropertyValue::Bytes(f.iter().flat_map(|u| u.to_le_bytes()).collect()))
                    .push(prop::FONT, wz_bytes("Calibri"))
                    .push(prop::LANGUAGE_ID, PropertyValue::U32(LCID_EN_US));
            }
        }
        self.b.add(jcid::NUMBER_LIST_NODE, p)
    }

    /// A NoteTagState (§2.2.42) and its shared definition. Shapes are MS-ONE's NoteTagShape
    /// values (§2.3.86): the highlight tags have no icon (0) and a NoteTagHighlightColor instead.
    fn tag_state(&mut self, tag: Tag) -> PropertySet {
        let (shape, checkable, checked) = match tag {
            Tag::ToDo { checked } => (3u16, true, checked),
            Tag::Important => (13, false, false),
            Tag::Question => (15, false, false),
            Tag::Critical => (17, false, false),
            Tag::Contact => (18, false, false),
            Tag::Idea => (21, false, false),
            Tag::Address => (23, false, false),
            Tag::Phone => (109, false, false),
            Tag::RememberForLater | Tag::Definition | Tag::Highlight => (0, false, false),
        };
        let item = Tag::ALL.iter().position(|t| t.name() == tag.name()).unwrap_or(0) as u16;
        let def = match self.tags.get(tag.name()) {
            Some(d) => *d,
            None => {
                let mut d = PropertySet::default();
                let highlight = tag.highlight();
                // NoteTagPropertyStatus: hasLabel, plus hasIcon or hasHighlightColor.
                let status = if highlight.is_some() { 0x1 | 0x4 } else { 0x1 | 0x8 };
                d.push(prop::NOTE_TAG_LABEL, wz_bytes(tag.label()))
                    .push(prop::NOTE_TAG_PROPERTY_STATUS, PropertyValue::U32(status))
                    .push(prop::NOTE_TAG_SHAPE, PropertyValue::U16(shape));
                if let Some(c) = highlight {
                    d.push(prop::NOTE_TAG_HIGHLIGHT_COLOR, PropertyValue::U32(to_colorref(c)));
                }
                d.push(prop::ACTION_ITEM_SCHEMA_VERSION, PropertyValue::U8(0))
                    .push(prop::ACTION_ITEM_TYPE, PropertyValue::U16(item))
                    .push(prop::NC_TAG, wz_bytes(tag.name()));
                let id = self.b.add(jcid::NOTE_TAG_DEFINITION, d);
                self.tags.insert(tag.name(), id);
                id
            }
        };
        let done = !checkable || checked;
        let mut s = PropertySet::default();
        s.push(prop::NOTE_TAG_DEFINITION_OID, PropertyValue::Ids(vec![def]))
            .push(prop::NOTE_TAG_CREATED, PropertyValue::U32(self.created))
            .push(prop::NOTE_TAG_COMPLETED, PropertyValue::U32(if done { self.modified } else { 0 }))
            .push(prop::ACTION_ITEM_STATUS, PropertyValue::U16(u16::from(done)));
        s
    }

    fn image(&mut self, id: Guid, img: &ImageElement, printout: Option<(Guid, u32)>) -> ExtendedGuid {
        let ext = format!(".{}", img.format.extension());
        let file = self.b.add_file(jcid::PICTURE_CONTAINER, img.data.clone(), &ext);
        let mut p = PropertySet::default();
        p.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(self.modified))
            .push(prop::PICTURE_CONTAINER, PropertyValue::Ids(vec![file]))
            .push(prop::LAYOUT_MAX_WIDTH, pt_to_hi(img.size.w))
            .push(prop::LAYOUT_MAX_HEIGHT, pt_to_hi(img.size.h))
            .push(prop::IS_LAYOUT_SIZE_SET_BY_USER, PropertyValue::Bool(true))
            .push(prop::LANGUAGE_ID, PropertyValue::U32(LCID_EN_US))
            .push(prop::PICTURE_WIDTH, pt_to_hi(img.size.w))
            .push(prop::PICTURE_HEIGHT, pt_to_hi(img.size.h))
            .push(prop::OFFSET_FROM_PARENT_HORIZ, pt_to_hi(img.pos.x))
            .push(prop::OFFSET_FROM_PARENT_VERT, pt_to_hi(img.pos.y));
        if !img.alt.is_empty() {
            p.push(prop::IMAGE_ALT_TEXT, wz_bytes(&img.alt));
        }
        if let Some(tag) = printout {
            p.push(prop::NC_PRINTOUT, printout_value(tag));
        }
        self.b.add_as(element_object_id(id), jcid::IMAGE_NODE, p)
    }

    fn embedded_file(&mut self, id: Guid, name: &str, bytes: &Arc<Vec<u8>>, pos: Point, printout: Option<(Guid, u32)>) -> ExtendedGuid {
        let ext = name.rsplit_once('.').map(|(_, e)| format!(".{e}")).unwrap_or_default();
        let file = self.b.add_file(jcid::EMBEDDED_FILE_CONTAINER, bytes.clone(), &ext);
        let icon = self.b.add_file(jcid::PICTURE_CONTAINER, Arc::new(crate::icon::file_icon_png()), ".png");
        let shown = name.replace(['\n', '\r'], " ");
        let mut p = PropertySet::default();
        p.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(self.modified))
            .push(prop::PICTURE_CONTAINER, PropertyValue::Ids(vec![icon]))
            .push(prop::LAYOUT_MAX_WIDTH, f32v(1.0))
            .push(prop::LAYOUT_MAX_HEIGHT, f32v(1.5))
            .push(prop::LANGUAGE_ID, PropertyValue::U32(LCID_EN_US))
            .push(prop::RICH_EDIT_TEXT_UNICODE, wz_bytes(&shown))
            .push(prop::EMBEDDED_FILE_CONTAINER, PropertyValue::Ids(vec![file]))
            .push(prop::EMBEDDED_FILE_NAME, wz_bytes(name))
            .push(prop::RICH_EDIT_TEXT_LANG_ID, PropertyValue::U16(LCID_EN_US as u16))
            .push(prop::OFFSET_FROM_PARENT_HORIZ, pt_to_hi(pos.x))
            .push(prop::OFFSET_FROM_PARENT_VERT, pt_to_hi(pos.y));
        if let Some(tag) = printout {
            p.push(prop::NC_PRINTOUT, printout_value(tag));
        }
        self.b.add_as(element_object_id(id), jcid::EMBEDDED_FILE_NODE, p)
    }
}

fn printout_value((group, index): (Guid, u32)) -> PropertyValue {
    let mut b = group.0.to_vec();
    b.extend_from_slice(&index.to_le_bytes());
    PropertyValue::Bytes(b)
}
