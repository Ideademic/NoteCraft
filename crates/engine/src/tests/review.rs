//! Review › Spelling, Language and Word Count, links (Ctrl+K, `notecraft:` links, `[[wiki]]`
//! links) and saving an attachment, through commands.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use notecraft_doc::link::InternalLink;
use notecraft_doc::{ElementKind, Guid};
use serde_json::{Value, json};

use crate::{CmdError, Session, ids};

fn gid(v: &Value, k: &str) -> Guid {
    Guid::parse(v[k].as_str().unwrap()).unwrap()
}

fn session_with(text: &str) -> (Session, Guid) {
    let mut s = Session::new_default();
    s.run(ids::PAGE_NEW, &json!({"title": "Review"})).unwrap();
    let o = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 108, "text": text})).unwrap().value, "id");
    (s, o)
}

fn text_of(s: &Session, o: Guid) -> String {
    s.current_page().unwrap().outline(o).unwrap().text()
}

fn issues(s: &mut Session) -> Vec<Value> {
    s.run(ids::PROOF_CHECK, &json!({})).unwrap().value["issues"].as_array().cloned().unwrap_or_default()
}

#[test]
fn spelling_is_found_suggested_and_fixed() {
    let (mut s, o) = session_with("The quick brwn fox\njumps ovr the dog, and the brwn cat");
    let found = issues(&mut s);
    let words: Vec<&str> = found.iter().map(|i| i["word"].as_str().unwrap()).collect();
    assert_eq!(words, ["brwn", "ovr", "brwn"]);
    assert_eq!((found[0]["para"].as_u64(), found[0]["start"].as_u64(), found[0]["end"].as_u64()), (Some(0), Some(10), Some(14)));
    // The issue at a caret inside the word, with suggestions from the English list.
    let sug = s.run(ids::PROOF_SUGGESTIONS, &json!({"outline": o, "at": {"para": 1, "char": 7}})).unwrap().value;
    assert_eq!(sug["word"], "ovr");
    let list: Vec<String> = sug["suggestions"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect();
    assert!(list.iter().any(|w| w == "over"), "{list:?}");
    assert!(s.run(ids::PROOF_SUGGESTIONS, &json!({"outline": o, "at": {"para": 0, "char": 1}})).unwrap().value.is_null());
    // Change: one undo step, the formatting kept.
    s.run(ids::FORMAT_BOLD, &json!({"outline": o, "start": {"para": 1, "char": 6}, "end": {"para": 1, "char": 9}})).unwrap();
    let out = s.run(ids::PROOF_APPLY, &json!({"outline": o, "para": 1, "start": 6, "end": 9, "text": "over", "word": "ovr"})).unwrap();
    assert_eq!(out.value["caret"], json!({"para": 1, "char": 10}));
    assert_eq!(text_of(&s, o), "The quick brwn fox\njumps over the dog, and the brwn cat");
    let run = s.current_page().unwrap().outline(o).unwrap().paragraphs[1].runs.iter().find(|r| r.text.contains("over")).cloned().unwrap();
    assert!(run.bold, "the word keeps its formatting");
    // A stale fix is refused and changes nothing.
    assert!(matches!(
        s.run(ids::PROOF_APPLY, &json!({"outline": o, "para": 1, "start": 6, "end": 9, "text": "x", "word": "ovr"})),
        Err(CmdError::Failed(_))
    ));
    // Change All.
    let all = s.run(ids::PROOF_CHANGE_ALL, &json!({"word": "brwn", "text": "brown"})).unwrap();
    assert_eq!(all.value["changed"], 2);
    assert_eq!(text_of(&s, o), "The quick brown fox\njumps over the dog, and the brown cat");
    assert!(issues(&mut s).is_empty());
    s.undo();
    assert_eq!(issues(&mut s).len(), 2);
}

#[test]
fn ignore_ignore_all_and_the_user_dictionary() {
    let (mut s, o) = session_with("zqvexa here and zqvexa there, plus wqoptr and ykfrdl");
    assert_eq!(issues(&mut s).len(), 4);
    let depth = s.undo_depth();
    // Ignore: only that one, while the text is the same.
    let r = s.run(ids::PROOF_IGNORE, &json!({"outline": o, "para": 0, "start": 0, "end": 6})).unwrap();
    assert_eq!(r.value["ignored"], true);
    assert_eq!(s.undo_depth(), depth, "ignoring isn't an edit");
    let left: Vec<String> = issues(&mut s).iter().map(|i| i["word"].as_str().unwrap().to_owned()).collect();
    assert_eq!(left, ["zqvexa", "wqoptr", "ykfrdl"]);
    // Ignore All: every one, this run.
    s.run(ids::PROOF_IGNORE_ALL, &json!({"word": "zqvexa"})).unwrap();
    assert_eq!(issues(&mut s).len(), 2);
    // Add to Dictionary: listed for the app to keep.
    let out = s.run(ids::PROOF_ADD_TO_DICTIONARY, &json!({"word": "Wqoptr"})).unwrap();
    assert!(out.value["words"].as_array().unwrap().iter().any(|w| w == "wqoptr"));
    assert_eq!(issues(&mut s).len(), 1);
    assert!(crate::proof::user_words().contains(&"wqoptr".to_owned()));
    let gone = s.run(ids::PROOF_REMOVE_FROM_DICTIONARY, &json!({"word": "wqoptr"})).unwrap();
    assert_eq!(gone.value["removed"], true);
    assert_eq!(issues(&mut s).len(), 2);
    for bad in [json!({"word": "two words"}), json!({"word": ""}), json!({"word": "x".repeat(65)}), json!({})] {
        assert!(matches!(s.run(ids::PROOF_ADD_TO_DICTIONARY, &bad), Err(CmdError::Params(_))), "{bad}");
    }
}

#[test]
fn languages_per_paragraph() {
    let (mut s, o) = session_with("the colour of it\nqzxvbq words");
    let words = |s: &mut Session| issues(s).iter().map(|i| i["word"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    // US English by default: the British spelling is flagged (craft-proof's dialect policy).
    assert!(words(&mut s).contains(&"qzxvbq".to_owned()));
    // British English for the first paragraph.
    s.run(ids::PROOF_SET_LANGUAGE, &json!({"outline": o, "start": {"para": 0, "char": 0}, "lang": "en-GB"})).unwrap();
    let page = s.current_page().unwrap();
    assert_eq!(page.outline(o).unwrap().paragraphs[0].lang.as_deref(), Some("en-GB"));
    assert_eq!(page.outline(o).unwrap().paragraphs[1].lang, None);
    assert!(!words(&mut s).contains(&"colour".to_owned()));
    // Don't check: no marks at all on the page.
    let out = s.run(ids::PROOF_SET_LANGUAGE, &json!({"lang": "zxx"})).unwrap();
    assert_eq!(out.value["paragraphs"], 2);
    assert!(words(&mut s).is_empty());
    // Back to the default; an undo step each.
    s.run(ids::PROOF_SET_LANGUAGE, &json!({"lang": null})).unwrap();
    assert_eq!(s.current_page().unwrap().outline(o).unwrap().paragraphs[0].lang, None);
    s.undo();
    assert_eq!(s.current_page().unwrap().outline(o).unwrap().paragraphs[0].lang.as_deref(), Some("zxx"));
    assert!(matches!(s.run(ids::PROOF_SET_LANGUAGE, &json!({"lang": "!!"})), Err(CmdError::Params(_))));
    // The default language.
    s.run(ids::PROOF_SET_DEFAULT_LANGUAGE, &json!({"lang": "pl-PL"})).unwrap();
    let langs = s.run(ids::PROOF_LANGUAGES, &json!({})).unwrap().value;
    assert_eq!(langs["default"], "pl-PL");
    assert!(langs["languages"].as_array().unwrap().iter().any(|l| l["tag"] == "en-GB" && l["dictionary"] == true));
}

#[test]
fn word_count_of_page_and_selection() {
    let (mut s, o) = session_with("Two words\n\nthree more words — 42");
    let page = s.run(ids::REVIEW_WORD_COUNT, &json!({})).unwrap().value;
    // The page title "Review" counts too.
    assert_eq!(page["scope"], "page");
    assert_eq!(page["words"], 7);
    assert_eq!(page["paragraphs"], 3);
    let sel = s.run(ids::REVIEW_WORD_COUNT, &json!({"outline": o, "start": {"para": 0, "char": 4}, "end": {"para": 2, "char": 5}})).unwrap().value;
    assert_eq!(sel["scope"], "selection");
    assert_eq!(sel["words"], 2);
    assert_eq!(sel["characters"], 10);
    assert_eq!(sel["charactersNoSpaces"], 10, "paragraph breaks aren't characters");
}

#[test]
fn links_are_set_read_followed_and_removed() {
    let (mut s, o) = session_with("visit the site today");
    let out = s
        .run(ids::LINK_SET, &json!({"outline": o, "start": {"para": 0, "char": 6}, "end": {"para": 0, "char": 14}, "url": "www.example.com"}))
        .unwrap();
    assert_eq!(out.value["url"], "https://www.example.com");
    let link = s.run(ids::LINK_GET, &json!({"outline": o, "at": {"para": 0, "char": 8}})).unwrap().value;
    assert_eq!((link["text"].as_str(), link["start"]["char"].as_u64(), link["end"]["char"].as_u64()), (Some("the site"), Some(6), Some(14)));
    // New text for the link.
    s.run(
        ids::LINK_SET,
        &json!({"outline": o, "start": {"para": 0, "char": 6}, "end": {"para": 0, "char": 14}, "url": "https://example.org", "text": "our page"}),
    )
    .unwrap();
    assert_eq!(text_of(&s, o), "visit our page today");
    // A link inserted at a caret shows its address.
    s.run(ids::LINK_SET, &json!({"outline": o, "at": {"para": 0, "char": 20}, "url": "mailto:me@example.org"})).unwrap();
    assert_eq!(text_of(&s, o), "visit our page todaymailto:me@example.org");
    // Follow: the front end opens web links.
    assert_eq!(s.run(ids::LINK_FOLLOW, &json!({"url": "https://example.org"})).unwrap().value["external"], "https://example.org");
    for bad in ["javascript:alert(1)", "data:text/html,x", "", "notecraft:page?id=nope"] {
        assert!(s.run(ids::LINK_SET, &json!({"outline": o, "url": bad})).is_err(), "{bad}");
        assert!(s.run(ids::LINK_FOLLOW, &json!({"url": bad})).is_err(), "{bad}");
    }
    // Remove the link around a caret.
    s.run(ids::LINK_REMOVE, &json!({"outline": o, "at": {"para": 0, "char": 8}})).unwrap();
    assert!(s.run(ids::LINK_GET, &json!({"outline": o, "at": {"para": 0, "char": 8}})).unwrap().value.is_null());
    let runs = &s.current_page().unwrap().outline(o).unwrap().paragraphs[0].runs;
    assert_eq!(runs.iter().filter(|r| r.link.is_some()).count(), 1, "the mail link stays");
}

#[test]
fn notecraft_links_to_pages_and_paragraphs() {
    let (mut s, o) = session_with("first\nsecond\nthird");
    let target = s.current.page.unwrap();
    let to_page = s.run(ids::LINK_TO_PAGE, &json!({})).unwrap().value["url"].as_str().unwrap().to_owned();
    let l = InternalLink::parse(&to_page).unwrap();
    assert_eq!((l.page, l.section, l.notebook, l.title.as_deref()), (Some(target), s.current.section, s.current.notebook, Some("Review")));
    let to_para = s.run(ids::LINK_TO_PARAGRAPH, &json!({"outline": o, "para": 2})).unwrap().value["url"].as_str().unwrap().to_owned();
    // From another page, the link goes back there.
    s.run(ids::PAGE_NEW, &json!({"title": "Elsewhere"})).unwrap();
    let back = s.run(ids::LINK_FOLLOW, &json!({"url": to_para})).unwrap().value;
    assert_eq!(s.current.page, Some(target));
    assert_eq!((back["internal"].as_bool(), back["outline"].as_str().and_then(Guid::parse), back["para"].as_u64()), (Some(true), Some(o), Some(2)));
    // A link whose page is gone finds a page of that title.
    let mut moved = l.clone();
    moved.page = Some(Guid::new());
    s.run(ids::PAGE_NEW, &json!({"title": "Elsewhere 2"})).unwrap();
    s.run(ids::LINK_FOLLOW, &json!({"url": moved.to_url()})).unwrap();
    assert_eq!(s.current.page, Some(target));
    let nowhere = InternalLink { page: Some(Guid::new()), ..InternalLink::default() };
    assert!(matches!(s.run(ids::LINK_FOLLOW, &json!({"url": nowhere.to_url()})), Err(CmdError::Failed(_))));
}

#[test]
fn wiki_links_create_or_find_pages() {
    let (mut s, o) = session_with("");
    let host = s.current.page.unwrap();
    let sec = s.current.section.unwrap();
    let pages_before = s.current_section().unwrap().pages.len();
    s.run(ids::TEXT_INSERT, &json!({"outline": o, "text": "See [[Shopping list]]"})).unwrap();
    let out = s.run(ids::LINK_WIKI, &json!({"outline": o, "at": {"para": 0, "char": 21}})).unwrap();
    assert_eq!(out.value["created"], true);
    assert_eq!(out.value["caret"], json!({"para": 0, "char": 17}));
    assert_eq!(text_of(&s, o), "See Shopping list");
    assert_eq!(s.current.page, Some(host), "the caret stays on the page");
    let created = gid(&out.value, "page");
    assert_eq!(s.current_section().unwrap().pages.len(), pages_before + 1);
    assert_eq!(s.current_section().unwrap().page(created).map(|p| p.title.as_str()), Some("Shopping list"));
    let run = s.current_page().unwrap().outline(o).unwrap().paragraphs[0].runs.iter().find(|r| r.text == "Shopping list").cloned().unwrap();
    let l = InternalLink::parse(run.link.as_deref().unwrap()).unwrap();
    assert_eq!((l.page, l.section), (Some(created), Some(sec)));
    // An existing page (any case) is linked, not made again; one undo step undoes it all.
    s.run(ids::TEXT_INSERT, &json!({"outline": o, "text": " and [[shopping LIST]]"})).unwrap();
    let again = s.run(ids::LINK_WIKI, &json!({"outline": o, "at": {"para": 0, "char": 39}})).unwrap();
    assert_eq!((again.value["created"].as_bool(), gid(&again.value, "page")), (Some(false), created));
    assert_eq!(s.current_section().unwrap().pages.len(), pages_before + 1);
    // Nothing to do without [[…]] before the caret.
    assert!(s.run(ids::LINK_WIKI, &json!({"outline": o, "at": {"para": 0, "char": 3}})).unwrap().value.is_null());
    s.undo();
    s.undo();
    s.undo();
    assert_eq!(s.current_section().unwrap().pages.len(), pages_before);
    assert_eq!(text_of(&s, o), "See [[Shopping list]]");
}

#[test]
fn attachments_and_pictures_are_saved() {
    let mut s = Session::new_default();
    s.run(ids::PAGE_NEW, &json!({"title": "Files"})).unwrap();
    let f = gid(&s.run(ids::INSERT_FILE, &json!({"bytesBase64": STANDARD.encode(b"hello file"), "name": "notes.txt"})).unwrap().value, "id");
    let out = s.run(ids::FILE_EXPORT_ATTACHMENT, &json!({"element": f})).unwrap().value;
    assert_eq!(out["fileName"], "notes.txt");
    assert_eq!(STANDARD.decode(out["bytesBase64"].as_str().unwrap()).unwrap(), b"hello file");
    let pic = gid(
        &s.run(ids::INSERT_IMAGE, &json!({"bytesBase64": super::commands::png_b64(4, 4), "name": "p.png", "alt": "Sunset / beach"})).unwrap().value,
        "id",
    );
    let out = s.run(ids::FILE_EXPORT_ATTACHMENT, &json!({"id": pic})).unwrap().value;
    assert!(out["fileName"].as_str().unwrap().ends_with(".png") && !out["fileName"].as_str().unwrap().contains('/'));
    assert_eq!(out["mime"], "image/png");
    let o = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 300, "text": "x"})).unwrap().value, "id");
    assert!(matches!(s.run(ids::FILE_EXPORT_ATTACHMENT, &json!({"element": o})), Err(CmdError::Params(_))));
    assert!(matches!(s.run(ids::FILE_EXPORT_ATTACHMENT, &json!({"element": Guid::new()})), Err(CmdError::Params(_))));
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = crate::io::safe_save::tests::temp_dir("attach");
        let path = dir.join("out.txt");
        s.run(ids::FILE_EXPORT_ATTACHMENT, &json!({"element": f, "path": path.to_string_lossy()})).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello file");
    }
    assert!(s.current_page().unwrap().elements.iter().any(|e| matches!(e.kind, ElementKind::EmbeddedFile { .. })));
}

