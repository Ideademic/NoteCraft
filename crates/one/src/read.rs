//! Reading a section: [MS-ONE] objects → the model.

use std::collections::HashMap;
use std::sync::Arc;

use notecraft_doc::page::{DEFAULT_OUTLINE_WIDTH, MAX_ELEMENTS_PER_PAGE, MAX_INDENT, MAX_PAGE_LEVEL, MAX_PARAGRAPHS};
use notecraft_doc::{
    Align, Color, ElementKind, Guid, ImageElement, ImageFormat, InkStroke, ListKind, ListStyle, Outline, Page, PageBackground, PageElement,
    ParaStyle, Paragraph,
    Point, Printout, RuleLines, Run, Section, Size, SourceInfo, Tag,
};
use notecraft_onestore::{ExtendedGuid, FileKind, Object, ObjectSpace, PropertySet, Store};

use crate::build::element_id;
use crate::ids::{jcid, prop, role};
use crate::util::{extended_ascii, from_colorref, from_filetime, from_time32, guid, hi_to_pt, string, wz};
use crate::{Error, Result, ink, opaque};

/// Most pages read from one section.
const MAX_PAGES: usize = 10_000;
/// Deepest outline element nesting walked.
const MAX_OE_DEPTH: usize = 64;
/// Most cells flattened from one table.
const MAX_TABLE_CELLS: usize = 10_000;

/// An object space with an id index.
pub(crate) struct View<'a> {
    pub space: &'a ObjectSpace,
    pub index: HashMap<ExtendedGuid, usize>,
}

impl<'a> View<'a> {
    pub fn new(space: &'a ObjectSpace) -> View<'a> {
        View { space, index: space.index() }
    }

    pub fn get(&self, id: ExtendedGuid) -> Option<&'a Object> {
        self.index.get(&id).and_then(|&i| self.space.objects.get(i))
    }

    pub fn root(&self, r: u32) -> Option<&'a Object> {
        self.space.root(r).and_then(|id| self.get(id))
    }

    /// The objects an array-of-ids property references, skipping missing ones.
    pub fn children(&self, set: &PropertySet, id: u32) -> Vec<&'a Object> {
        set.ids(id).iter().filter_map(|i| self.get(*i)).collect()
    }
}

/// Reads a section from its revision store. `name` is used when the file holds no display name.
pub fn section(store: &Store, name: &str) -> Result<Section> {
    if store.kind != FileKind::Section {
        return Err(Error::Format("not a section (.one) file".into()));
    }
    let root = View::new(store.root_space().ok_or_else(|| Error::Format("root object space missing".into()))?);
    let node = root.root(role::CONTENT).filter(|o| o.jcid == jcid::SECTION_NODE).ok_or_else(|| Error::Format("no section node".into()))?;
    let meta = root.root(role::METADATA).map(|o| &o.props);
    let display = meta.and_then(|m| string(m, prop::SECTION_DISPLAY_NAME)).filter(|s| !s.trim().is_empty());
    let color = meta.and_then(|m| m.u32(prop::NOTEBOOK_COLOR)).and_then(from_colorref).unwrap_or(Color::section_color(0));
    let mut r = Reader { unsupported: !store.warnings.is_empty() };
    let mut pages = Vec::new();
    for series in root.children(&node.props, prop::ELEMENT_CHILD_NODES) {
        if series.jcid != jcid::PAGE_SERIES_NODE {
            r.unsupported = true;
            continue;
        }
        for sid in series.props.ids(prop::CHILD_GRAPH_SPACE_ELEMENT_NODES) {
            if pages.len() >= MAX_PAGES {
                r.unsupported = true;
                break;
            }
            match store.space(*sid).and_then(|s| r.page(s)) {
                Some(p) => pages.push(p),
                None => r.unsupported = true,
            }
        }
    }
    let authored = meta.is_some_and(|m| m.has(prop::NC_WRITER));
    Ok(Section {
        id: store.file_id,
        name: display.unwrap_or_else(|| name.to_owned()),
        color,
        pages,
        source: SourceInfo { authored_by_notecraft: authored, unsupported_content: r.unsupported, file_name: None },
    })
}

