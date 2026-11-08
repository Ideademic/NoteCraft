//! JCIDs and PropertyIDs.
//!
//! Most come from [MS-ONE] §2.1.12 / §2.1.13. Some MS-ONE uses but doesn't number, and the ink
//! objects (which MS-ONE doesn't document), were identified by black-box inspection of sample
//! files (the Apache-2.0 Apache Tika test documents); those are marked *observed*. NoteCraft's own
//! properties are marked *private*: they sit in an id range MS-ONE doesn't use, and [MS-ONE] §2.1.12
//! has readers ignore and preserve properties they don't know.

/// JCIDs ([MS-ONE] §2.1.13 / §2.1.5).
pub mod jcid {
    pub const TOC_CONTAINER: u32 = 0x0002_0001;
    pub const AUTHOR_CONTAINER: u32 = 0x0012_0001;
    pub const SECTION_NODE: u32 = 0x0006_0007;
    pub const PAGE_SERIES_NODE: u32 = 0x0006_0008;
    pub const PAGE_NODE: u32 = 0x0006_000B;
    pub const OUTLINE_NODE: u32 = 0x0006_000C;
    pub const OUTLINE_ELEMENT_NODE: u32 = 0x0006_000D;
    pub const RICH_TEXT_OE_NODE: u32 = 0x0006_000E;
    pub const IMAGE_NODE: u32 = 0x0006_0011;
    pub const NUMBER_LIST_NODE: u32 = 0x0006_0012;
    pub const OUTLINE_GROUP: u32 = 0x0006_0019;
    pub const TABLE_NODE: u32 = 0x0006_0022;
    pub const TABLE_ROW_NODE: u32 = 0x0006_0023;
    pub const TABLE_CELL_NODE: u32 = 0x0006_0024;
    pub const TITLE_NODE: u32 = 0x0006_002C;
    pub const PAGE_METADATA: u32 = 0x0002_0030;
    pub const SECTION_METADATA: u32 = 0x0002_0031;
    pub const EMBEDDED_FILE_NODE: u32 = 0x0006_0035;
    pub const EMBEDDED_FILE_CONTAINER: u32 = 0x0008_0036;
    pub const PAGE_MANIFEST_NODE: u32 = 0x0006_0037;
    pub const PICTURE_CONTAINER: u32 = 0x0008_0039;
    pub const NOTE_TAG_DEFINITION: u32 = 0x0012_0043;
    pub const REVISION_METADATA: u32 = 0x0002_0044;
    pub const PARAGRAPH_STYLE: u32 = 0x0012_004D;
    /// The content root of a page's version history object space ([MS-ONE] §2.2.37).
    pub const VERSION_HISTORY_CONTENT: u32 = 0x0006_003C;
    /// One version history page ([MS-ONE] §2.2.38).
    pub const VERSION_PROXY: u32 = 0x0006_003D;
    /// The metadata root of a version history object space ([MS-ONE] §2.2.40).
    pub const VERSION_HISTORY_METADATA: u32 = 0x0002_0046;
    /// *Observed*: an ink drawing placed on the page (in `ElementChildNodesOfPage`).
    pub const INK_NODE: u32 = 0x0006_0014;
    /// *Observed*: the strokes of an ink drawing.
    pub const INK_DATA: u32 = 0x0002_003B;
    /// *Observed*: one ink stroke.
    pub const INK_STROKE: u32 = 0x0002_0047;
    /// *Observed*: drawing attributes shared by strokes.
    pub const INK_STROKE_PROPERTIES: u32 = 0x0012_0048;
}

