<!-- craft-house-rules:start core source=dde09ea31e2e457f5b4653a6e76f7cdf55944bdf sha256=540e6cea75d12d0710c3dca89658328e81e508ecef102446ca5bb95b7c0b92bb -->
## House rules

These apply in every Craft repository. A later rule may be stricter, not weaker. The source is `standards/house-rules-core.md` in `storytold/craft-contrib`. Propose changes there, not in a copy.

1. **A person is accountable.** Whoever runs an agent answers for what it opens and for its review comments.
2. **Check, then claim.** Before writing code, read the issue comments, search pull requests for `#N` and the keywords, and check recent commits on `main`. Comment on the issue first. Release a claim you will not finish.
3. **Verify, don't guess.** Open a definition before using it. Confirm the cause in the code, even when someone names a location.
4. **Keep fixes narrow and tested.** Fix what the issue describes, where the bug is. Add a test that fails without the fix.
5. **Review the diff before opening a pull request.** Identifiers exist as used, the test fails without the fix, nothing unrelated changed, and the description is true.
6. **Look for a better pull request.** If another fixes the same thing, close yours when it is better or equal, and say why.
7. **Report only what ran.** Say what you tested and what you could not run. Without a local build or test, say so and rely on CI before marking ready.
8. **`main` changes through a pull request.** Do not push to it. Branch protection enforces this. There is no exception.
9. **Stop and ask only about** licensing outside the allowlist, publishing, releases, a new remote, secrets, spending money, destructive git, and anything else hard to undo. Otherwise keep working. `plan/` is a gitignored notebook, not rules. Without it, do not ask for it and do not stop.
10. **Keep secrets and personal details out** of the repository, logs and requests.
11. **CI stays shared.** Workflows call `storytold/craft-common-actions`. Do not change CI in a feature or fix pull request. If CI cannot cover a change, open an issue there.
12. **Guards say why.** A check that looks removable says what it prevents. Read that before changing it.
13. **Instructions must be readable by everyone.** No notes about one machine, and no links to files a reader cannot open. `craftrules` is retired. Do not link to it.
14. **Keep pull requests moving.** Concurrent pull requests are fine; unanswered actionable feedback is not. Respond in 7 days, a reminder at 14, closure eligible at 30, from when an actionable request waits on the author. Suggestions and unconfirmed bot findings do not start the clock. A fix, explanation or agreed plan resets it; unrelated activity does not. Waiting on a reviewer or maintainer pauses it and does not count against the author. A maintainer decides closure; it is never automatic. Reuse search results, do not poll, and stop at a rate limit.
15. **No agent attribution.** A pull request does not say an agent wrote it. A repository rule may not require that.
<!-- craft-house-rules:end -->
<!-- craft-house-rules:start rust source=dde09ea31e2e457f5b4653a6e76f7cdf55944bdf sha256=387e0ccb4b49e567f60951d7d6fff7305054c89f43ad812b9993dc7c38528013 -->
## Rust Craft repositories

These apply to native Craft apps and to shared Rust libraries. Select them by repository type. A repository that merely contains some Rust does not take this block. Rules later in a repository's file may be stricter, but not weaker. The source is `standards/house-rules-rust.md` in [storytold/craft-contrib](https://github.com/storytold/craft-contrib).

