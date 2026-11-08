//! Spelling for Review › Spelling and the red wavy underlines, on craft-proof.
//!
//! A paragraph is checked in its own language ([`notecraft_doc::Paragraph::lang`]) or the
//! session's default ([`ProofState::default_lang`], which the app sets from the system). Results
//! are cached per paragraph text, language and craft-proof's [`craft_proof::dictionary_generation`]
//! (bumped by Add to Dictionary and Ignore All), so redrawing a page costs a hash lookup per
//! paragraph.
//!
//! The canvas never waits for a check: [`lookup`] answers from the cache or queues the paragraph.
//! On the desktop a background thread works through the queue; in the browser (no threads) the
//! canvas calls [`work`] with a time budget each frame. Commands ([`issues`]) check at once.
//!
//! Positions are characters (Unicode scalar values) in the paragraph's text, like the selection
//! model's `char`. "Ignore" (once) is kept in the session: it holds while the paragraph's text is
//! unchanged.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use craft_proof::{IssueKind, ProofLang};
use notecraft_doc::Paragraph;

/// Most cached paragraph results before the cache starts over.
const MAX_CACHE: usize = 8192;
/// Most paragraphs waiting to be checked; more are dropped (they are asked for again).
const MAX_QUEUE: usize = 4096;
/// Longest paragraph checked, characters (longer text is checked up to here).
pub const MAX_CHECK_CHARS: usize = 20_000;
/// Most issues reported for one paragraph.
const MAX_ISSUES_PER_PARA: usize = 500;

/// A misspelled word in a paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Misspelling {
    /// Characters from the paragraph's start, `start..end`.
    pub start: usize,
    pub end: usize,
    pub word: String,
    /// The language it was checked in (`en`, `en-gb`…, craft-proof's tag).
    pub lang: String,
}

/// The session's proofing state: the default language and words ignored once.
#[derive(Clone, Debug)]
pub struct ProofState {
    /// The proofing language of paragraphs without one (BCP 47), the system's by default.
    pub default_lang: String,
    /// Ignore (once): (paragraph text hash, start, word).
    ignored: HashSet<(u64, usize, String)>,
    /// Bumped whenever what is ignored changes (callers caching results compare it).
    pub generation: u64,
}

impl Default for ProofState {
    fn default() -> Self {
        ProofState { default_lang: "en-US".into(), ignored: HashSet::new(), generation: 0 }
    }
}

impl ProofState {
    /// The language paragraph `p` is checked in.
    pub fn lang_of(&self, p: &Paragraph) -> ProofLang {
        ProofLang::from_tag(Some(p.lang.as_deref().unwrap_or(&self.default_lang)))
    }

    /// Ignores the misspelling `m` of a paragraph whose text is `text` until the text changes.
    pub fn ignore_once(&mut self, text: &str, m: &Misspelling) {
        if self.ignored.len() >= 10_000 {
            self.ignored.clear();
        }
        if self.ignored.insert((text_hash(text), m.start, m.word.clone())) {
            self.generation += 1;
        }
    }

    /// `issues` of a paragraph whose text is `text`, less the ones ignored once.
    pub fn visible(&self, text: &str, issues: &[Misspelling]) -> Vec<Misspelling> {
        if self.ignored.is_empty() {
            return issues.to_vec();
        }
        let h = text_hash(text);
        issues.iter().filter(|m| !self.ignored.contains(&(h, m.start, m.word.clone()))).cloned().collect()
    }

    /// Whether anything was ignored once (callers can skip [`ProofState::visible`]).
    pub fn has_ignored(&self) -> bool {
        !self.ignored.is_empty()
    }
}

fn text_hash(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// The cache key of `text` in `lang` with today's dictionaries.
fn key(text: &str, lang: ProofLang) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    lang.hash(&mut h);
    craft_proof::dictionary_generation().hash(&mut h);
    h.finish()
}

#[derive(Default)]
struct Shared {
    done: HashMap<u64, Arc<Vec<Misspelling>>>,
    queued: HashSet<u64>,
    queue: VecDeque<(u64, String, ProofLang)>,
}

static SHARED: LazyLock<Mutex<Shared>> = LazyLock::new(|| Mutex::new(Shared::default()));

fn shared() -> MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn store(k: u64, v: Arc<Vec<Misspelling>>) {
    let mut s = shared();
    if s.done.len() >= MAX_CACHE {
        s.done.clear();
    }
    s.queued.remove(&k);
    s.done.insert(k, v);
}

