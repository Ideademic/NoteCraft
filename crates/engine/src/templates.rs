//! Page templates (Insert › Page Templates): NoteCraft's own, built here in code, and the
//! user's ("Save current page as a template"), which the front end stores and hands to
//! [`crate::Session::templates`].
//!
//! The built-in designs and their wording are NoteCraft's own.

use notecraft_doc::page::CONTENT_ORIGIN;
use notecraft_doc::{Color, ElementKind, Guid, ListKind, Outline, Page, PageElement, ParaStyle, Paragraph, Point, RuleLines, Run, Tag};
use serde::{Deserialize, Serialize};

/// Most user templates kept.
pub const MAX_USER_TEMPLATES: usize = 64;
/// Largest user template, bytes of JSON.
pub const MAX_TEMPLATE_BYTES: usize = 16 * 1024 * 1024;
/// The category user templates are listed under.
pub const USER_CATEGORY: &str = "My Templates";

/// A page template: a page to copy for each new page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    /// `nc:…` for built-in templates, `user:…` for the user's.
    pub id: String,
    pub name: String,
    pub category: String,
    pub page: Page,
}

impl Template {
    pub fn is_builtin(&self) -> bool {
        self.id.starts_with("nc:")
    }

    /// A new page from the template: fresh ids for the page, its items and ink, created now,
    /// no versions.
    pub fn instantiate(&self) -> Page {
        let mut p = self.page.clone();
        let fresh = Page::new(p.title.clone());
        p.id = fresh.id;
        p.created = fresh.created;
        p.modified = fresh.modified;
        p.level = 0;
        p.versions.clear();
        for el in &mut p.elements {
            el.id = Guid::new();
        }
        for st in &mut p.ink {
            st.id = Guid::new();
        }
        p
    }
}

fn para(text: &str) -> Paragraph {
    Paragraph::plain(text)
}

fn heading(text: &str, style: ParaStyle) -> Paragraph {
    Paragraph { style, ..para(text) }
}

fn bullet(text: &str) -> Paragraph {
    Paragraph { list: Some(ListKind::Bullet), ..para(text) }
}

fn numbered(text: &str) -> Paragraph {
    Paragraph { list: Some(ListKind::Numbered), ..para(text) }
}

fn todo(text: &str) -> Paragraph {
    Paragraph { tag: Some(Tag::ToDo { checked: false }), ..para(text) }
}

fn label(text: &str) -> Paragraph {
    Paragraph { runs: vec![Run { text: text.to_owned(), bold: true, ..Run::default() }], ..Paragraph::default() }
}

fn hint(text: &str) -> Paragraph {
    Paragraph {
        runs: vec![Run { text: text.to_owned(), italic: true, color: Some(Color::rgb(0x76, 0x76, 0x7E)), ..Run::default() }],
        ..Paragraph::default()
    }
}

fn outline(at: Point, width: f32, paragraphs: Vec<Paragraph>) -> PageElement {
    PageElement::new(ElementKind::Outline(Outline { pos: at, width, paragraphs }))
}

fn page(title: &str, color: Option<Color>, rule: RuleLines, elements: Vec<PageElement>) -> Page {
    let mut p = Page::new(title);
    p.background.color = color;
    p.background.rule = rule;
    p.elements = elements;
    p
}

fn t(id: &str, name: &str, category: &str, page: Page) -> Template {
    Template { id: id.to_owned(), name: name.to_owned(), category: category.to_owned(), page }
}

