//! Generated protobuf types (`wkyt.delta.v1`) and lossless conversions
//! to/from the domain types. The wire format exists as D11's insurance:
//! today batches move over an in-process channel, but the encoding is
//! stable and language-neutral if the transport ever leaves the process.

use crate::delta::{Delta, DeltaBatch, SyncToken};
use crate::item::Item;
use chrono::{DateTime, Utc};
use prost::Message;

#[allow(clippy::all)]
pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/wkyt.delta.v1.rs"));
}

/// A protobuf payload that cannot be mapped back to domain types.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("invalid epoch-millis timestamp: {0}")]
    InvalidTimestamp(i64),
    #[error("invalid JSON in {field}: {source}")]
    InvalidJson {
        field: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("malformed protobuf: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("Delta message with no variant set")]
    EmptyDelta,
}

fn to_millis(ts: DateTime<Utc>) -> i64 {
    ts.timestamp_millis()
}

fn from_millis(ms: i64) -> Result<DateTime<Utc>, CodecError> {
    DateTime::from_timestamp_millis(ms).ok_or(CodecError::InvalidTimestamp(ms))
}

fn json_field<T: serde::de::DeserializeOwned>(
    field: &'static str,
    raw: &str,
) -> Result<T, CodecError> {
    serde_json::from_str(raw).map_err(|source| CodecError::InvalidJson { field, source })
}

impl From<&Item> for v1::Item {
    fn from(item: &Item) -> Self {
        v1::Item {
            id: item.id.clone(),
            source_id: item.source_id.clone(),
            connector_id: item.connector_id.clone(),
            // serde_json round-trip keeps ItemKind::Other lossless.
            kind_json: serde_json::to_string(&item.kind)
                .expect("ItemKind serialization is infallible"),
            timestamp_ms: to_millis(item.timestamp),
            ingested_at_ms: to_millis(item.ingested_at),
            properties_json: item.properties.to_string(),
            raw_payload_json: item.raw_payload.as_ref().map(|v| v.to_string()),
        }
    }
}

impl TryFrom<v1::Item> for Item {
    type Error = CodecError;

    fn try_from(p: v1::Item) -> Result<Self, CodecError> {
        Ok(Item {
            id: p.id,
            source_id: p.source_id,
            connector_id: p.connector_id,
            kind: json_field("kind_json", &p.kind_json)?,
            timestamp: from_millis(p.timestamp_ms)?,
            ingested_at: from_millis(p.ingested_at_ms)?,
            properties: json_field("properties_json", &p.properties_json)?,
            raw_payload: p
                .raw_payload_json
                .as_deref()
                .map(|raw| json_field("raw_payload_json", raw))
                .transpose()?,
        })
    }
}

impl From<&Delta> for v1::Delta {
    fn from(delta: &Delta) -> Self {
        let inner = match delta {
            Delta::Upsert(item) => v1::delta::Delta::Upsert(item.into()),
            Delta::Tombstone { source_id } => v1::delta::Delta::Tombstone(v1::Tombstone {
                source_id: source_id.clone(),
            }),
        };
        v1::Delta { delta: Some(inner) }
    }
}

impl TryFrom<v1::Delta> for Delta {
    type Error = CodecError;

    fn try_from(p: v1::Delta) -> Result<Self, CodecError> {
        match p.delta.ok_or(CodecError::EmptyDelta)? {
            v1::delta::Delta::Upsert(item) => Ok(Delta::Upsert(item.try_into()?)),
            v1::delta::Delta::Tombstone(t) => Ok(Delta::Tombstone {
                source_id: t.source_id,
            }),
        }
    }
}

impl From<&DeltaBatch> for v1::DeltaBatch {
    fn from(batch: &DeltaBatch) -> Self {
        v1::DeltaBatch {
            connector_id: batch.connector_id.clone(),
            deltas: batch.deltas.iter().map(Into::into).collect(),
            cursor: batch.cursor.as_ref().map(|c| c.0.clone()),
        }
    }
}

impl TryFrom<v1::DeltaBatch> for DeltaBatch {
    type Error = CodecError;

    fn try_from(p: v1::DeltaBatch) -> Result<Self, CodecError> {
        Ok(DeltaBatch {
            connector_id: p.connector_id,
            deltas: p
                .deltas
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            cursor: p.cursor.map(SyncToken),
        })
    }
}

impl DeltaBatch {
    /// Encode to the `wkyt.delta.v1` wire format.
    pub fn encode_to_vec(&self) -> Vec<u8> {
        v1::DeltaBatch::from(self).encode_to_vec()
    }

    /// Decode from the `wkyt.delta.v1` wire format.
    pub fn decode(buf: &[u8]) -> Result<Self, CodecError> {
        v1::DeltaBatch::decode(buf)?.try_into()
    }
}
