//! KeyTips: press and release Alt (or F10) and every ribbon tab shows a letter; type it and the
//! tab's controls show theirs; type one of those and the control runs, as in Office. Esc steps
//! back, a click anywhere puts them away.
//!
//! Controls don't name their own KeyTips: each control drawn on the ribbon while KeyTips are up
//! takes the next label in drawing order ([`Keytips::control`]), single letters while a tab has
//! up to 26 controls, two letters beyond that so no label is the start of another.

use egui::{Align2, Key, Rect, Stroke, pos2, vec2};

use crate::ribbon::RibbonTab;
use crate::theme::{Tokens, regular};

/// What KeyTips show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Off,
    /// The ribbon tabs (and the Quick Access Toolbar) show their letters.
    Tabs,
    /// The selected tab's controls show theirs.
    Controls,
}

/// KeyTip state between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keytips {
    pub phase: Phase,
    /// The letters typed so far in [`Phase::Controls`].
    pub prefix: String,
    /// Controls counted this frame, and last frame's total (it picks one or two letters).
    counter: usize,
    last_total: usize,
    /// Labels drawn this frame with where (for the overlay and for matching typed letters).
    pub drawn: Vec<(Rect, String)>,
    /// Last frame's labels.
    previous: Vec<String>,
    /// The label typed in full: that control acts as if clicked this frame.
    pub fire: Option<String>,
    /// Alt went down with nothing else pressed since.
    alt_armed: bool,
    /// Alt was down last frame.
    prev_alt: bool,
}

/// The tab letters.
pub fn tab_letter(tab: RibbonTab) -> &'static str {
    match tab {
        RibbonTab::File => "F",
        RibbonTab::Home => "H",
        RibbonTab::Insert => "N",
        RibbonTab::Draw => "D",
        RibbonTab::History => "S",
        RibbonTab::Review => "R",
        RibbonTab::View => "W",
        RibbonTab::Table => "JL",
    }
}

/// The `i`th label of `total` controls.
pub fn label(i: usize, total: usize) -> String {
    const A: &[u8; 26] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let ch = |k: usize| char::from(A.get(k % 26).copied().unwrap_or(b'A'));
    if total <= 26 { ch(i).to_string() } else { format!("{}{}", ch(i / 26), ch(i)) }
}

impl Keytips {
    /// Starts a frame: forgets last frame's labels (keeping their names for matching).
    pub fn begin_frame(&mut self) {
        self.previous = self.drawn.drain(..).map(|(_, l)| l).collect();
        self.last_total = self.counter;
        self.counter = 0;
    }

    /// Ends a frame: a label fired once is spent.
    pub fn end_frame(&mut self) {
        self.fire = None;
    }

    /// Whether KeyTips are showing at all.
    pub fn active(&self) -> bool {
        self.phase != Phase::Off
    }

    /// A ribbon control at `rect` is drawn: gives it the next label while controls show theirs.
    /// Returns whether its label was just typed (the control should act as clicked).
    pub fn control(&mut self, rect: Rect) -> bool {
        if self.phase != Phase::Controls && self.fire.is_none() {
            return false;
        }
        let l = label(self.counter, self.last_total.max(self.counter + 1));
        self.counter += 1;
        let fired = self.fire.as_deref() == Some(l.as_str());
        if self.phase == Phase::Controls && l.starts_with(&self.prefix) {
            self.drawn.push((rect, l));
        }
        fired
    }

    /// A tab (or Quick Access Toolbar button) at `rect` with its letter.
    pub fn tab(&mut self, rect: Rect, letter: &str) {
        if self.phase == Phase::Tabs {
            self.drawn.push((rect, letter.to_owned()));
        }
    }

    /// Hides them.
    pub fn off(&mut self) {
        self.phase = Phase::Off;
        self.prefix.clear();
    }