pub(crate) struct Reader {
    pub(crate) unsupported: bool,
}

/// A printout being put together from its file and page pictures.
struct PrintoutParts {
    group: Guid,
    /// Where it goes in the page's elements (a placeholder until the page is done).
    slot: usize,
    file: Option<(Guid, String, Arc<Vec<u8>>, Point)>,
    pages: Vec<(u32, ImageElement)>,
}

/// Everything collected from one page's elements.
#[derive(Default)]
struct PageParts {
    elements: Vec<PageElement>,
    ink: Vec<InkStroke>,
    printouts: Vec<PrintoutParts>,
}

impl PageParts {
    fn push(&mut self, e: PageElement) {
        if self.elements.len() < MAX_ELEMENTS_PER_PAGE {
            self.elements.push(e);
        }
    }

    fn printout(&mut self, group: Guid) -> Option<&mut PrintoutParts> {
        if !self.printouts.iter().any(|p| p.group == group) {
            if self.elements.len() >= MAX_ELEMENTS_PER_PAGE {
                return None;
            }
            let placeholder = Printout { file_name: String::new(), file_bytes: Arc::new(Vec::new()), pos: Point::ZERO, pages: Vec::new() };
            self.printouts.push(PrintoutParts { group, slot: self.elements.len(), file: None, pages: Vec::new() });
            self.elements.push(PageElement { id: group, kind: ElementKind::Printout(placeholder) });
        }
        self.printouts.iter_mut().find(|p| p.group == group)
    }

    /// The elements, printouts filled in.
    fn finish(mut self) -> (Vec<PageElement>, Vec<InkStroke>) {
        for mut p in std::mem::take(&mut self.printouts) {
            p.pages.sort_by_key(|(i, _)| *i);
            let pages: Vec<ImageElement> = p.pages.into_iter().map(|(_, img)| img).collect();
            let (id, file_name, file_bytes, pos) = match p.file {
                Some(f) => f,
                None => (p.group, String::new(), Arc::new(Vec::new()), pages.first().map_or(Point::ZERO, |i| i.pos)),
            };
            if let Some(slot) = self.elements.get_mut(p.slot) {
                *slot = PageElement { id, kind: ElementKind::Printout(Printout { file_name, file_bytes, pos, pages }) };
            }
        }
        (self.elements, self.ink)
    }
}

impl Reader {
    pub(crate) fn page(&mut self, space: &ObjectSpace) -> Option<Page> {
        let v = View::new(space);
        let manifest = v.root(role::CONTENT)?;
        let node = v.children(&manifest.props, prop::CONTENT_CHILD_NODES).into_iter().find(|o| o.jcid == jcid::PAGE_NODE)?;
        let meta = v.root(role::METADATA).map(|o| &o.props);
        let np = &node.props;
        let id = meta.and_then(|m| guid(m, prop::NOTEBOOK_MANAGEMENT_ENTITY_GUID)).unwrap_or(space.id.to_guid());
        let created = meta.and_then(|m| m.uint(prop::TOPOLOGY_CREATION_TIME_STAMP)).and_then(from_filetime).unwrap_or_else(notecraft_doc::now);
        let modified = np.u32(prop::LAST_MODIFIED_TIME).and_then(from_time32).unwrap_or(created);
        let level = meta.and_then(|m| m.u32(prop::PAGE_LEVEL)).map_or(0, |l| l.saturating_sub(1).min(u32::from(MAX_PAGE_LEVEL)) as u8);
        let origin = Point::new(
            hi_to_pt(np.f32(prop::PAGE_MARGIN_ORIGIN_X).or_else(|| np.f32(prop::PAGE_MARGIN_LEFT)).unwrap_or(0.0)),
            hi_to_pt(np.f32(prop::PAGE_MARGIN_ORIGIN_Y).or_else(|| np.f32(prop::PAGE_MARGIN_TOP)).unwrap_or(0.0)),
        );
        let title = v
            .children(np, prop::STRUCTURE_ELEMENT_CHILD_NODES)
            .into_iter()
            .find(|o| o.jcid == jcid::TITLE_NODE)
            .map(|t| self.title_text(&v, t))
            .filter(|t| !t.is_empty())
            .or_else(|| meta.and_then(|m| string(m, prop::CACHED_TITLE_STRING)))
            .unwrap_or_default();
        let mut parts = PageParts::default();
        for child in v.children(np, prop::ELEMENT_CHILD_NODES) {
            self.page_element(&v, child, origin, &mut parts);
        }
        let background = PageBackground {
            color: np.u32(prop::NC_PAGE_COLOR).map(|c| Color::rgba(c as u8, (c >> 8) as u8, (c >> 16) as u8, (c >> 24) as u8)),
            rule: string(np, prop::NC_RULE_LINES).and_then(|s| RuleLines::parse(&s)).unwrap_or_default(),
        };
        let (elements, ink) = parts.finish();
        let versions = crate::versions::read(self, space, manifest);
        Some(Page { id, title, created, modified, level, background, elements, ink, versions })
    }