1. **Never crash.** Non-test code does not panic and has no `unsafe`, except where the repository file names an exception. Return errors, treat input as hostile, and bound recursion and allocations. New production crate roots copy the `deny` attribute from an existing crate: `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::unimplemented`, `clippy::todo`, `clippy::unreachable`. Every crash fix comes with a test that panicked before the fix.
2. **Clean room.** Learn from observed behaviour and public specifications. Do not copy another product's code, assets, strings or artwork. Check a dependency's licence before adding it.
3. **Assets and fonts.** Every asset is original or suitably licensed, and is recorded in `ATTRIBUTION.md` in the same change. Font files live in [storytold/craft-fonts](https://github.com/storytold/craft-fonts), not in the app repository.
4. **Rust only for native Craft apps.** No handwritten JavaScript or TypeScript, and no Tauri, Electron or webview shell, in those apps. Shared tooling follows its documented technical scope and maintainer-approved exceptions. Craft PR Review's existing Tauri and TypeScript UI is one such exception. It does not relax this rule for native apps. Preserve any existing web build.
5. **Time goes through `chrono`.** Do not hand-roll calendar conversion. For dates beyond what `chrono` can represent, follow [standards/time.md](https://github.com/storytold/craft-contrib/blob/main/standards/time.md).
6. **Contributor credits.** Add or edit only your own person's entry in `contributors/people.toml`, and only after asking them. Never guess a name or hand-edit `contributors.json`.

## Licence allowlist

Dependencies and assets use SPDX identifiers. An expression is allowed when every identifier is in this list. A repository may name a shorter list. It may not add an identifier here by editing its own copy of these rules.

- `ISC`
- `MIT`
- `BSD-2-Clause`
- `BSD-3-Clause`
- `Apache-2.0`
- `OFL-1.1`
- `Ubuntu-font-1.0`
- `Bitstream-Vera`
- `CC0-1.0`
- `CC-BY-4.0`
- `CC-BY-SA-4.0`
- `LicenseRef-Public-Domain`

`LicenseRef-Storyteller-Trademark` is not in this list. It covers Storyteller brand marks only, where a repository already says so.
<!-- craft-house-rules:end -->

# NoteCraft — instructions for agents

NoteCraft is a community "Craft" app by Ideademic (github.com/Ideademic/NoteCraft, MIT OR Apache-2.0): a clean-room, pure-Rust/egui notebook in the spirit of OneNote 2016/2019 for Windows desktop, built to fit in with ArtCraft's Craft apps (WordCraft, DeckCraft, GridCraft…; same conventions). It runs natively on macOS, Windows, Linux and BSD, and on the web via WASM. Storage is local: **`.one` is the native format** (a notebook is a folder with `Open Notebook.onetoc2` plus one `.one` per section; section groups are subfolders), with `.onepkg` import/export. There is no Save button: changes autosave.

`plan/` is an optional gitignored notebook. Do not ask for it and do not stop when it is absent.

## Never crash
A crash loses notes. **This outranks feature work.**
- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`; no `unsafe` (`unsafe_code = "forbid"`). An unfinished feature returns an error (`NotImplemented`) or is disabled.
- **Input is hostile** (files, command params): `get()` instead of `[i]`, slice strings only at char boundaries, checked/saturating arithmetic, clamp sizes, cap allocations and counts, bound recursion (`MAX_GROUP_DEPTH`, `MAX_POINTS`, `MAX_INK_PER_PAGE`…).
- **Last-resort guard:** `Session::run` turns an escaped panic into an error and restores the notebooks. It's a safety net, not a licence. `crates/engine/src/tests.rs::hostile_params_never_panic` runs every command with junk params; keep it green.
- Production crate roots start with `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`. Tests may `unwrap` (`clippy.toml`).

## Assets: no Microsoft iconography or images
**Non-negotiable.** Never copy, trace, screenshot-crop or recreate icons, images, templates, fonts, sounds or any other asset from OneNote, Office or any Microsoft product, not even temporarily. Icons are drawn in code (`crates/ui-egui/src/icons/`, mostly vendored from DeckCraft). Every asset file has a row in [`ATTRIBUTION.md`](ATTRIBUTION.md) (author, source, licence); `cargo xtask assets` fails otherwise. Proprietary fonts are never bundled.

## Clean room
- The `.one` / `.onetoc2` / `.onepkg` readers and writers are written **only** from Microsoft's public Open Specifications: [MS-ONESTORE] (revision store), [MS-ONE] (OneNote file structure), [MS-DTYP], [MS-OSHARED] (MsoCrc32), [MS-CAB] / the Cabinet format, and observed black-box behaviour.
- **Never read, copy or port the source** of other OneNote implementations: `onenote_parser` / `onenote.rs`, `libmson`, `pyOneNote`, `OfficeIMO`, Apache Tika's or LibreOffice's OneNote code, or any other. Not to "check" a detail either. (Using Apache Tika's Apache-2.0 sample `.one` files as gitignored test inputs is fine; they are data, not code.)
- Never copy Microsoft wording beyond feature names.

## Licences
Dependencies must be within the allowlist above, and NoteCraft narrows it: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, OFL-1.1, CC0-1.0 (plus Unicode-3.0, Zlib and BSL-1.0 only where WordCraft's lock already carries them transitively). **No MPL, GPL, LGPL or AGPL** (so no `onenote_parser`). Every dependency is declared once in the root `[workspace.dependencies]`; check its licence before adding it.

## Architecture
| Layer | Crate (`notecraft-*`) | Job |
|---|---|---|
| L0 | `geom` | points, half-inches (MS-ONE layout, 36 pt), ink units (0.01 mm), zoom, `Point`/`Size`/`Rect` |
| L1 | `doc` | the model: `Notebook` ▸ `SectionEntry` (`Section` / `SectionGroup`) ▸ `Page` ▸ `PageElement`s + page-level `InkStroke`s; `Guid`, `Color` |
| L2 | `onestore` | MS-ONESTORE container read/write (header, file node lists, property sets, transaction log, CRCs) |
| L2 | `one` | MS-ONE mapping store ↔ model; `.onetoc2`; notebook folders (`onestore` → `one` → `onepkg` may depend on each other in that order) |
| L2 | `onepkg` | `.onepkg` (Cabinet) via `cab` |
| L2 | `formats` | HTML / Markdown / text export; picture import |
| L3 | `printout` | PDF → page pictures (craft-pdf `raster`) for File Printout |
| L4 | `engine` | `Session`, command registry, snapshot undo, dirty tracking, I/O dispatch, `Storage`, autosave |
| L6 | `ui-egui` | the egui front end (swappable) |
| apps | `apps/notecraft`, `apps/notecraft-cli`, `apps/notecraft-web` | binaries |

- **Layering** is enforced by `cargo xtask layers`: a crate depends only on lower layers; nothing below L6 depends on egui / eframe / winit / rfd.
- **Everything is a command.** User-visible behaviour is a `CommandSpec` in `crates/engine/src/cmd/*.rs` (id from `ids.rs`, label, shortcut, params doc, `mutates`, `enabled`, `run`) plus tests. The ribbon, shortcuts, CLI and tests dispatch by id through `Session::run`. Programmatic calls never open dialogs.
- **Model conventions:** positions are `f32` points from the page's top-left corner, y down. Ink lives at page level (`Page::ink`), free-floating like OneNote's, not inside outlines. Binary payloads are `Arc<Vec<u8>>` (cheap undo snapshots; base64 in JSON).
- The UI is thin: it reads `Session` and changes it only through commands. Colours come from `theme::Tokens` (`APP_COLOR = #c44df0`; flat, opaque, corners of 2 px). The shell and the canvas meet only at `canvas::CanvasState` and `canvas::show`.
- **Rust only** (no handwritten JS/TS). **Never break wasm**: native-only code is `#[cfg(not(target_arch = "wasm32"))]`; time comes from `notecraft_doc::now()` (web-time), not `SystemTime::now()`.

## Quality gates
Before every commit: `cargo xtask ci` (fmt, clippy `-D warnings`, tests, assets, layers, wasm). Without the wasm32 target installed `cargo xtask wasm` warns and skips locally; CI (`CI` set) fails instead.

## Working in parallel
Edit only the crates you own; add dependencies to the root `[workspace.dependencies]` only when unavoidable, and say so. Use a separate `CARGO_TARGET_DIR` per agent and delete it when done.

## CI workflows
The workflows in `.github/` are copied from `storytold/craft-common-actions` and adapted for a community repository (free runners, unsigned builds). Don't change CI in a feature or fix PR.