    /// Reads this frame's keys. Returns the tab chosen by its letter, if any.
    pub fn handle_keys(&mut self, ctx: &egui::Context) -> Option<RibbonTab> {
        let (events, mods, clicked) = ctx.input(|i| (i.events.clone(), i.modifiers, i.pointer.any_click()));
        if clicked && self.active() {
            self.off();
            return None;
        }
        // Alt tapped alone toggles them (egui reports modifier changes through `modifiers`).
        let alone = mods.alt && !mods.ctrl && !mods.command && !mods.shift;
        if alone && !self.prev_alt {
            self.alt_armed = true;
        }
        if mods.alt
            && (!alone || events.iter().any(|e| matches!(e, egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::PointerButton { .. })))
        {
            self.alt_armed = false;
        }
        if !mods.alt && self.prev_alt && self.alt_armed {
            self.alt_armed = false;
            if self.active() { self.off() } else { self.phase = Phase::Tabs }
        }
        self.prev_alt = mods.alt;
        let mut chosen = None;
        for e in &events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            if *key == Key::F10 && !modifiers.any() {
                if self.active() {
                    self.off()
                } else {
                    self.phase = Phase::Tabs
                }
                continue;
            }
            if !self.active() {
                continue;
            }
            if *key == Key::Escape {
                match self.phase {
                    Phase::Controls => {
                        self.phase = Phase::Tabs;
                        self.prefix.clear();
                    }
                    _ => self.off(),
                }
                continue;
            }
            let Some(letter) = key_letter(*key) else { continue };
            match self.phase {
                Phase::Tabs => {
                    if let Some(tab) = RibbonTab::ALL.into_iter().find(|t| tab_letter(*t).starts_with(letter)) {
                        chosen = Some(tab);
                        self.phase = if tab == RibbonTab::File { Phase::Off } else { Phase::Controls };
                        self.prefix.clear();
                    }
                }
                Phase::Controls => {
                    let next = format!("{}{letter}", self.prefix);
                    if self.previous.contains(&next) {
                        self.fire = Some(next);
                        self.off();
                    } else if self.previous.iter().any(|l| l.starts_with(&next)) {
                        self.prefix = next;
                    }
                }
                Phase::Off => {}
            }
        }
        chosen
    }

    /// Draws the badges over everything.
    pub fn paint(&self, ctx: &egui::Context, tokens: &Tokens) {
        if !self.active() || self.drawn.is_empty() {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("nc_keytips")));
        for (r, l) in &self.drawn {
            let w = 8.0 + 7.0 * l.chars().count() as f32;
            let at = pos2(r.center().x, r.max.y - 4.0);
            let badge = Rect::from_center_size(at, vec2(w, 16.0));
            painter.rect(badge, 2.0, tokens.text, Stroke::new(1.0, tokens.panel), egui::StrokeKind::Inside);
            painter.text(badge.center(), Align2::CENTER_CENTER, l, regular(11.0), tokens.panel);
        }
    }
}

fn key_letter(key: Key) -> Option<char> {
    let name = key.name();
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_uppercase()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_never_prefix_each_other() {
        assert_eq!(label(0, 3), "A");
        assert_eq!(label(25, 26), "Z");
        let many: Vec<String> = (0..40).map(|i| label(i, 40)).collect();
        assert!(many.iter().all(|l| l.len() == 2));
        for a in &many {
            assert_eq!(many.iter().filter(|b| b.starts_with(a.as_str())).count(), 1, "{a}");
        }
        let tabs: Vec<&str> = RibbonTab::ALL.iter().map(|t| tab_letter(*t)).collect();
        let mut sorted = tabs.clone();
        sorted.dedup();
        assert_eq!(tabs.len(), sorted.len());
    }

    #[test]
    fn controls_fire_by_label() {
        let mut k = Keytips { phase: Phase::Controls, ..Default::default() };
        k.begin_frame();
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0));
        assert!(!k.control(r));
        assert!(!k.control(r));
        assert_eq!(k.drawn.len(), 2);
        k.begin_frame();
        k.fire = Some("B".into());
        k.phase = Phase::Off;
        assert!(!k.control(r));
        assert!(k.control(r), "the second control is B");
        k.end_frame();
        assert!(k.fire.is_none());
    }
}