/// PropertyIDs (with their type bits, as [MS-ONE] §2.1.12 lists them).
pub mod prop {
    pub const LAYOUT_TIGHT_LAYOUT: u32 = 0x0800_1C00;
    pub const PAGE_WIDTH: u32 = 0x1400_1C01;
    pub const PAGE_HEIGHT: u32 = 0x1400_1C02;
    pub const OUTLINE_ELEMENT_CHILD_LEVEL: u32 = 0x0C00_1C03;
    pub const BOLD: u32 = 0x0800_1C04;
    pub const ITALIC: u32 = 0x0800_1C05;
    pub const UNDERLINE: u32 = 0x0800_1C06;
    pub const STRIKETHROUGH: u32 = 0x0800_1C07;
    pub const FONT: u32 = 0x1C00_1C0A;
    pub const FONT_SIZE: u32 = 0x1000_1C0B;
    pub const FONT_COLOR: u32 = 0x1400_1C0C;
    pub const HIGHLIGHT: u32 = 0x1400_1C0D;
    pub const RG_OUTLINE_INDENT_DISTANCE: u32 = 0x1C00_1C12;
    pub const BODY_TEXT_ALIGNMENT: u32 = 0x0C00_1C13;
    pub const OFFSET_FROM_PARENT_HORIZ: u32 = 0x1400_1C14;
    pub const OFFSET_FROM_PARENT_VERT: u32 = 0x1400_1C15;
    pub const NUMBER_LIST_FORMAT: u32 = 0x1C00_1C1A;
    pub const LAYOUT_MAX_WIDTH: u32 = 0x1400_1C1B;
    pub const LAYOUT_MAX_HEIGHT: u32 = 0x1400_1C1C;
    /// ContentChildNodes of an outline element / page manifest.
    pub const CONTENT_CHILD_NODES: u32 = 0x2400_1C1F;
    /// ElementChildNodes of sections, pages, titles, outlines, outline elements, tables…
    pub const ELEMENT_CHILD_NODES: u32 = 0x2400_1C20;
    pub const RICH_EDIT_TEXT_UNICODE: u32 = 0x1C00_1C22;
    pub const LIST_NODES: u32 = 0x2400_1C26;
    pub const NOTEBOOK_MANAGEMENT_ENTITY_GUID: u32 = 0x1C00_1C30;
    pub const LANGUAGE_ID: u32 = 0x1400_1C3B;
    pub const LAYOUT_ALIGNMENT_IN_PARENT: u32 = 0x1400_1C3E;
    pub const PICTURE_CONTAINER: u32 = 0x2000_1C3F;
    pub const PAGE_MARGIN_TOP: u32 = 0x1400_1C4C;
    pub const PAGE_MARGIN_BOTTOM: u32 = 0x1400_1C4D;
    pub const PAGE_MARGIN_LEFT: u32 = 0x1400_1C4E;
    pub const PAGE_MARGIN_RIGHT: u32 = 0x1400_1C4F;
    pub const LIST_FONT: u32 = 0x1C00_1C52;
    pub const TOPOLOGY_CREATION_TIME_STAMP: u32 = 0x1800_1C65;
    pub const LAYOUT_ALIGNMENT_SELF: u32 = 0x1400_1C84;
    pub const IS_TITLE_TIME: u32 = 0x0800_1C87;
    pub const IS_BOILER_TEXT: u32 = 0x0800_1C88;
    pub const PAGE_SIZE: u32 = 0x1400_1C8B;
    pub const ENFORCE_OUTLINE_STRUCTURE: u32 = 0x0800_1C91;
    pub const CANNOT_BE_SELECTED: u32 = 0x0800_1CB2;
    pub const IS_TITLE_TEXT: u32 = 0x0800_1CB4;
    pub const IS_TITLE_DATE: u32 = 0x0800_1CB5;
    pub const IS_LAYOUT_SIZE_SET_BY_USER: u32 = 0x0800_1CBD;
    /// *Observed* (MS-ONE names NotebookColor without numbering it): a COLORREF-style
    /// `0x00BBGGRR`, `0xFFFFFFFF` for none.
    pub const NOTEBOOK_COLOR: u32 = 0x1400_1CBE;
    pub const LAYOUT_RESOLVE_CHILD_COLLISIONS: u32 = 0x0800_1CDC;
    pub const IS_READ_ONLY: u32 = 0x0800_1CDE;
    pub const LAYOUT_MINIMUM_OUTLINE_WIDTH: u32 = 0x1400_1CEC;
    pub const LAYOUT_COLLISION_PRIORITY: u32 = 0x1400_1CF1;
    pub const CACHED_TITLE_STRING: u32 = 0x1C00_1CF3;
    pub const DESCENDANTS_CANNOT_BE_MOVED: u32 = 0x0800_1CF9;
    pub const RICH_EDIT_TEXT_LANG_ID: u32 = 0x1000_1CFE;
    pub const LAYOUT_TIGHT_ALIGNMENT: u32 = 0x0800_1CFF;
    pub const CREATION_TIME_STAMP: u32 = 0x1400_1D09;
    pub const DELETABLE: u32 = 0x0800_1D0C;
    pub const LIST_MSAA_INDEX: u32 = 0x1000_1D0E;
    pub const PAGE_MARGIN_ORIGIN_X: u32 = 0x1400_1D0F;
    pub const PAGE_MARGIN_ORIGIN_Y: u32 = 0x1400_1D10;
    pub const IS_BACKGROUND: u32 = 0x0800_1D13;
    pub const CACHED_TITLE_STRING_FROM_PAGE: u32 = 0x1C00_1D3C;
    /// jcidTableNode ([MS-ONE] §2.2.27): rows, columns, borders, column widths (a count byte,
    /// then a float per column in half-inches) and locked columns (a count byte, then one bit
    /// per column).
    pub const ROW_COUNT: u32 = 0x1400_1D57;
    pub const COLUMN_COUNT: u32 = 0x1400_1D58;
    pub const TABLE_BORDERS_VISIBLE: u32 = 0x0800_1D5E;
    pub const TABLE_COLUMN_WIDTHS: u32 = 0x1C00_1D66;
    pub const TABLE_COLUMNS_LOCKED: u32 = 0x1C00_1D7D;
    /// jcidTableCellNode: the cell's background, a COLORREF.
    pub const CELL_SHADING_COLOR: u32 = 0x1400_1E26;
    pub const STRUCTURE_ELEMENT_CHILD_NODES: u32 = 0x2400_1D5F;
    pub const CHILD_GRAPH_SPACE_ELEMENT_NODES: u32 = 0x2C00_1D63;
    pub const META_DATA_OBJECTS_ABOVE_GRAPH_SPACE: u32 = 0x2400_3442;
    pub const LAST_MODIFIED_TIME_STAMP: u32 = 0x1800_1D77;
    pub const LAST_MODIFIED_TIME: u32 = 0x1400_1D7A;
    pub const SCHEMA_REVISION_IN_ORDER_TO_READ: u32 = 0x1400_1D82;
    /// [MS-ONESTORE] §2.7.2.
    pub const FILE_IDENTITY_GUID: u32 = 0x1C00_1D94;
    pub const EMBEDDED_FILE_CONTAINER: u32 = 0x2000_1D9B;
    pub const EMBEDDED_FILE_NAME: u32 = 0x1C00_1D9C;
    pub const SOURCE_FILEPATH: u32 = 0x1C00_1D9D;
    pub const IMAGE_FILENAME: u32 = 0x1C00_1DD7;
    pub const PAGE_LEVEL: u32 = 0x1400_1DFF;
    pub const TEXT_RUN_INDEX: u32 = 0x1C00_1E12;
    pub const TEXT_RUN_FORMATTING: u32 = 0x2400_1E13;
    pub const HYPERLINK: u32 = 0x0800_1E14;
    pub const UNDERLINE_TYPE: u32 = 0x0C00_1E15;
    pub const HIDDEN: u32 = 0x0800_1E16;
    pub const HYPERLINK_PROTECTED: u32 = 0x0800_1E19;
    pub const WZ_HYPERLINK_URL: u32 = 0x1C00_1E20;
    pub const IMAGE_ALT_TEXT: u32 = 0x1C00_1E58;
    pub const PARAGRAPH_STYLE: u32 = 0x2000_342C;
    pub const PARAGRAPH_STYLE_ID: u32 = 0x1C00_345A;
    pub const ACTION_ITEM_TYPE: u32 = 0x1000_3463;
    pub const NOTE_TAG_SHAPE: u32 = 0x1000_3464;
    /// [MS-ONE] NoteTagHighlightColor (on the tag definition): a COLORREF painted behind the
    /// tagged text.
    pub const NOTE_TAG_HIGHLIGHT_COLOR: u32 = 0x1400_3465;
    pub const NOTE_TAG_PROPERTY_STATUS: u32 = 0x1400_3467;
    pub const NOTE_TAG_LABEL: u32 = 0x1C00_3468;
    pub const NOTE_TAG_CREATED: u32 = 0x1400_346E;
    pub const NOTE_TAG_COMPLETED: u32 = 0x1400_346F;
    pub const ACTION_ITEM_STATUS: u32 = 0x1000_3470;
    pub const ACTION_ITEM_SCHEMA_VERSION: u32 = 0x0C00_3473;
    /// [MS-ONE] §2.3.94 ParagraphAlignment (on jcidRichTextOENode and jcidParagraphStyleObject):
    /// 0 left, 1 center, 2 right. (BodyTextAlignment, §2.3.17, is always 0 and ignored.)
    pub const PARAGRAPH_ALIGNMENT: u32 = 0x0C00_3477;
    pub const NOTE_TAG_DEFINITION_OID: u32 = 0x2000_3488;
    /// MS-ONE's table prints 0x04003489; MS-ONE §2.2.88 and the sample files make it an array
    /// of property values (type 0x10).
    pub const NOTE_TAG_STATES: u32 = 0x4000_3489;
    /// *Observed* (MS-ONE names SchemaRevisionInOrderToWrite without numbering it).
    pub const SCHEMA_REVISION_IN_ORDER_TO_WRITE: u32 = 0x1400_348B;
    pub const TEXT_EXTENDED_ASCII: u32 = 0x1C00_3498;
    pub const SECTION_DISPLAY_NAME: u32 = 0x1C00_349B;
    pub const PICTURE_WIDTH: u32 = 0x1400_34CD;
    pub const PICTURE_HEIGHT: u32 = 0x1400_34CE;