    fn title_text(&mut self, v: &View<'_>, title: &Object) -> String {
        let mut texts = Vec::new();
        for outline in v.children(&title.props, prop::ELEMENT_CHILD_NODES) {
            if outline.props.bool(prop::IS_TITLE_DATE) == Some(true) {
                continue;
            }
            let mut paras = Vec::new();
            let mut extra = PageParts::default();
            self.outline_elements(v, &outline.props, (0, 0), Point::ZERO, &mut paras, &mut extra);
            texts.extend(paras.iter().map(Paragraph::text).filter(|t| !t.is_empty()));
            if !texts.is_empty() {
                break;
            }
        }
        texts.join(" ")
    }

    fn page_element(&mut self, v: &View<'_>, o: &Object, origin: Point, parts: &mut PageParts) {
        let pos = Point::new(
            origin.x + hi_to_pt(o.props.f32(prop::OFFSET_FROM_PARENT_HORIZ).unwrap_or(0.0)),
            origin.y + hi_to_pt(o.props.f32(prop::OFFSET_FROM_PARENT_VERT).unwrap_or(0.0)),
        )
        .sanitized();
        let id = element_id(o.id);
        match o.jcid {
            jcid::OUTLINE_NODE => {
                let width = o.props.f32(prop::LAYOUT_MAX_WIDTH).map(hi_to_pt).filter(|w| *w > 1.0).unwrap_or(DEFAULT_OUTLINE_WIDTH);
                let mut paragraphs = Vec::new();
                let base = o.props.u32(prop::OUTLINE_ELEMENT_CHILD_LEVEL).unwrap_or(1).saturating_sub(1);
                self.outline_elements(v, &o.props, (base, 0), pos, &mut paragraphs, parts);
                if paragraphs.is_empty() {
                    paragraphs.push(Paragraph::plain(""));
                }
                parts.push(PageElement { id, kind: ElementKind::Outline(Outline { pos, width, paragraphs }) });
            }
            jcid::IMAGE_NODE => self.image(v, o, id, pos, parts),
            jcid::EMBEDDED_FILE_NODE => self.embedded_file(v, o, id, pos, parts),
            jcid::INK_NODE => ink::read_ink_node(v.space, &v.index, &o.props, pos, &mut parts.ink),
            _ => self.opaque(v, o, id, parts),
        }
    }

    fn opaque(&mut self, v: &View<'_>, o: &Object, id: Guid, parts: &mut PageParts) {
        self.unsupported = true;
        if let Some(data) = opaque::capture(v.space, &v.index, o.id) {
            parts.push(PageElement { id, kind: ElementKind::Opaque { jcid: o.jcid, data: Arc::new(data) } });
        }
    }

