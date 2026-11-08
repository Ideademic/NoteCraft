use std::sync::Arc;

use notecraft_doc::Guid;

use super::*;

fn eg(n: u32) -> ExtendedGuid {
    ExtendedGuid::new([0xAB; 16], n)
}

fn section_store() -> Store {
    let page_space = ExtendedGuid::from_guid(Guid::new(), 1);
    let mut root_props = PropertySet::default();
    root_props.push(0x24001C20, PropertyValue::Ids(vec![eg(3)])).push(0x1C001C30, PropertyValue::Bytes(vec![1; 16]));
    let mut series = PropertySet::default();
    series.push(0x2C001D63, PropertyValue::Ids(vec![page_space]));
    let mut style = PropertySet::default();
    style.push(0x1C00345A, PropertyValue::Bytes(bytes::utf16le("p")));
    let mut pic = PropertySet::default();
    pic.push(0x20001C3F, PropertyValue::Ids(vec![eg(6)]));
    let root = ExtendedGuid::from_guid(Guid::new(), 1);
    Store {
        kind: FileKind::Section,
        file_id: Guid::new(),
        ancestor: Guid::new(),
        file_name: "Quick Notes.one".into(),
        root,
        spaces: vec![
            ObjectSpace {
                id: root,
                roots: vec![(1, eg(1)), (2, eg(2))],
                objects: vec![
                    Object::new(eg(1), 0x0006_0007, root_props),
                    Object::new(eg(2), 0x0002_0031, PropertySet::default()),
                    Object::new(eg(3), 0x0006_0008, series),
                ],
                ..ObjectSpace::default()
            },
            ObjectSpace {
                id: page_space,
                roots: vec![(1, eg(4))],
                objects: vec![
                    Object::new(eg(4), 0x0006_0011, pic),
                    Object::new(eg(5), 0x0012_004D, style),
                    Object::file(eg(6), 0x0008_0039, Arc::new(b"\x89PNG\r\n\x1a\nfake".to_vec()), ".png"),
                ],
                ..ObjectSpace::default()
            },
        ],
        warnings: Vec::new(),
    }
}

fn toc_store() -> Store {
    let root = ExtendedGuid::from_guid(Guid::new(), 1);
    let mut entry = PropertySet::default();
    entry.push(0x1C001D94, PropertyValue::Bytes(vec![7; 16]));
    let mut toc = PropertySet::default();
    toc.push(0x24001C20, PropertyValue::Ids(vec![eg(2), eg(3)]));
    Store {
        kind: FileKind::Toc,
        file_id: Guid::new(),
        ancestor: Guid::NIL,
        file_name: "Open Notebook.onetoc2".into(),
        root,
        spaces: vec![ObjectSpace {
            id: root,
            roots: vec![(1, eg(1))],
            objects: vec![
                Object::new(eg(1), 0x0002_0001, toc),
                Object::new(eg(2), 0x0002_0001, entry.clone()),
                Object::new(eg(3), 0x0002_0001, entry),
            ],
            ..ObjectSpace::default()
        }],
        warnings: Vec::new(),
    }
}

/// What a read gives back for a written store (names aren't stored, only their CRC).
fn normalized(mut s: Store) -> Store {
    s.file_name.clear();
    s
}

#[test]
fn section_round_trip() {
    let store = section_store();
    let bytes = write(&store).unwrap();
    let header = Header::parse(&bytes).unwrap();
    assert_eq!(header.expected_length, bytes.len() as u64);
    assert_eq!(header.crc_name, crc::name_crc("Quick Notes.one"));
    assert_eq!(header.ancestor, store.ancestor.0);
    assert!(header.fcr_free_chunk_list.nil);
    assert_eq!(header.transactions, 1);
    let back = read(&bytes).unwrap();
    assert_eq!(back, normalized(store));
}