    // Ink, *observed* in sample files.
    /// Ink node → its ink data object.
    pub const INK_DATA: u32 = 0x2000_3415;
    /// Ink data → its strokes.
    pub const INK_STROKES: u32 = 0x2400_3416;
    /// Stroke → its drawing attributes.
    pub const INK_STROKE_PROPERTIES: u32 = 0x2000_3409;
    /// Stroke path: a multi-byte encoded value count, then that many signed multi-byte encoded
    /// deltas ([ISF] encoding), one run per packet property (all X, then all Y, …).
    pub const INK_PATH: u32 = 0x1C00_340B;
    /// Stroke identity (a GUID).
    pub const INK_STROKE_GUID: u32 = 0x1C00_341A;
    /// Drawing attributes: the packet properties, 32 bytes each (GUID, min, max, unit,
    /// resolution).
    pub const INK_PACKET_DESCRIPTION: u32 = 0x1C00_340A;
    /// Drawing attributes: pen colour, COLORREF.
    pub const INK_COLOR: u32 = 0x1400_340F;
    /// Drawing attributes: pen tip width and height, float, HIMETRIC.
    pub const INK_WIDTH: u32 = 0x1400_340C;
    pub const INK_HEIGHT: u32 = 0x1400_340D;
    /// Drawing attributes: a flag every sample sets to true (meaning unknown).
    pub const INK_FLAG: u32 = 0x0800_3411;