    fn image(&mut self, v: &View<'_>, o: &Object, id: Guid, pos: Point, parts: &mut PageParts) {
        let Some(data) = v.children(&o.props, prop::PICTURE_CONTAINER).first().and_then(|f| f.file_data.clone()) else {
            self.unsupported = true;
            return;
        };
        let p = &o.props;
        let size = Size::new(
            p.f32(prop::LAYOUT_MAX_WIDTH).or_else(|| p.f32(prop::PICTURE_WIDTH)).map(hi_to_pt).unwrap_or(0.0),
            p.f32(prop::LAYOUT_MAX_HEIGHT).or_else(|| p.f32(prop::PICTURE_HEIGHT)).map(hi_to_pt).unwrap_or(0.0),
        );
        let size = if size.w > 0.0 && size.h > 0.0 { size } else { picture_size(&data).unwrap_or(Size::new(100.0, 100.0)) }.sanitized();
        let img = ImageElement { pos, size, format: ImageFormat::sniff(&data), data, alt: string(p, prop::IMAGE_ALT_TEXT).unwrap_or_default() };
        if let Some((group, index)) = printout_tag(p) {
            if let Some(p) = parts.printout(group) {
                p.pages.push((index, img));
            }
            return;
        }
        parts.push(PageElement { id, kind: ElementKind::Image(img) });
    }

    fn embedded_file(&mut self, v: &View<'_>, o: &Object, id: Guid, pos: Point, parts: &mut PageParts) {
        let p = &o.props;
        let bytes = v.children(p, prop::EMBEDDED_FILE_CONTAINER).first().and_then(|f| f.file_data.clone());
        let Some(bytes) = bytes else {
            self.unsupported = true;
            return;
        };
        let name = string(p, prop::EMBEDDED_FILE_NAME).or_else(|| string(p, prop::RICH_EDIT_TEXT_UNICODE)).unwrap_or_else(|| "Attachment".into());
        if let Some((group, _)) = printout_tag(p) {
            if let Some(p) = parts.printout(group) {
                p.file = Some((id, name, bytes, pos));
            }
            return;
        }
        parts.push(PageElement { id, kind: ElementKind::EmbeddedFile { name, bytes, pos } });
    }

    /// Walks the outline elements under `parent` (an outline, outline group or outline element).
    /// `at` is the (indent, nesting depth) of the elements found.
    fn outline_elements(
        &mut self,
        v: &View<'_>,
        parent: &PropertySet,
        at: (u32, usize),
        pos: Point,
        out: &mut Vec<Paragraph>,
        parts: &mut PageParts,
    ) {
        let (indent, depth) = at;
        if depth > MAX_OE_DEPTH {
            self.unsupported = true;
            return;
        }
        for child in v.children(parent, prop::ELEMENT_CHILD_NODES) {
            if out.len() >= MAX_PARAGRAPHS {
                self.unsupported = true;
                return;
            }
            let level = child.props.u32(prop::OUTLINE_ELEMENT_CHILD_LEVEL).unwrap_or(1).max(1);
            match child.jcid {
                jcid::OUTLINE_ELEMENT_NODE => {
                    self.outline_element(v, child, indent, pos, out, parts);
                    self.outline_elements(v, &child.props, (indent.saturating_add(level), depth + 1), pos, out, parts);
                }
                jcid::OUTLINE_GROUP => self.outline_elements(v, &child.props, (indent.saturating_add(level), depth + 1), pos, out, parts),
                _ => self.unsupported = true,
            }
        }
    }