/// NoteCraft's built-in templates, in the order the pane lists them.
pub fn builtins() -> Vec<Template> {
    let at = CONTENT_ORIGIN;
    let col2 = Point::new(at.x + 330.0, at.y);
    let blue = Some(Color::rgb(0xDC, 0xE9, 0xF8));
    let green = Some(Color::rgb(0xE2, 0xF2, 0xDF));
    let yellow = Some(Color::rgb(0xFD, 0xF6, 0xD3));
    let lavender = Some(Color::rgb(0xEC, 0xE4, 0xF7));
    let rose = Some(Color::rgb(0xFA, 0xE3, 0xE6));
    vec![
        t("nc:blank", "Plain page", "Blank", page("", None, RuleLines::None, vec![])),
        t("nc:blank-blue-college", "Blue, college ruled", "Blank", page("", blue, RuleLines::College, vec![])),
        t("nc:blank-green-grid", "Green, small grid", "Blank", page("", green, RuleLines::GridSmall, vec![])),
        t("nc:blank-yellow-wide", "Yellow legal pad", "Blank", page("", yellow, RuleLines::Wide, vec![])),
        t("nc:blank-lavender-narrow", "Lavender, narrow ruled", "Blank", page("", lavender, RuleLines::Narrow, vec![])),
        t("nc:blank-rose", "Rose, no lines", "Blank", page("", rose, RuleLines::None, vec![])),
        t(
            "nc:meeting",
            "Simple meeting notes",
            "Business",
            page(
                "Meeting",
                None,
                RuleLines::None,
                vec![outline(
                    at,
                    460.0,
                    vec![
                        label("When and where"),
                        hint("Date, time, room or call link"),
                        label("Who came"),
                        bullet(""),
                        label("What we set out to decide"),
                        numbered(""),
                        heading("Notes", ParaStyle::Heading2),
                        para(""),
                        heading("Decisions", ParaStyle::Heading2),
                        bullet(""),
                        heading("Follow-ups", ParaStyle::Heading2),
                        todo("Who does what, by when"),
                    ],
                )],
            ),
        ),
        t(
            "nc:project",
            "Project overview",
            "Business",
            page(
                "Project",
                Some(Color::rgb(0xF4, 0xF6, 0xF8)),
                RuleLines::None,
                vec![
                    outline(
                        at,
                        310.0,
                        vec![
                            heading("Why we're doing this", ParaStyle::Heading2),
                            hint("One or two sentences on the goal"),
                            heading("What done looks like", ParaStyle::Heading2),
                            bullet(""),
                            heading("People", ParaStyle::Heading2),
                            bullet("Lead: "),
                            bullet("Helping: "),
                        ],
                    ),
                    outline(
                        col2,
                        280.0,
                        vec![
                            heading("Milestones", ParaStyle::Heading2),
                            todo("First draft"),
                            todo("Review"),
                            todo("Ship"),
                            heading("Risks and open questions", ParaStyle::Heading2),
                            Paragraph { tag: Some(Tag::Question), ..para("") },
                        ],
                    ),
                ],
            ),
        ),
        t(
            "nc:lecture",
            "Lecture notes",
            "Academic",
            page(
                "Lecture",
                None,
                RuleLines::College,
                vec![
                    outline(
                        at,
                        150.0,
                        vec![
                            label("Key terms"),
                            hint("Words to look up"),
                            para(""),
                            label("Questions"),
                            Paragraph { tag: Some(Tag::Question), ..para("") },
                        ],
                    ),
                    outline(
                        Point::new(at.x + 180.0, at.y),
                        420.0,
                        vec![
                            label("Course, topic, date"),
                            para(""),
                            heading("Notes", ParaStyle::Heading2),
                            bullet(""),
                            heading("In my own words", ParaStyle::Heading2),
                            hint("Sum up the lecture in three sentences"),
                        ],
                    ),
                ],
            ),
        ),
        t(
            "nc:todo",
            "To-do list",
            "Planners",
            page(
                "To do",
                Some(Color::rgb(0xFF, 0xFB, 0xEE)),
                RuleLines::None,
                vec![outline(
                    at,
                    360.0,
                    vec![
                        heading("Today", ParaStyle::Heading2),
                        todo(""),
                        todo(""),
                        todo(""),
                        heading("This week", ParaStyle::Heading2),
                        todo(""),
                        todo(""),
                        heading("Someday", ParaStyle::Heading2),
                        todo(""),
                    ],
                )],
            ),
        ),
        t(
            "nc:weekly",
            "Weekly planner",
            "Planners",
            page(
                "Week of",
                None,
                RuleLines::None,
                ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Weekend"]
                    .iter()
                    .enumerate()
                    .map(|(i, day)| {
                        let x = at.x + (i % 3) as f32 * 220.0;
                        let y = at.y + (i / 3) as f32 * 190.0;
                        outline(Point::new(x, y), 200.0, vec![heading(day, ParaStyle::Heading3), todo(""), todo(""), bullet("")])
                    })
                    .chain(std::iter::once(outline(
                        Point::new(at.x, at.y + 390.0),
                        640.0,
                        vec![heading("Goals for the week", ParaStyle::Heading3), numbered(""), numbered("")],
                    )))
                    .collect(),
            ),
        ),
        t(
            "nc:journal",
            "Journal",
            "Personal",
            page(
                "Journal",
                Some(Color::rgb(0xFB, 0xF4, 0xEA)),
                RuleLines::Standard,
                vec![outline(
                    at,
                    480.0,
                    vec![
                        label("Today I'm grateful for"),
                        bullet(""),
                        label("What happened"),
                        para(""),
                        label("On my mind"),
                        para(""),
                        label("Tomorrow I want to"),
                        todo(""),
                    ],
                )],
            ),
        ),
    ]
}

/// A built-in template by id.
pub fn builtin(id: &str) -> Option<Template> {
    builtins().into_iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_unique_and_instantiate_fresh() {
        let all = builtins();
        let mut ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        for want in ["nc:blank", "nc:meeting", "nc:todo", "nc:lecture", "nc:project", "nc:weekly", "nc:journal", "nc:blank-blue-college"] {
            assert!(builtin(want).is_some(), "{want}");
        }
        let t = builtin("nc:weekly").unwrap();
        let a = t.instantiate();
        let b = t.instantiate();
        assert_ne!(a.id, b.id);
        assert!(a.elements.iter().zip(&b.elements).all(|(x, y)| x.id != y.id));
        assert!(all.iter().all(Template::is_builtin));
    }
}