/// Checks `text` in `lang` now (no cache).
pub fn check(text: &str, lang: ProofLang) -> Vec<Misspelling> {
    let text: String = text.chars().take(MAX_CHECK_CHARS).collect();
    let spans = [(0..text.len(), lang)];
    let issues = craft_proof::check_text_with(&text, &spans, &craft_proof::Options::default(), false, false);
    let mut out = Vec::new();
    let tag = lang.tag().into_owned();
    for i in issues.into_iter().filter(|i| i.kind == IssueKind::Spelling) {
        let (Some(before), Some(word)) = (text.get(..i.start), text.get(i.start..i.end.max(i.start))) else { continue };
        let start = before.chars().count();
        let n = word.chars().count();
        if n == 0 {
            continue;
        }
        out.push(Misspelling { start, end: start + n, word: word.to_owned(), lang: tag.clone() });
        if out.len() >= MAX_ISSUES_PER_PARA {
            break;
        }
    }
    out
}

/// The misspellings of paragraph `p` (cached; checked now when not cached), less those ignored
/// once.
pub fn issues(state: &ProofState, p: &Paragraph) -> Vec<Misspelling> {
    let text = p.text();
    let lang = state.lang_of(p);
    let k = key(&text, lang);
    let cached = shared().done.get(&k).cloned();
    let all = match cached {
        Some(v) => v,
        None => {
            let v = Arc::new(check(&text, lang));
            store(k, v.clone());
            v
        }
    };
    state.visible(&text, &all)
}

/// The misspellings of `text` in `lang` when they are known; otherwise the paragraph is queued
/// for checking ([`work`], or the desktop's worker thread) and `None` is returned.
pub fn lookup(text: &str, lang: ProofLang) -> Option<Arc<Vec<Misspelling>>> {
    let k = key(text, lang);
    let mut s = shared();
    if let Some(v) = s.done.get(&k) {
        return Some(v.clone());
    }
    if !s.queued.contains(&k) && s.queue.len() < MAX_QUEUE {
        s.queued.insert(k);
        s.queue.push_back((k, text.chars().take(MAX_CHECK_CHARS).collect(), lang));
        drop(s);
        wake_worker();
    }
    None
}

/// How many paragraphs wait to be checked.
pub fn pending() -> usize {
    shared().queue.len()
}

/// Checks queued paragraphs until `budget_ms` milliseconds have passed (at least one when any
/// waits). Returns how many were checked. The browser calls this every frame; on the desktop the
/// worker thread does it.
pub fn work(budget_ms: f64) -> usize {
    let start = web_time::Instant::now();
    let mut n = 0;
    loop {
        let job = shared().queue.pop_front();
        let Some((k, text, lang)) = job else { break };
        let v = Arc::new(check(&text, lang));
        store(k, v);
        n += 1;
        if start.elapsed().as_secs_f64() * 1000.0 >= budget_ms {
            break;
        }
    }
    n
}

#[cfg(not(target_arch = "wasm32"))]
fn wake_worker() {
    use std::sync::OnceLock;
    use std::sync::mpsc::{Sender, channel};
    static WORKER: OnceLock<Mutex<Option<Sender<()>>>> = OnceLock::new();
    let cell = WORKER.get_or_init(|| {
        let (tx, rx) = channel::<()>();
        let spawned = std::thread::Builder::new().name("notecraft-proof".into()).spawn(move || {
            while rx.recv().is_ok() {
                while work(50.0) > 0 {}
            }
        });
        Mutex::new(spawned.ok().map(|_| tx))
    });
    let tx = cell.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(tx) = tx.as_ref() {
        let _ = tx.send(());
    }
}

#[cfg(target_arch = "wasm32")]
fn wake_worker() {}

/// Whether paragraphs are checked by a background thread (the desktop) rather than by [`work`].
pub const BACKGROUND: bool = cfg!(not(target_arch = "wasm32"));

/// Loads the dictionaries ahead of the first check (the desktop does it on its worker thread, so
/// the first underline doesn't wait for them).
pub fn warm_up() {
    let _ = lookup("NoteCraft", ProofLang::En);
}