    /// One outline element's content as paragraphs.
    fn outline_element(&mut self, v: &View<'_>, oe: &Object, indent: u32, pos: Point, out: &mut Vec<Paragraph>, parts: &mut PageParts) {
        let indent = indent.min(u32::from(MAX_INDENT)) as u8;
        let marker = v.children(&oe.props, prop::LIST_NODES).first().map(|l| list_marker(&l.props));
        let list = marker.map(|(k, _)| k);
        // A style that is the level's default stays implicit, so a NoteCraft paragraph reads back
        // as it was written.
        let list_style = marker.and_then(|(k, st)| st.filter(|st| *st != ListStyle::by_level(k, indent)));
        let content = v.children(&oe.props, prop::CONTENT_CHILD_NODES);
        let Some(c) = content.first() else {
            out.push(Paragraph { indent, list, list_style, ..Paragraph::plain("") });
            return;
        };
        let here = Point::new(pos.x, pos.y + out.len() as f32 * 18.0);
        match c.jcid {
            jcid::RICH_TEXT_OE_NODE => {
                let mut para = self.rich_text(v, c);
                para.indent = indent;
                para.list = list;
                para.list_style = list_style;
                out.push(para);
            }
            jcid::TABLE_NODE => {
                self.unsupported = true;
                self.table(v, c, indent, out);
            }
            jcid::IMAGE_NODE => self.image(v, c, element_id(c.id), here, parts),
            jcid::EMBEDDED_FILE_NODE => self.embedded_file(v, c, element_id(c.id), here, parts),
            _ => self.unsupported = true,
        }
    }

    /// A table, flattened: one paragraph per row, cells separated by tabs (read-only).
    fn table(&mut self, v: &View<'_>, table: &Object, indent: u8, out: &mut Vec<Paragraph>) {
        let mut cells = 0usize;
        for row in v.children(&table.props, prop::ELEMENT_CHILD_NODES) {
            let mut texts = Vec::new();
            for cell in v.children(&row.props, prop::ELEMENT_CHILD_NODES) {
                cells += 1;
                if cells > MAX_TABLE_CELLS || out.len() >= MAX_PARAGRAPHS {
                    return;
                }
                let mut paras = Vec::new();
                let mut extra = PageParts::default();
                self.outline_elements(v, &cell.props, (0, MAX_OE_DEPTH / 2), Point::ZERO, &mut paras, &mut extra);
                texts.push(paras.iter().map(Paragraph::text).collect::<Vec<_>>().join(" "));
            }
            out.push(Paragraph { indent, ..Paragraph::plain(&texts.join("\t")) });
        }
    }