    // NoteCraft's private properties (type bits as usual; ids 0x3FC_0000 and up).
    /// Section metadata: written by NoteCraft (the producer's name and version, UTF-16).
    pub const NC_WRITER: u32 = 0x1FFC_0001;
    /// Page node: background colour, `0xAABBGGRR`.
    pub const NC_PAGE_COLOR: u32 = 0x17FC_0002;
    /// Page node: rule lines ([`notecraft_doc::RuleLines`] name, UTF-16).
    pub const NC_RULE_LINES: u32 = 0x1FFC_0003;
    /// Stroke: the exact NoteCraft stroke (see `ink.rs`).
    pub const NC_STROKE: u32 = 0x1FFC_0004;
    /// Embedded file / picture: the printout it belongs to (16-byte GUID) and, for pictures,
    /// the page index (4 bytes).
    pub const NC_PRINTOUT: u32 = 0x1FFC_0005;
    /// Outline / picture / embedded file: the model element id when it isn't the object's GUID.
    pub const NC_ELEMENT_ID: u32 = 0x1FFC_0006;
    /// Opaque content: an element NoteCraft kept but doesn't edit, re-emitted as found.
    pub const NC_OPAQUE: u32 = 0x1FFC_0007;
    /// Colour alpha of a text run's colour or highlight (`0xCCHH`, a byte each).
    pub const NC_RUN_ALPHA: u32 = 0x13FC_0008;
    /// Run: font size in points when it isn't a whole number of half-points (f32).
    pub const NC_FONT_SIZE: u32 = 0x17FC_0009;
    /// Paragraph: the NoteCraft paragraph style name (UTF-16).
    pub const NC_PARA_STYLE: u32 = 0x1FFC_000A;
    /// Paragraph: 3 for justified (ParagraphAlignment has no such value; it says left then).
    pub const NC_PARA_ALIGN: u32 = 0x0FFC_000B;
    /// Note tag definition: the NoteCraft tag name ([`notecraft_doc::Tag::name`], UTF-16), so a
    /// tag reads back exactly even where its shape and label are shared or translated.
    pub const NC_TAG: u32 = 0x1FFC_000C;
    /// Paragraph: the proofing language NoteCraft keeps (a BCP 47 tag, UTF-16), for tags without
    /// an LCID (`zxx`) and so a language chosen on purpose reads back as chosen.
    pub const NC_PARA_LANG: u32 = 0x1FFC_000D;

    // .onetoc2 (MS-ONE names these without numbering them; NoteCraft-chosen, see README).
    /// TOC: the entries (sections and section groups) in order.
    pub const NC_TOC_ENTRIES: u32 = 0x27FC_0010;
    /// TOC entry: the file or folder name (UTF-16).
    pub const NC_TOC_FILE_NAME: u32 = 0x1FFC_0011;
    /// TOC entry: 1 for a section group folder.
    pub const NC_TOC_IS_GROUP: u32 = 0x0FFC_0012;
    /// TOC: the notebook name (UTF-16).
    pub const NC_TOC_NAME: u32 = 0x1FFC_0013;
    /// TOC entry: position in the notebook.
    pub const NC_TOC_ORDER: u32 = 0x17FC_0014;