/// Spelling suggestions for `word` in the language with craft-proof tag `lang`, best first.
pub fn suggestions(word: &str, lang: &str, max: usize) -> Vec<String> {
    let word: String = word.chars().take(craft_proof::MAX_CUSTOM_WORD_CHARS * 2).collect();
    craft_proof::suggest_in(&word, max.min(20), ProofLang::from_tag(Some(lang)))
}

// --- the user dictionary --------------------------------------------------------------------

/// Words in the user dictionary, sorted (the app keeps them in its settings).
pub fn user_words() -> Vec<String> {
    craft_proof::user_dictionary()
}

/// Adds the app's saved user dictionary (one word per entry; junk is skipped).
pub fn load_user_words(words: &[String]) {
    for w in words.iter().take(100_000) {
        if let Some(w) = craft_proof::custom_word(w) {
            craft_proof::add_word(&w);
        }
    }
}

/// Changes whenever the user dictionary or Ignore All changes.
pub fn dictionary_generation() -> u64 {
    craft_proof::dictionary_generation()
}

// --- languages --------------------------------------------------------------------------------

/// The proofing languages offered (Review › Language): (tag, name). `zxx` is "do not check".
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en-US", "English (United States)"),
    ("en-GB", "English (United Kingdom)"),
    ("en-AU", "English (Australia)"),
    ("en-CA", "English (Canada)"),
    ("cs-CZ", "Czech"),
    ("hr-HR", "Croatian"),
    ("id-ID", "Indonesian"),
    ("it-IT", "Italian"),
    ("pl-PL", "Polish"),
    ("pt-BR", "Portuguese (Brazil)"),
    ("ru-RU", "Russian"),
    ("es-ES", "Spanish (Spain)"),
    ("es-MX", "Spanish (Mexico)"),
    ("es-419", "Spanish (Latin America)"),
    ("es-UY", "Spanish (Uruguay)"),
    ("tr-TR", "Turkish"),
    ("de-DE", "German"),
    ("fr-FR", "French"),
    ("nl-NL", "Dutch"),
    ("zxx", "Do not check spelling"),
];

/// The display name of a language tag (the tag itself when NoteCraft has no name for it).
pub fn language_name(tag: &str) -> String {
    let t = tag.trim();
    LANGUAGES
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(t))
        .or_else(|| LANGUAGES.iter().find(|(k, _)| k.split('-').next().is_some_and(|p| p.eq_ignore_ascii_case(t))))
        .map_or_else(|| t.to_owned(), |(_, n)| (*n).to_owned())
}

/// Whether text in `tag` gets spelling marks in this build (a dictionary was found for it).
pub fn has_dictionary(tag: &str) -> bool {
    let lang = ProofLang::from_tag(Some(tag));
    lang != ProofLang::Other && (lang.is_english() || craft_proof::has_dictionary(Some(tag)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_misspellings_in_characters() {
        let m = check("The — qzxvbw is here, cat", ProofLang::En);
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!((m[0].start, m[0].end, m[0].word.as_str()), (6, 12, "qzxvbw"));
        assert!(check("The cat sat on the mat.", ProofLang::En).is_empty());
        assert!(check("qzxvbw", ProofLang::Other).is_empty(), "no proofing tools: no marks");
    }

    #[test]
    fn lookup_queues_then_answers() {
        let text = "a unique qzxvbwq paragraph for the queue test";
        let mut answer = lookup(text, ProofLang::En);
        for _ in 0..200 {
            if answer.is_some() {
                break;
            }
            work(50.0);
            std::thread::sleep(std::time::Duration::from_millis(5));
            answer = lookup(text, ProofLang::En);
        }
        let answer = answer.unwrap();
        assert_eq!(answer.iter().map(|m| m.word.as_str()).collect::<Vec<_>>(), vec!["qzxvbwq"]);
    }

    #[test]
    fn ignore_once_holds_until_the_text_changes() {
        let mut st = ProofState::default();
        let p = Paragraph::plain("one qzxvbwr two");
        let found = issues(&st, &p);
        assert_eq!(found.len(), 1);
        st.ignore_once(&p.text(), &found[0]);
        assert!(issues(&st, &p).is_empty());
        assert_eq!(issues(&st, &Paragraph::plain("one qzxvbwr two!")).len(), 1);
        assert_eq!(language_name("en-gb"), "English (United Kingdom)");
        assert_eq!(language_name("es"), "Spanish (Spain)");
        assert_eq!(language_name("xx-YY"), "xx-YY");
        assert!(has_dictionary("en-US") && !has_dictionary("zxx"));
    }
}