    fn rich_text(&mut self, v: &View<'_>, rt: &Object) -> Paragraph {
        let p = &rt.props;
        let units: Vec<u16> = match p.bytes(prop::RICH_EDIT_TEXT_UNICODE) {
            Some(b) => wz(b).encode_utf16().collect(),
            None => p.bytes(prop::TEXT_EXTENDED_ASCII).map(extended_ascii).unwrap_or_default().encode_utf16().collect(),
        };
        let mut ends: Vec<usize> =
            p.bytes(prop::TEXT_RUN_INDEX).map(|b| b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c) as usize).collect()).unwrap_or_default();
        ends.retain(|&e| e <= units.len());
        ends.push(units.len());
        let formats = v.children(p, prop::TEXT_RUN_FORMATTING);
        let mut runs: Vec<Run> = Vec::with_capacity(ends.len());
        let mut start = 0usize;
        let mut link: Option<String> = None;
        for (i, &end) in ends.iter().enumerate() {
            let end = end.max(start);
            let text: String = char::decode_utf16(units.get(start..end).unwrap_or_default().iter().copied())
                .map(|c| match c {
                    Ok('\u{b}' | '\r') => '\n',
                    Ok(c) => c,
                    Err(_) => char::REPLACEMENT_CHARACTER,
                })
                .collect();
            start = end;
            let fmt = formats.get(i).or(formats.last()).map(|o| &o.props);
            let is_link = fmt.is_some_and(|f| f.bool(prop::HYPERLINK) == Some(true));
            if let Some(url) = text.strip_prefix('\u{FDDF}').and_then(hyperlink_target) {
                link = Some(url);
                continue;
            }
            if fmt.is_some_and(|f| f.bool(prop::HIDDEN) == Some(true)) {
                continue;
            }
            if !is_link {
                link = None;
            }
            let mut run = fmt.map(run_format).unwrap_or_default();
            run.text = text;
            run.link = if is_link { link.clone() } else { None };
            match runs.last_mut() {
                Some(last) if last.same_format(&run) => last.text.push_str(&run.text),
                _ => runs.push(run),
            }
        }
        if runs.is_empty() {
            runs.push(formats.first().map(|o| run_format(&o.props)).unwrap_or_default());
        }
        let style_obj = v.children(p, prop::PARAGRAPH_STYLE);
        let style = style_obj.first().and_then(|s| string(&s.props, prop::PARAGRAPH_STYLE_ID)).map_or(ParaStyle::Normal, |s| para_style(&s));
        let style = string(p, prop::NC_PARA_STYLE).and_then(|s| ParaStyle::parse(&s)).unwrap_or(style);
        // ParagraphAlignment on the paragraph, else on its style (§2.2.44).
        let aligned = p.uint(prop::PARAGRAPH_ALIGNMENT).or_else(|| style_obj.first().and_then(|s| s.props.uint(prop::PARAGRAPH_ALIGNMENT)));
        let align = match (p.uint(prop::NC_PARA_ALIGN), aligned) {
            (Some(3), _) => Align::Justify,
            (_, Some(1)) => Align::Center,
            (_, Some(2)) => Align::Right,
            _ => Align::Left,
        };
        let tag = self.note_tag(v, p);
        Paragraph { runs, indent: 0, list: None, list_style: None, tag, style, align }
    }

    fn note_tag(&mut self, v: &View<'_>, p: &PropertySet) -> Option<Tag> {
        let mut found = None;
        for state in p.sets(prop::NOTE_TAG_STATES) {
            let def = v.children(state, prop::NOTE_TAG_DEFINITION_OID).first().map(|o| &o.props);
            let shape = def.and_then(|d| d.uint(prop::NOTE_TAG_SHAPE)).or_else(|| state.uint(prop::NOTE_TAG_SHAPE)).unwrap_or(0);
            let raw_label = def.and_then(|d| string(d, prop::NOTE_TAG_LABEL)).unwrap_or_default();
            let label = raw_label.to_lowercase();
            let task = state.uint(prop::ACTION_ITEM_TYPE).is_some_and(|t| (100..=105).contains(&t));
            let done = state.uint(prop::ACTION_ITEM_STATUS).is_some_and(|s| s & 1 == 1);
            let own = def.and_then(|d| string(d, prop::NC_TAG)).and_then(|n| Tag::parse(&n));
            let highlight = def.and_then(|d| d.u32(prop::NOTE_TAG_HIGHLIGHT_COLOR)).and_then(from_colorref);
            let tag = match (own, shape) {
                (Some(Tag::ToDo { .. }), _) => Some(Tag::ToDo { checked: done }),
                (Some(t), _) => Some(t),
                _ if task => Some(Tag::ToDo { checked: done }),
                // NoteTagShape (§2.3.86): every check box is a To Do here.
                (_, 1..=12 | 28 | 30 | 32 | 48 | 50 | 52 | 69 | 71 | 73 | 89..=99) => Some(Tag::ToDo { checked: done }),
                (_, 13 | 34 | 40 | 54 | 61 | 75) => Some(Tag::Important),
                (_, 15 | 111) => Some(Tag::Question),
                (_, 17 | 115) => Some(Tag::Critical),
                (_, 18 | 114 | 118) => Some(Tag::Contact),
                (_, 21) => Some(Tag::Idea),
                (_, 23) => Some(Tag::Address),
                (_, 109 | 110) => Some(Tag::Phone),
                _ if label.contains("to do") => Some(Tag::ToDo { checked: done }),
                _ => Tag::parse(&raw_label).or_else(|| {
                    // A highlight tag in another language: by its colour, green for Definition.
                    let c = highlight.filter(|_| shape == 0)?;
                    Some(if c.g > c.r && c.g > c.b { Tag::Definition } else { Tag::Highlight })
                }),
            };
            match (found, tag) {
                (None, Some(t)) => found = Some(t),
                // NoteCraft keeps one tag per paragraph.
                _ => self.unsupported = true,
            }
        }
        found
    }
}

