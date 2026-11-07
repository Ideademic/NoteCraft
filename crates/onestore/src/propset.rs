//! Property sets ([MS-ONESTORE] §2.6): the typed values objects carry; ids from [MS-ONE] §2.1.

/// A property id: the 26-bit id plus the type bits (§2.6.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PropertyId(pub u32);

/// A property value.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    /// No data (`0x1`), or a bool (`0x2`).
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    /// Length-prefixed bytes (`0x7`).
    Bytes(Vec<u8>),
    /// Object / object space / context id references (`0x8`–`0xD`), resolved.
    Ids(Vec<crate::ExtendedGuid>),
    /// A nested property set (`0x11`) or an array of them (`0x10`).
    Sets(Vec<PropertySet>),
}

/// An ordered list of properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertySet {
    pub props: Vec<(PropertyId, PropertyValue)>,
}

impl PropertySet {
    /// The first value with this id (type bits ignored).
    pub fn get(&self, id: u32) -> Option<&PropertyValue> {
        self.props.iter().find(|(p, _)| p.0 & 0x03FF_FFFF == id & 0x03FF_FFFF).map(|(_, v)| v)
    }
}
