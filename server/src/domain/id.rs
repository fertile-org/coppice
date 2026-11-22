//! Entity id newtype: UUID v4 strings or 12-char `[0-9a-z]` ids, stored as VARCHAR(36).
//!
//! Backed by a fixed buffer so `Id` stays `Copy` like the former `Uuid` fields
//! (serde/sqlx still see a string).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::postgres::{PgArgumentBuffer, PgTypeInfo, PgValueRef};
use sqlx::{Decode, Encode, Postgres, Type};
use uuid::Uuid;

/// Stable entity identifier. Always lowercase when generated or normalized from a UUID.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id {
    buf: [u8; 36],
    len: u8,
}

/// Why an id string was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidId;

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid id")
    }
}

impl std::error::Error for InvalidId {}

impl Id {
    /// Fresh UUID v4, lowercase.
    pub fn new() -> Self {
        Self::from_ascii(Uuid::new_v4().to_string().as_bytes())
    }

    /// All-zero UUID string (test / placeholder sentinel).
    pub fn nil() -> Self {
        Self::from_ascii(Uuid::nil().to_string().as_bytes())
    }

    /// Deterministic UUID string from a u128 (tests / const fixtures).
    pub const fn from_u128(value: u128) -> Self {
        let u = Uuid::from_u128(value);
        let bytes = u.as_bytes();
        let hex = b"0123456789abcdef";
        let mut out = [0u8; 36];
        let mut i = 0usize;
        let mut o = 0usize;
        while i < 16 {
            if o == 8 || o == 13 || o == 18 || o == 23 {
                out[o] = b'-';
                o += 1;
            }
            out[o] = hex[(bytes[i] >> 4) as usize];
            out[o + 1] = hex[(bytes[i] & 0xf) as usize];
            o += 2;
            i += 1;
        }
        Self { buf: out, len: 36 }
    }

    /// Parse path / JSON / query input. Same acceptance as [`FromStr`].
    pub fn parse(s: &str) -> Result<Self, InvalidId> {
        s.parse()
    }

    /// Uuid-compatible alias used widely in existing call sites.
    pub fn parse_str(s: &str) -> Result<Self, InvalidId> {
        Self::parse(s)
    }

    pub fn as_str(&self) -> &str {
        // Validated ASCII on construction.
        std::str::from_utf8(&self.buf[..self.len as usize]).unwrap_or("")
    }

    pub fn into_string(self) -> String {
        self.as_str().to_string()
    }

    /// UUID without hyphens (matches `Uuid::simple()`). Short ids are unchanged.
    pub fn simple(&self) -> String {
        self.as_str().chars().filter(|c| *c != '-').collect()
    }

    /// UUID bits when the id is UUID-shaped; `0` for short ids.
    pub fn as_u128(&self) -> u128 {
        Uuid::parse_str(self.as_str())
            .map(|u| u.as_u128())
            .unwrap_or(0)
    }

    const fn from_ascii_const(bytes: &[u8], len: usize) -> Self {
        let mut buf = [0u8; 36];
        let mut i = 0;
        while i < len {
            buf[i] = bytes[i];
            i += 1;
        }
        Self {
            buf,
            len: len as u8,
        }
    }

    fn from_ascii(bytes: &[u8]) -> Self {
        debug_assert!(bytes.len() <= 36);
        Self::from_ascii_const(bytes, bytes.len())
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::nil()
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for Id {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl FromStr for Id {
    type Err = InvalidId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if is_uuid_str(s) {
            let lower = s.to_ascii_lowercase();
            return Ok(Self::from_ascii(lower.as_bytes()));
        }
        if is_short_id(s) {
            return Ok(Self::from_ascii(s.as_bytes()));
        }
        Err(InvalidId)
    }
}

impl TryFrom<String> for Id {
    type Error = InvalidId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<&str> for Id {
    type Error = InvalidId;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Type<Postgres> for Id {
    fn type_info() -> PgTypeInfo {
        // Match VARCHAR(36) columns so array ops (`&&`, `ANY`) type-check.
        PgTypeInfo::with_name("VARCHAR")
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        *ty == PgTypeInfo::with_name("VARCHAR")
            || *ty == PgTypeInfo::with_name("TEXT")
            || <String as Type<Postgres>>::compatible(ty)
    }
}

impl Encode<'_, Postgres> for Id {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.as_str(), buf)
    }
}

impl Decode<'_, Postgres> for Id {
    fn decode(value: PgValueRef<'_>) -> Result<Self, BoxDynError> {
        let s = <String as Decode<Postgres>>::decode(value)?;
        // Trust rows already in the DB; CHECK enforces shape on write for PKs.
        if s.len() > 36 {
            return Err("id longer than 36 bytes".into());
        }
        Ok(Self::from_ascii(s.as_bytes()))
    }
}

impl sqlx::postgres::PgHasArrayType for Id {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_VARCHAR")
    }

    fn array_compatible(ty: &PgTypeInfo) -> bool {
        *ty == PgTypeInfo::with_name("_VARCHAR")
            || *ty == PgTypeInfo::with_name("_TEXT")
            || <String as sqlx::postgres::PgHasArrayType>::array_compatible(ty)
    }
}

fn is_uuid_str(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let b = s.as_bytes();
    let is_hex = |c: u8| c.is_ascii_hexdigit();
    for (i, &c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if c != b'-' {
                    return false;
                }
            }
            _ => {
                if !is_hex(c) {
                    return false;
                }
            }
        }
    }
    true
}

fn is_short_id(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'z'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_lowercase_uuid() {
        let id = Id::new();
        assert_eq!(id.as_str().len(), 36);
        assert!(is_uuid_str(id.as_str()));
        assert_eq!(id.as_str(), id.as_str().to_ascii_lowercase());
    }

    #[test]
    fn parses_uuid_and_short() {
        let u = Id::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(u.as_str(), "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(u.simple(), "550e8400e29b41d4a716446655440000");
        let upper = Id::parse("550E8400-E29B-41D4-A716-446655440000").unwrap();
        assert_eq!(upper.as_str(), "550e8400-e29b-41d4-a716-446655440000");
        let short = Id::parse("abc123xyz789").unwrap();
        assert_eq!(short.as_str(), "abc123xyz789");
        assert_eq!(short.simple(), "abc123xyz789");
    }

    #[test]
    fn rejects_bad_ids() {
        assert!(Id::parse("").is_err());
        assert!(Id::parse("not-an-id").is_err());
        assert!(Id::parse("ABC123XYZ789").is_err()); // short must be lowercase
        assert!(Id::parse("550e8400-e29b-41d4-a716-44665544000").is_err());
    }
}
