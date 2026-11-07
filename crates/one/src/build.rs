//! Building object spaces for the writer.

use notecraft_doc::Guid;
use notecraft_onestore::{ExtendedGuid, Object, ObjectSpace, PropertySet};

/// Most objects sharing one GUID: CompactIDs keep only 8 bits of `n`.
const IDS_PER_GUID: u32 = 255;

/// An object space being written: objects get ids from a GUID with a counter (like OneNote's
/// own files), a fresh GUID every 255 objects.
pub struct SpaceBuilder {
    pub id: ExtendedGuid,
    pub roots: Vec<(u32, ExtendedGuid)>,
    pub objects: Vec<Object>,
    guid: [u8; 16],
    next: u32,
}

impl SpaceBuilder {
    pub fn new(id: ExtendedGuid) -> SpaceBuilder {
        SpaceBuilder { id, roots: Vec::new(), objects: Vec::new(), guid: Guid::new().0, next: 1 }
    }

    /// A fresh object id.
    pub fn new_id(&mut self) -> ExtendedGuid {
        if self.next > IDS_PER_GUID {
            self.guid = Guid::new().0;
            self.next = 1;
        }
        let id = ExtendedGuid::new(self.guid, self.next);
        self.next += 1;
        id
    }

    /// Adds a property-set object with a fresh id.
    pub fn add(&mut self, jcid: u32, props: PropertySet) -> ExtendedGuid {
        let id = self.new_id();
        self.objects.push(Object::new(id, jcid, props));
        id
    }

    /// Adds a property-set object with a given id.
    pub fn add_as(&mut self, id: ExtendedGuid, jcid: u32, props: PropertySet) -> ExtendedGuid {
        self.objects.push(Object::new(id, jcid, props));
        id
    }

    /// Adds a file data object.
    pub fn add_file(&mut self, jcid: u32, data: std::sync::Arc<Vec<u8>>, ext: &str) -> ExtendedGuid {
        let id = self.new_id();
        self.objects.push(Object::file(id, jcid, data, ext));
        id
    }

    pub fn root(&mut self, role: u32, id: ExtendedGuid) {
        self.roots.push((role, id));
    }

    pub fn finish(self) -> ObjectSpace {
        ObjectSpace { id: self.id, roots: self.roots, objects: self.objects }
    }
}

/// The object id NoteCraft writes for a model element: its GUID with `n` 1, so reading it back
/// gives the same id (see [`element_id`]).
pub fn element_object_id(id: Guid) -> ExtendedGuid {
    ExtendedGuid::from_guid(if id.is_nil() { Guid::new() } else { id }, 1)
}

/// The model id for an object: its GUID when `n` is 1, else the GUID mixed with `n` (OneNote's
/// files give many objects one GUID and different counters).
pub fn element_id(id: ExtendedGuid) -> Guid {
    if id.n == 1 {
        return Guid(id.guid);
    }
    let mut g = id.guid;
    for (b, n) in g.iter_mut().skip(12).zip(id.n.to_le_bytes()) {
        *b ^= n;
    }
    // Keep it distinct from an object with the same GUID and n = 1.
    g[11] ^= 0x5A;
    Guid(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_wrap_to_a_new_guid() {
        let mut b = SpaceBuilder::new(ExtendedGuid::from_guid(Guid::new(), 1));
        let first = b.new_id();
        for _ in 0..300 {
            let id = b.new_id();
            assert!(id.n >= 1 && id.n <= 255);
        }
        assert_ne!(b.new_id().guid, first.guid);
    }

    #[test]
    fn element_ids_round_trip() {
        let g = Guid::new();
        assert_eq!(element_id(element_object_id(g)), g);
        let a = element_id(ExtendedGuid::new(g.0, 2));
        let c = element_id(ExtendedGuid::new(g.0, 3));
        assert!(a != c && a != g);
    }
}
