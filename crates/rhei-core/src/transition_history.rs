//! The central ledger's legacy movements and exceptional audit pairs. §FS-rhei-complete.3.1

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::io;

/// Account attribution of one attended missing-edge correction. §FS-rhei-transition-cmd.6.1
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ForceAudit {
    pub confirmation: String,
    pub from: String,
    pub os_user: String,
    pub reason: String,
    pub recovery_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_sha256: Option<String>,
    pub schema_version: u8,
    pub task_id: String,
    pub timestamp: String,
    pub to: String,
}

/// A paired exception remains one movement to every consumer. §FS-rhei-complete.3.1
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Movement {
    pub task_id: String,
    pub from: String,
    pub to: String,
    pub audit: Option<ForceAudit>,
}

/// JSON object ordering is explicit even if serde_json enables preserve_order. §FS-rhei-recover.2
pub fn canonical_json<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    fn sorted(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let entries: std::collections::BTreeMap<_, _> = map.into_iter().collect();
                serde_json::Value::Object(
                    entries.into_iter().map(|(k, v)| (k, sorted(v))).collect(),
                )
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.into_iter().map(sorted).collect())
            }
            other => other,
        }
    }
    serde_json::to_vec(&sorted(serde_json::to_value(value)?)).map_err(io::Error::other)
}

/// Unpadded, canonical base64url; decoding rejects alternate tails. §FS-rhei-recover.2
pub fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}
pub fn decode(text: &str) -> io::Result<Vec<u8>> {
    let bytes = URL_SAFE_NO_PAD.decode(text).map_err(io::Error::other)?;
    if encode(&bytes) != text {
        return Err(corrupt("noncanonical base64url"));
    }
    Ok(bytes)
}

fn corrupt(reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("corrupt forced transition history: {reason}"),
    )
}

impl ForceAudit {
    /// Produce the two exact adjacent rows, with no synthetic run event. §FS-rhei-transition-cmd.6.1
    pub fn pair(&self) -> io::Result<(String, String)> {
        Ok((
            format!("{} !force-v1 {}\n", self.task_id, encode(&canonical_json(self)?)),
            format!("{} {}@{}\n", self.task_id, self.from, self.to),
        ))
    }

    /// Duplicated identity and hop are checked by the adjacent-row parser. §FS-rhei-complete.3.1
    fn from_payload(payload: &str) -> io::Result<Self> {
        let raw = decode(payload)?;
        let value: Self = serde_json::from_slice(&raw).map_err(io::Error::other)?;
        let id = uuid::Uuid::parse_str(&value.recovery_id).map_err(io::Error::other)?;
        let timestamp = time::OffsetDateTime::parse(
            &value.timestamp,
            &time::format_description::well_known::Rfc3339,
        )
        .map_err(io::Error::other)?;
        let token = |s: &str| {
            !s.is_empty()
                && !s.starts_with('!')
                && !s.chars().any(|c| c.is_whitespace() || c == '@')
        };
        if canonical_json(&value)? != raw
            || value.schema_version != 1
            || value.confirmation != "typed-hop-v1"
            || value.reason.trim().is_empty()
            || value.os_user.trim().is_empty()
            || id.get_version_num() != 7
            || id.to_string() != value.recovery_id
            || !value.timestamp.ends_with('Z')
            || timestamp.offset() != time::UtcOffset::UTC
            || !token(&value.task_id)
            || !token(&value.from)
            || !token(&value.to)
            || value.result_sha256.as_ref().is_some_and(|s| !is_digest(s))
        {
            return Err(corrupt("invalid force-v1 payload"));
        }
        Ok(value)
    }
}

pub fn is_digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Unknown metadata is skipped; malformed known pairs fail closed. §FS-rhei-complete.3.1
pub fn parse(raw: &str) -> io::Result<Vec<Movement>> {
    let mut lines = raw.split_inclusive('\n');
    let mut movements = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    while let Some(line) = lines.next() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 2 {
            continue;
        }
        if fields[1] == "!force-v1" {
            if fields.len() != 3 {
                return Err(corrupt("invalid metadata row"));
            }
            let audit = ForceAudit::from_payload(fields[2])?;
            let (expected_meta, expected_move) = audit.pair()?;
            if line != expected_meta
                || fields[0] != audit.task_id
                || lines.next() != Some(expected_move.as_str())
                || !ids.insert(audit.recovery_id.clone())
            {
                return Err(corrupt("non-adjacent, contradictory or duplicate force pair"));
            }
            movements.push(Movement {
                task_id: audit.task_id.clone(),
                from: audit.from.clone(),
                to: audit.to.clone(),
                audit: Some(audit),
            });
        } else if !fields[1].starts_with('!') {
            if let Some((from, to)) = fields[1].split_once('@') {
                if !from.is_empty() && !to.is_empty() {
                    movements.push(Movement {
                        task_id: fields[0].to_string(),
                        from: from.to_string(),
                        to: to.to_string(),
                        audit: None,
                    });
                }
            }
        }
    }
    Ok(movements)
}

#[cfg(test)]
#[path = "transition_history_tests.rs"]
mod tests;
