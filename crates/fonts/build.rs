//! The bundled fonts: an optional craft-fonts build input (https://github.com/storytold/craft-fonts,
//! the recipe of its `docs/integration.md`, opting in to `fonts/latin-manifest.txt`).
//!
//! With `CRAFT_FONTS_DIR=<checkout>` the metric-compatible faces NoteCraft substitutes for missing
//! Office fonts are embedded as `CRAFT_FONTS`: Carlito (Calibri), Caladea (Cambria) and Liberation
//! Sans / Serif / Mono (Arial / Times New Roman / Courier New), all OFL-1.1. Unset, `CRAFT_FONTS` is
//! empty and the app uses the installed fonts and egui's own faces. Web (wasm32) builds embed a
//! smaller set ([`WEB_FONTS`]), since the browser has no fonts of its own to offer. With
//! `CRAFT_FONTS_REQUIRED=1` (release builds) a missing checkout or face is a build error. It only
//! reads the local checkout: no network.
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The families embedded on the desktop (every style the manifest lists).
const FAMILIES: &[&str] = &["Carlito", "Caladea", "Liberation Sans", "Liberation Serif", "Liberation Mono"];

/// What a web build keeps: the default font (Calibri's substitute) upright and bold, Caladea (small:
/// about 82 KB a face), and one upright face of the Arial, Times and Courier substitutes:
/// 2,767,524 bytes of font files (about 2.6 MiB).
const WEB_FONTS: &[(&str, &str)] = &[
    ("Carlito", "Regular"),
    ("Carlito", "Bold"),
    ("Caladea", "Regular"),
    ("Caladea", "Bold"),
    ("Caladea", "Italic"),
    ("Caladea", "Bold Italic"),
    ("Liberation Sans", "Regular"),
    ("Liberation Serif", "Regular"),
    ("Liberation Mono", "Regular"),
];

const MANIFEST: &str = "fonts/latin-manifest.txt";

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_REQUIRED");
    let mut src = String::from("pub static CRAFT_FONTS: &[CraftFont] = &[\n");
    let required = std::env::var_os("CRAFT_FONTS_REQUIRED").is_some_and(|v| v == "1");
    match std::env::var_os("CRAFT_FONTS_DIR").filter(|d| !d.is_empty()).map(PathBuf::from) {
        Some(dir) => match craft_fonts(&dir, required) {
            Ok(entries) => src.push_str(&entries),
            Err(e) if required => println!("cargo::error=CRAFT_FONTS_DIR={}: {e}", dir.display()),
            Err(e) => println!("cargo::warning=building without craft-fonts: CRAFT_FONTS_DIR={}: {e}", dir.display()),
        },
        None if required => println!("cargo::error=CRAFT_FONTS_REQUIRED is set but CRAFT_FONTS_DIR is missing"),
        None => {}
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("craft_fonts.rs");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// One `CraftFont { .. }` initialiser per selected manifest line.
fn craft_fonts(dir: &Path, required: bool) -> Result<String, String> {
    let manifest = dir.join(MANIFEST);
    println!("cargo::rerun-if-changed={}", manifest.display());
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    let mut out = String::new();
    let mut included: Vec<(String, String)> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(" | ").map(str::trim).collect();
        let [family, style, file, scripts, ..] = f.as_slice() else {
            return Err(format!("malformed manifest line: {line}"));
        };
        let wanted = if wasm { WEB_FONTS.contains(&(*family, *style)) } else { FAMILIES.contains(family) };
        if !wanted {
            continue;
        }
        let path = dir.join(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
        println!("cargo::rerun-if-changed={}", path.display());
        included.push(((*family).to_owned(), (*style).to_owned()));
        let scripts: Vec<String> = scripts.split(',').map(|s| format!("{:?}", s.trim())).collect();
        let _ = writeln!(
            out,
            "    CraftFont {{ family: {family:?}, style: {style:?}, scripts: &[{}], bytes: include_bytes!({:?}) }},",
            scripts.join(", "),
            path.display().to_string(),
        );
    }
    if required {
        let missing: Vec<String> = if wasm {
            WEB_FONTS.iter().filter(|(f, s)| !included.iter().any(|(a, b)| a == f && b == s)).map(|(f, s)| format!("{f} {s}")).collect()
        } else {
            FAMILIES.iter().filter(|f| !included.iter().any(|(a, _)| a == *f)).map(|f| (*f).to_owned()).collect()
        };
        if !missing.is_empty() {
            return Err(format!("{MANIFEST} has no {}", missing.join(", ")));
        }
    }
    Ok(out)
}
