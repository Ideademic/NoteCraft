//! Prints what NoteCraft reads from a `.one` file: pages, elements and ink (development aid).
//!
//! `cargo run -p notecraft-one --example summary -- file.one`

use notecraft_doc::ElementKind;

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        match notecraft_one::read_section(&bytes) {
            Err(e) => println!("{path}: {e}"),
            Ok(s) => {
                println!("{path}: section {:?} colour {:?} unsupported {}", s.name, s.color, s.source.unsupported_content);
                for p in &s.pages {
                    println!("  page {:?} level {} created {} ink {}", p.title, p.level, p.created, p.ink.len());
                    for e in &p.elements {
                        let what = match &e.kind {
                            ElementKind::Outline(o) => {
                                format!("outline at {:?} w {} : {:?}", o.pos, o.width, o.text().chars().take(60).collect::<String>())
                            }
                            ElementKind::Image(i) => format!("image {:?} {:?} {} bytes", i.pos, i.size, i.data.len()),
                            ElementKind::EmbeddedFile { name, bytes, pos } => format!("file {name} {} bytes at {pos:?}", bytes.len()),
                            ElementKind::Printout(pr) => format!("printout {} {} pages", pr.file_name, pr.pages.len()),
                            ElementKind::Opaque { jcid, data } => format!("opaque {jcid:#010x} {} bytes", data.len()),
                        };
                        println!("    {what}");
                    }
                    for s in p.ink.iter().take(3) {
                        println!("    ink {:?} w {} {} pts first {:?}", s.color, s.width, s.points.len(), s.points.first());
                    }
                }
            }
        }
    }
}
