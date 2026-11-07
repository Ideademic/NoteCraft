//! `#[serde(with = "crate::b64")]` for binary payloads: base64 text in JSON instead of an array
//! of numbers.

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(v: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&STANDARD.encode(v.as_slice()))
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
    let s = String::deserialize(d)?;
    STANDARD.decode(s.as_bytes()).map(Arc::new).map_err(serde::de::Error::custom)
}