/// A section store whose page space has revisions in two more contexts (a version history), one
/// with a file data object shared with the default revision.
fn store_with_contexts() -> Store {
    let mut store = section_store();
    let page = &mut store.spaces[1];
    let shared = page.objects.iter().find_map(|o| o.file_data.clone()).unwrap();
    let mut a = PropertySet::default();
    a.push(0x37FC_0021, PropertyValue::Ids(vec![ExtendedGuid::new([0xCD; 16], 1)])).push(0x18001D77, PropertyValue::U64(42));
    let mut manifest = PropertySet::default();
    manifest.push(0x24001C1F, PropertyValue::Ids(vec![eg(41)]));
    page.contexts = vec![
        ContextRevision { context: ExtendedGuid::new([0x7F; 16], 1), roots: vec![(1, eg(30))], objects: vec![Object::new(eg(30), 0x0006_003D, a)] },
        ContextRevision {
            context: ExtendedGuid::new([0xCD; 16], 1),
            roots: vec![(1, eg(40)), (2, eg(4))],
            objects: vec![
                Object::new(eg(40), 0x0006_0037, manifest),
                Object::new(eg(41), 0x0006_000B, PropertySet::default()),
                Object::new(eg(4), 0x0002_0030, PropertySet::default()),
                Object::file(eg(42), 0x0008_0039, shared, ".png"),
            ],
        },
    ];
    store
}

#[test]
fn context_revisions_round_trip() {
    let store = store_with_contexts();
    let bytes = write(&store).unwrap();
    let back = read(&bytes).unwrap();
    assert_eq!(back, normalized(store.clone()));
    let page = &back.spaces[1];
    assert_eq!(page.contexts.len(), 2);
    assert!(page.context(ExtendedGuid::new([0xCD; 16], 1)).is_some_and(|c| c.root(2) == Some(eg(4))));
    // The default revision is what it was: other contexts don't leak into it.
    assert_eq!(page.objects.len(), 3);
    // Duplicate or nil contexts are refused.
    let mut bad = store.clone();
    bad.spaces[1].contexts[1].context = bad.spaces[1].contexts[0].context;
    assert!(write(&bad).is_err());
    let mut bad = store;
    bad.spaces[1].contexts[0].context = ExtendedGuid::NIL;
    assert!(write(&bad).is_err());
}

#[test]
fn toc_round_trip() {
    let store = toc_store();
    let bytes = write(&store).unwrap();
    assert_eq!(Header::parse(&bytes).unwrap().kind, FileKind::Toc);
    assert_eq!(read(&bytes).unwrap(), normalized(store));
}

#[test]
fn write_refuses_bad_stores() {
    assert!(write(&Store::default()).is_err());
    let mut s = toc_store();
    s.spaces[0].objects.push(Object::file(eg(9), 0x0008_0039, Arc::new(vec![1]), ".png"));
    assert!(write(&s).is_err(), "file data in a .onetoc2");
    let mut s = section_store();
    s.spaces[0].objects[0].id = ExtendedGuid::new([1; 16], 300);
    assert!(write(&s).is_err(), "n too large for a CompactID");
}

#[test]
fn junk_is_an_error_not_a_panic() {
    for input in [&b""[..], b"hello", &[0u8; 2048]] {
        assert!(read(input).is_err());
    }
}

/// A small deterministic generator for the hostile-input tests.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn hostile_input_never_panics() {
    let good = [write(&section_store()).unwrap(), write(&toc_store()).unwrap(), write(&store_with_contexts()).unwrap()];
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    for base in &good {
        // Every truncation.
        for len in (0..base.len()).step_by(7) {
            let _ = read(&base[..len]);
        }
        // Bit flips and byte stomps, keeping the header mostly intact so parsing goes deep.
        for _ in 0..3000 {
            let mut b = base.clone();
            let flips = 1 + (rng.next() % 8) as usize;
            for _ in 0..flips {
                let i = (rng.next() as usize) % b.len();
                if rng.next().is_multiple_of(2) {
                    b[i] ^= 1 << (rng.next() % 8);
                } else {
                    b[i] = rng.next() as u8;
                }
            }
            let _ = read(&b);
        }
    }
    // Random bytes behind a valid header.
    for _ in 0..500 {
        let mut b = good[0][..header::HEADER_SIZE].to_vec();
        let n = (rng.next() % 4096) as usize;
        b.extend((0..n).map(|_| rng.next() as u8));
        let _ = read(&b);
    }
}
