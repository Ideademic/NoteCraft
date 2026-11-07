# Contributing to NoteCraft

Thanks for helping. NoteCraft is a community Craft app by Ideademic, and it follows the Craft
apps' conventions, so the rules are short but firm.

1. **Read [AGENTS.md](AGENTS.md) first.** It holds the house rules, the architecture and the
   quality gates, for people and coding agents alike.
2. **Never crash.** No `unwrap`, `expect`, `panic!` or `unsafe` in non-test code; treat every file
   as hostile. A crash fix comes with a test that panicked before it.
3. **Clean room.** The `.one` / `.onetoc2` / `.onepkg` code is written only from Microsoft's
   published Open Specifications and black-box observation. Never read or port another OneNote
   implementation's source (including `onenote_parser`, `libmson`, `pyOneNote`, Tika's or
   LibreOffice's code), and never copy Microsoft's artwork, icons, templates or wording.
4. **Assets.** Every icon, image or sample file is original or openly licensed and gets a row in
   [ATTRIBUTION.md](ATTRIBUTION.md) in the same change (`cargo xtask assets` checks this). No
   ArtCraft or Storyteller logos.
5. **Licences.** Dependencies must be MIT, Apache-2.0, BSD-2/3-Clause, ISC, OFL-1.1 or CC0-1.0
   (no GPL, LGPL, AGPL or MPL), declared once in the root `Cargo.toml`.
6. **Before you push:** `cargo xtask ci` (rustfmt, clippy `-D warnings`, tests, assets, layering,
   wasm). Install the `wasm32-unknown-unknown` target and `cargo-nextest` for the full run.
7. **Pull requests** go to `main`, stay narrow, and say what you tested and what you couldn't run.

By contributing you agree that your work is licensed under MIT OR Apache-2.0, like the rest of
NoteCraft.

Security problems: please follow [SECURITY.md](SECURITY.md) instead of opening an issue.