/// `HYPERLINK "url"` (the hidden field code of a hyperlink run) → the url.
fn hyperlink_target(field: &str) -> Option<String> {
    let rest = field.trim_start().strip_prefix("HYPERLINK")?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    rest.get(..end).map(str::to_owned)
}

/// A text run's formatting (jcidParagraphStyleObjectForText).
fn run_format(f: &PropertySet) -> Run {
    let alpha = f.uint(prop::NC_RUN_ALPHA).unwrap_or(0xFFFF);
    let color = |id: u32, a: u8| f.u32(id).and_then(from_colorref).map(|c| c.with_alpha(a));
    let size =
        f.f32(prop::NC_FONT_SIZE).or_else(|| f.uint(prop::FONT_SIZE).map(|hp| hp as f32 / 2.0)).filter(|s| s.is_finite() && *s > 0.0 && *s < 2000.0);
    Run {
        text: String::new(),
        bold: f.bool(prop::BOLD) == Some(true),
        italic: f.bool(prop::ITALIC) == Some(true),
        underline: f.bool(prop::UNDERLINE) == Some(true),
        strike: f.bool(prop::STRIKETHROUGH) == Some(true),
        highlight: color(prop::HIGHLIGHT, (alpha & 0xFF) as u8),
        color: color(prop::FONT_COLOR, (alpha >> 8) as u8),
        font: string(f, prop::FONT).filter(|s| !s.is_empty()),
        size,
        link: None,
    }
}

/// OneNote's paragraph style names → NoteCraft styles.
pub(crate) fn para_style(id: &str) -> ParaStyle {
    match id.to_ascii_lowercase().as_str() {
        "h1" => ParaStyle::Heading1,
        "h2" => ParaStyle::Heading2,
        "h3" => ParaStyle::Heading3,
        "h4" => ParaStyle::Heading4,
        "h5" => ParaStyle::Heading5,
        "h6" => ParaStyle::Heading6,
        "pagetitle" => ParaStyle::Title,
        "blockquote" | "quote" | "cite" => ParaStyle::Quote,
        "code" => ParaStyle::Code,
        _ => ParaStyle::Normal,
    }
}

/// NumberListFormat ([MS-ONE] §2.3.20): numbered when it holds U+FFFD, whose next character is
/// the MSONFC number format ([MS-OSHARED] §2.2.1.3); otherwise a bullet, whose character gives
/// its shape. The style is `None` for formats NoteCraft doesn't draw.
fn list_marker(p: &PropertySet) -> (ListKind, Option<ListStyle>) {
    let units: Vec<u16> = p.bytes(prop::NUMBER_LIST_FORMAT).map(|b| b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect()).unwrap_or_default();
    let chars = units.get(1..).unwrap_or_default();
    if let Some(i) = chars.iter().position(|u| *u == 0xFFFD) {
        let style = match chars.get(i + 1) {
            Some(0) => Some(ListStyle::Decimal),
            Some(1) => Some(ListStyle::UpperRoman),
            Some(2) => Some(ListStyle::LowerRoman),
            Some(3) => Some(ListStyle::UpperAlpha),
            Some(4) => Some(ListStyle::LowerAlpha),
            _ => None,
        };
        return (ListKind::Numbered, style);
    }
    let style = char::decode_utf16(chars.iter().copied()).filter_map(std::result::Result::ok).find(|c| !c.is_whitespace()).and_then(ListStyle::from_bullet_char);
    (ListKind::Bullet, style)
}

