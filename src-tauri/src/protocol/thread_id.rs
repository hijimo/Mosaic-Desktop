use serde::{Deserialize, Serialize};
use std::fmt::Display;
use uuid::Uuid;

/// Unique identifier for a conversation thread, wrapping a UUID v7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThreadId {
    uuid: Uuid,
}

impl ThreadId {
    pub fn new() -> Self {
        Self {
            uuid: Uuid::now_v7(),
        }
    }

    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self {
            uuid: Uuid::parse_str(s)?,
        })
    }

    /// Access the underlying UUID (e.g. for timestamp extraction).
    pub fn as_uuid(&self) -> Uuid {
        self.uuid
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.uuid, f)
    }
}

impl TryFrom<&str> for ThreadId {
    type Error = uuid::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_string(value)
    }
}

impl TryFrom<String> for ThreadId {
    type Error = uuid::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_string(&value)
    }
}

impl From<ThreadId> for String {
    fn from(value: ThreadId) -> Self {
        value.to_string()
    }
}

impl Serialize for ThreadId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(&self.uuid)
    }
}

impl<'de> Deserialize<'de> for ThreadId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let uuid = Uuid::parse_str(&value).map_err(serde::de::Error::custom)?;
        Ok(Self { uuid })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_not_nil() {
        let id = ThreadId::default();
        assert_ne!(id.uuid, Uuid::nil());
    }

    #[test]
    fn roundtrip_string() {
        let id = ThreadId::new();
        let s = id.to_string();
        let parsed = ThreadId::try_from(s.as_str()).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn serde_roundtrip() {
        let id = ThreadId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: ThreadId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }
}
