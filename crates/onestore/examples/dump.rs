//! Prints the object spaces, objects and properties of a `.one` / `.onetoc2` file: a
//! development aid for black-box inspection of sample files.
//!
//! `cargo run -p notecraft-onestore --example dump -- file.one [--all]`

use notecraft_onestore::{PropertySet, PropertyValue};

fn show(set: &PropertySet, indent: usize, out: &mut String) {
    for (id, v) in &set.props {
        let pad = " ".repeat(indent);
        let val = match v {
            PropertyValue::Bytes(b)
                if b.len() >= 2 && b.len().is_multiple_of(2) && b.iter().skip(1).step_by(2).filter(|&&x| x == 0).count() * 2 >= b.len() / 2 =>
            {
                format!("bytes[{}] {:?} utf16={:?}", b.len(), &b[..b.len().min(24)], notecraft_onestore::bytes::from_utf16le(&b[..b.len().min(200)]))
            }
            PropertyValue::Bytes(b) => format!("bytes[{}] {:02x?}", b.len(), &b[..b.len().min(48)]),
            PropertyValue::U32(x) => format!("u32 {x:#010x} ({x}, f32 {})", f32::from_bits(*x)),
            PropertyValue::Sets(sets) => {
                out.push_str(&format!("{pad}{:#010x} sets[{}]\n", id.0, sets.len()));
                for s in sets {
                    out.push_str(&format!("{pad}  {{\n"));
                    show(s, indent + 4, out);
                    out.push_str(&format!("{pad}  }}\n"));
                }
                continue;
            }
            other => format!("{other:?}"),
        };
        out.push_str(&format!("{pad}{:#010x} {val}\n", id.0));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: dump <file> [--all]");
        return;
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    match notecraft_onestore::read(&bytes) {
        Err(e) => println!("error: {e}"),
        Ok(store) => {
            println!("kind {:?} file {} ancestor {} root {:?} warnings {:?}", store.kind, store.file_id, store.ancestor, store.root, store.warnings);
            for sp in &store.spaces {
                println!("== space {} n={} roots {:?}", sp.id.to_guid(), sp.id.n, sp.roots);
                for o in &sp.objects {
                    let mut s = String::new();
                    show(&o.props, 4, &mut s);
                    let fd = o.file_data.as_ref().map(|d| format!(" file[{}] {:?}", d.len(), o.file_ext)).unwrap_or_default();
                    println!("  obj {}/{} jcid {:#010x}{fd}\n{s}", o.id.to_guid(), o.id.n, o.jcid);
                }
            }
        }
    }
}