    // Page versions ([MS-ONE] §2.1.17, §2.1.18): see `versions.rs`.
    /// Page manifest: the version history object space's context (an array of one ContextID).
    pub const VERSION_HISTORY_GRAPH_SPACE_CONTEXT_NODES: u32 = 0x3400_347B;
    /// TOC: whether version history pages can be kept for the notebook (default true).
    pub const ENABLE_HISTORY: u32 = 0x0800_1E1E;
    /// Version history content: NoteCraft's version proxies, in order. MS-ONE links a proxy to its
    /// version through VersionContextNodes, which it names without numbering, so NoteCraft
    /// leaves the documented ElementChildNodes empty (as OneNote's own files have it when there
    /// are no versions) and lists its proxies here.
    pub const NC_VERSION_PROXIES: u32 = 0x27FC_0020;
    /// Version proxy: the version's context (an array of one ContextID), standing in for
    /// VersionContextNodes.
    pub const NC_VERSION_CONTEXT: u32 = 0x37FC_0021;

    // The recycle bin (`OneNote_RecycleBin/`): see `recycle.rs`.
    /// Section metadata of a deleted section, page metadata of a deleted page: when it was
    /// deleted (FILETIME).
    pub const NC_DELETED_TIME: u32 = 0x1BFC_0022;
    /// Deleted section: the section groups it was in (16-byte GUIDs, root first).
    pub const NC_DELETED_GROUP_PATH: u32 = 0x1FFC_0023;
    /// Deleted section or page: its position in its group or section.
    pub const NC_DELETED_INDEX: u32 = 0x17FC_0024;
    /// Deleted page: the section it was in (16-byte GUID).
    pub const NC_DELETED_SECTION: u32 = 0x1FFC_0025;
    /// Deleted page: that section's name (UTF-16).
    pub const NC_DELETED_SECTION_NAME: u32 = 0x1FFC_0026;
}

/// The context of a page's version history object space ([MS-ONE] §2.1.18):
/// {7111497f-1b6b-4209-9491-c98b04cf4c5a}, 1, on-disk byte order.
pub const VERSION_HISTORY_CONTEXT: [u8; 16] = [0x7F, 0x49, 0x11, 0x71, 0x6B, 0x1B, 0x09, 0x42, 0x94, 0x91, 0xC9, 0x8B, 0x04, 0xCF, 0x4C, 0x5A];

/// Root roles ([MS-ONE] §2.1.8).
pub mod role {
    pub const CONTENT: u32 = 1;
    pub const METADATA: u32 = 2;
    pub const VERSION_METADATA: u32 = 4;
}

/// The metadata salt GUID ([MS-ONE] §2.2.81), on-disk byte order.
pub const METADATA_SALT: [u8; 16] = [0x31, 0xC0, 0xA8, 0x22, 0x00, 0x36, 0xEE, 0x42, 0xB7, 0x14, 0xD7, 0xAC, 0xDA, 0x24, 0x35, 0xE8];

/// SchemaRevisionInOrderToRead / ToWrite ([MS-ONE] §2.3.69, §2.3.97).
pub const SCHEMA_REVISION: u32 = 0x28;
/// LCID en-US, for LanguageID / RichEditTextLangID.
pub const LCID_EN_US: u32 = 0x0409;

#[cfg(test)]
mod tests {
    use notecraft_onestore::PropertyId;

    #[test]
    fn private_ids_have_the_types_they_claim() {
        use super::prop;
        let t = |raw| PropertyId::new(raw).ty();
        assert_eq!(t(prop::NC_WRITER), 0x7);
        assert_eq!(t(prop::NC_PAGE_COLOR), 0x5);
        assert_eq!(t(prop::NC_RUN_ALPHA), 0x4);
        assert_eq!(t(prop::NC_TOC_ENTRIES), 0x9);
        assert_eq!(t(prop::NC_TOC_IS_GROUP), 0x3);
        assert_eq!(t(prop::NOTE_TAG_STATES), 0x10);
        assert_eq!(t(prop::CHILD_GRAPH_SPACE_ELEMENT_NODES), 0xB);
        for raw in [prop::NC_WRITER, prop::NC_STROKE, prop::NC_TOC_ENTRIES, prop::NC_TOC_ORDER] {
            assert!(PropertyId::new(raw).id() >= 0x3FC_0000);
        }
    }
}