#[test]
fn links_and_languages_survive_a_one_round_trip() {
    let (mut s, o) = session_with("colour and [[Other page]]");
    s.run(ids::PROOF_SET_LANGUAGE, &json!({"outline": o, "lang": "en-GB"})).unwrap();
    s.run(ids::LINK_WIKI, &json!({"outline": o, "at": {"para": 0, "char": 25}})).unwrap();
    s.run(ids::LINK_SET, &json!({"outline": o, "start": {"para": 0, "char": 0}, "end": {"para": 0, "char": 6}, "url": "https://example.com/c"}))
        .unwrap();
    let sec = s.current_section().unwrap().clone();
    let bytes = notecraft_one::write_section(&sec).unwrap();
    let back = notecraft_one::read_section(&bytes).unwrap();
    let page = back.page(s.current.page.unwrap()).unwrap();
    let para = &page.outline(o).unwrap().paragraphs[0];
    assert_eq!(para.lang.as_deref(), Some("en-GB"));
    let links: Vec<&str> = para.runs.iter().filter_map(|r| r.link.as_deref()).collect();
    assert_eq!(links.len(), 2, "{:?}", para.runs);
    assert_eq!(links[0], "https://example.com/c");
    assert!(InternalLink::parse(links[1]).and_then(|l| l.page).is_some_and(|p| back.page(p).is_some()));
}
