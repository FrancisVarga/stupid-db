use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Wire-format message envelope for inter-component communication.
///
/// Messages are serialized with MessagePack for compact, fast transport.
/// The `topic` field is used by PUB/SUB routing, while `correlation_id`
/// enables request-response tracking and distributed tracing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Routing topic for PUB/SUB filtering (e.g. "entity.created", "anomaly.detected").
    pub topic: String,

    /// MessagePack-encoded payload bytes.
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,

    /// When this message was created.
    pub timestamp: DateTime<Utc>,

    /// Correlation ID for request-response tracking and distributed tracing.
    pub correlation_id: Uuid,

    /// Schema version for forward-compatible evolution.
    /// Consumers should check this before deserializing the payload.
    #[serde(default = "default_version")]
    pub version: u16,
}

/// Default version for messages that omit the field (backward compat).
fn default_version() -> u16 {
    1
}

impl Message {
    /// Create a new message, serializing the payload with MessagePack.
    pub fn new<T: Serialize>(
        topic: impl Into<String>,
        payload: &T,
    ) -> Result<Self, rmp_serde::encode::Error> {
        Ok(Self {
            topic: topic.into(),
            payload: rmp_serde::to_vec(payload)?,
            timestamp: Utc::now(),
            correlation_id: Uuid::new_v4(),
            version: 1,
        })
    }

    /// Create a message with an explicit correlation ID (for replies/continuations).
    pub fn with_correlation<T: Serialize>(
        topic: impl Into<String>,
        payload: &T,
        correlation_id: Uuid,
    ) -> Result<Self, rmp_serde::encode::Error> {
        Ok(Self {
            topic: topic.into(),
            payload: rmp_serde::to_vec(payload)?,
            timestamp: Utc::now(),
            correlation_id,
            version: 1,
        })
    }

    /// Deserialize the payload into the expected type.
    pub fn decode<T: for<'de> Deserialize<'de>>(&self) -> Result<T, rmp_serde::decode::Error> {
        rmp_serde::from_slice(&self.payload)
    }

    /// Serialize this entire message envelope to MessagePack bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec(self)
    }

    /// Deserialize a message envelope from MessagePack bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(bytes)
    }
}

/// Helper module for serde to handle `Vec<u8>` as raw bytes in MessagePack.
mod serde_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let bytes: &[u8] = Deserialize::deserialize(d)?;
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_message() {
        let payload = "hello world".to_string();
        let msg = Message::new("test.topic", &payload).unwrap();

        assert_eq!(msg.topic, "test.topic");
        assert_eq!(msg.decode::<String>().unwrap(), "hello world");
    }

    #[test]
    fn roundtrip_envelope_bytes() {
        let msg = Message::new("events.entity", &42u64).unwrap();
        let bytes = msg.to_bytes().unwrap();
        let decoded = Message::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.topic, "events.entity");
        assert_eq!(decoded.correlation_id, msg.correlation_id);
        assert_eq!(decoded.decode::<u64>().unwrap(), 42);
    }

    #[test]
    fn with_correlation_preserves_id() {
        let id = Uuid::new_v4();
        let msg = Message::with_correlation("reply", &true, id).unwrap();
        assert_eq!(msg.correlation_id, id);
    }
}
