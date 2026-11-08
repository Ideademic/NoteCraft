//! Links between notes: `notecraft:` URLs (Copy Link to Page, Copy Link to Paragraph, `[[Page]]`
//! wiki links), and which other link schemes NoteCraft follows.
//!
//! The form is NoteCraft's own:
//!
//! ```text
//! notecraft:page?id=<page>&section=<section>&notebook=<notebook>&title=<title>
//! notecraft:page?id=<page>&…&outline=<outline>&para=<n>      (a paragraph)
//! ```
//!
//! GUIDs are bare (no braces), the title is percent-encoded UTF-8 and only a fallback for finding
//! the page when its id is gone (a page copied into another notebook). Every part but `id` is
//! optional; unknown parts are ignored.

use crate::Guid;

/// The scheme of NoteCraft's own links, with its colon.
pub const SCHEME: &str = "notecraft:";
/// Longest link NoteCraft keeps or follows (hostile files and commands).
pub const MAX_URL: usize = 2048;

/// Where a `notecraft:` link points.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InternalLink {
    pub page: Option<Guid>,
    pub section: Option<Guid>,
    pub notebook: Option<Guid>,
    /// A paragraph on the page: its note container and index.
    pub outline: Option<Guid>,
    pub para: Option<usize>,
    /// The page's title when the link was made.
    pub title: Option<String>,
}

fn bare(g: Guid) -> String {
    let s = g.to_string();
    s.trim_start_matches('{').trim_end_matches('}').to_owned()
}

/// Percent-encodes everything but unreserved characters (RFC 3986).
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Decodes `%XX` escapes (and `+` as a space); malformed escapes stay as they are, invalid UTF-8
/// becomes U+FFFD.
pub fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        let hex = |k: usize| bytes.get(k).and_then(|c| char::from(*c).to_digit(16));
        match b {
            b'%' => match (hex(i + 1), hex(i + 2)) {
                (Some(h), Some(l)) => {
                    out.push(u8::try_from(h * 16 + l).unwrap_or(b'?'));
                    i += 3;
                    continue;
                }
                _ => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl InternalLink {
    /// A link to a page.
    pub fn to_page(notebook: Guid, section: Guid, page: Guid, title: &str) -> InternalLink {
        InternalLink {
            page: Some(page),
            section: Some(section),
            notebook: Some(notebook),
            title: (!title.trim().is_empty()).then(|| title.trim().chars().take(200).collect()),
            ..InternalLink::default()
        }
    }

    /// The link as a `notecraft:` URL.
    pub fn to_url(&self) -> String {
        let mut parts = Vec::new();
        if let Some(p) = self.page {
            parts.push(format!("id={}", bare(p)));
        }
        if let Some(s) = self.section {
            parts.push(format!("section={}", bare(s)));
        }
        if let Some(n) = self.notebook {
            parts.push(format!("notebook={}", bare(n)));
        }
        if let Some(o) = self.outline {
            parts.push(format!("outline={}", bare(o)));
        }
        if let Some(i) = self.para {
            parts.push(format!("para={i}"));
        }
        if let Some(t) = &self.title {
            parts.push(format!("title={}", encode(t)));
        }
        format!("{SCHEME}page?{}", parts.join("&"))
    }

    /// Reads a `notecraft:` URL; `None` for anything else, or one naming no page, section or
    /// title.
    pub fn parse(url: &str) -> Option<InternalLink> {
        let url = url.trim();
        if url.len() > MAX_URL {
            return None;
        }
        let rest = url.get(..SCHEME.len()).filter(|s| s.eq_ignore_ascii_case(SCHEME)).and_then(|_| url.get(SCHEME.len()..))?;
        let rest = rest.trim_start_matches('/');
        let query = rest.split_once('?').map_or(rest, |(_, q)| q);
        let query = query.split('#').next().unwrap_or("");
        let mut l = InternalLink::default();
        for pair in query.split('&') {
            let Some((k, v)) = pair.split_once('=') else { continue };
            match k {
                "id" | "page" => l.page = Guid::parse(v),
                "section" => l.section = Guid::parse(v),
                "notebook" => l.notebook = Guid::parse(v),
                "outline" => l.outline = Guid::parse(v),
                "para" => l.para = v.parse::<usize>().ok(),
                "title" => {
                    let t: String = decode(v).chars().filter(|c| !c.is_control()).take(200).collect();
                    l.title = (!t.trim().is_empty()).then_some(t);
                }
                _ => {}
            }
        }
        (l.page.is_some() || l.section.is_some() || l.title.is_some()).then_some(l)
    }
}

/// Whether `url` is a `notecraft:` link.
pub fn is_internal(url: &str) -> bool {
    url.trim().get(..SCHEME.len()).is_some_and(|s| s.eq_ignore_ascii_case(SCHEME))
}

/// The scheme of `url`, lower case (`https`, `mailto`, `notecraft`); `None` without one.
pub fn scheme(url: &str) -> Option<String> {
    let (s, _) = url.trim().split_once(':')?;
    let ok = !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    ok.then(|| s.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let l = InternalLink {
            outline: Some(Guid::new()),
            para: Some(3),
            ..InternalLink::to_page(Guid::new(), Guid::new(), Guid::new(), "Plans & ideas ✓")
        };
        let url = l.to_url();
        assert!(url.starts_with("notecraft:page?id="));
        assert!(!url.contains(' ') && !url.contains('{'));
        assert_eq!(InternalLink::parse(&url), Some(l));
        assert!(is_internal(" NOTECRAFT:page?id=x&title=a"));
        assert_eq!(InternalLink::parse("notecraft:page?title=a%20b").and_then(|l| l.title), Some("a b".into()));
    }

    #[test]
    fn junk_is_none_or_partial() {
        for junk in ["", "notecraft:", "notecraft:page?", "https://x", "notecraft:page?id=zz&para=-1", &"notecraft:page?title=".repeat(500)] {
            assert_eq!(InternalLink::parse(junk), None, "{junk}");
        }
        assert_eq!(decode("%E2%9C%93%zz%"), "\u{2713}%zz%");
        assert_eq!(decode("%FF"), "\u{FFFD}");
        assert_eq!(scheme("HTTPS://x"), Some("https".into()));
        assert_eq!(scheme("c:\\x"), Some("c".into()));
        assert_eq!(scheme("no scheme"), None);
    }
}
