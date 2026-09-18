//! Resolver-supplied source identities for composition provenance.
//! §FS-rhei-library.4.1 §AR-rhei-library.2
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// How a source was requested and which discovery tier supplied it. The
/// locator is diagnostic data; only portable locators participate in identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceLocator {
    pub portable: bool,
    pub requested: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    pub tier: String,
}

/// The strongest source revision Rhei can honestly establish.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceRevision {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_path: Option<String>,
    pub commit: Option<String>,
    pub content: Option<String>,
    pub kind: String,
    pub replay: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

/// Source identity supplied by the resolver before lowering begins.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceIdentity {
    pub locator: SourceLocator,
    pub revision: SourceRevision,
}

impl SourceIdentity {
    pub fn unavailable(requested: impl Into<String>) -> Self {
        Self {
            locator: SourceLocator {
                portable: true,
                requested: requested.into(),
                resolved: None,
                tier: "synthetic".into(),
            },
            revision: SourceRevision {
                block_path: None,
                commit: None,
                content: None,
                kind: "unavailable".into(),
                replay: "not-guaranteed".into(),
                release: None,
                status: "unavailable".into(),
                template: None,
            },
        }
    }

    pub(super) fn id(&self) -> CompileResult<String> {
        let portable = self.locator.portable.then(|| {
            self.locator.resolved.clone().unwrap_or_else(|| self.locator.requested.clone())
        });
        let marker = match self.revision.kind.as_str() {
            "built-in" => json!([self.revision.template, self.revision.release]),
            "git" => json!(self.revision.commit),
            _ => Value::Null,
        };
        let tuple = json!([
            self.revision.kind,
            portable,
            marker,
            self.revision.block_path,
            self.revision.content
        ]);
        Ok(format!("src:sha256:{}", super::provenance::digest_value(&tuple)?))
    }
}