/// NoteCraft's printout tag on a picture or embedded file: the group and page index.
fn printout_tag(p: &PropertySet) -> Option<(Guid, u32)> {
    let b = p.bytes(prop::NC_PRINTOUT)?;
    let g: [u8; 16] = b.get(..16)?.try_into().ok()?;
    let index = b.get(16..20).and_then(|i| i.try_into().ok()).map_or(0, u32::from_le_bytes);
    Some((Guid(g), index))
}

/// A picture's size at 96 dpi from its PNG / GIF / BMP / JPEG header.
pub(crate) fn picture_size(d: &[u8]) -> Option<Size> {
    let be32 = |i: usize| d.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let le16 = |i: usize| d.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let (w, h) = match ImageFormat::sniff(d) {
        ImageFormat::Png => (be32(16)?, be32(20)?),
        ImageFormat::Gif => (u32::from(le16(6)?), u32::from(le16(8)?)),
        ImageFormat::Bmp => (
            d.get(18..22).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))?,
            d.get(22..26).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]).unsigned_abs())?,
        ),
        ImageFormat::Jpeg => jpeg_size(d)?,
        ImageFormat::Other => return None,
    };
    (w > 0 && h > 0 && w < 100_000 && h < 100_000).then(|| Size::new(w as f32 * 0.75, h as f32 * 0.75))
}

fn jpeg_size(d: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2usize;
    for _ in 0..10_000 {
        let marker = *d.get(i + 1)?;
        let len = usize::from(u16::from_be_bytes([*d.get(i + 2)?, *d.get(i + 3)?]));
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let h = u16::from_be_bytes([*d.get(i + 5)?, *d.get(i + 6)?]);
            let w = u16::from_be_bytes([*d.get(i + 7)?, *d.get(i + 8)?]);
            return Some((u32::from(w), u32::from(h)));
        }
        i = i.checked_add(2 + len)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyperlink_fields() {
        assert_eq!(hyperlink_target("HYPERLINK \"https://example.com/a?b=1\""), Some("https://example.com/a?b=1".into()));
        assert_eq!(hyperlink_target("HYPERLINK nope"), None);
        assert_eq!(hyperlink_target("PAGE"), None);
    }

    #[test]
    fn number_list_formats_of_other_writers() {
        let fmt = |units: &[u16]| {
            let mut p = PropertySet::default();
            p.push(prop::NUMBER_LIST_FORMAT, notecraft_onestore::PropertyValue::Bytes(units.iter().flat_map(|u| u.to_le_bytes()).collect()));
            list_marker(&p)
        };
        // A symbol font's private-use bullet, an "(a)" number, a format NoteCraft doesn't draw.
        assert_eq!(fmt(&[1, 0xF0B7]), (ListKind::Bullet, Some(ListStyle::Disc)));
        assert_eq!(fmt(&[4, u16::from(b'('), 0xFFFD, 4, u16::from(b')')]), (ListKind::Numbered, Some(ListStyle::LowerAlpha)));
        assert_eq!(fmt(&[3, 0xFFFD, 0x17, u16::from(b'.')]), (ListKind::Numbered, None));
        assert_eq!(fmt(&[1, u16::from(b'*')]), (ListKind::Bullet, None));
        // Truncated or empty data is a plain bullet, never a panic.
        assert_eq!(fmt(&[]), (ListKind::Bullet, None));
        assert_eq!(fmt(&[9, 0xFFFD]), (ListKind::Numbered, None));
        assert_eq!(fmt(&[2, 0xD800, 0xDC00]), (ListKind::Bullet, None));
    }

    #[test]
    fn styles_and_sizes() {
        assert_eq!(para_style("h3"), ParaStyle::Heading3);
        assert_eq!(para_style("PageTitle"), ParaStyle::Title);
        assert_eq!(para_style("whatever"), ParaStyle::Normal);
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&40u32.to_be_bytes());
        png.extend_from_slice(&20u32.to_be_bytes());
        assert_eq!(picture_size(&png), Some(Size::new(30.0, 15.0)));
        assert_eq!(picture_size(b"\xFF\xD8\xFF"), None);
        assert_eq!(picture_size(b""), None);
    }
}
