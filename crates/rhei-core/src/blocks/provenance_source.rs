//! Resolver-supplied source identities for composition provenance.
//! §FS-rhei-library.4.1 §AR-rhei-library.2
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Reconciled diagnostic locators for one content identity. §FS-rhei-library.4.1
#[derive(Debug, Clone, Serialize)]
pub(super) struct SourceRecord {
    pub locator: SourceLocator,
    pub locators: Vec<SourceLocator>,
    pub revision: SourceRevision,
}

impl From<&SourceIdentity> for SourceRecord {
    fn from(source: &SourceIdentity) -> Self {
        Self {
            locator: source.locator.clone(),
            locators: vec![source.locator.clone()],
            revision: source.revision.clone(),
        }
    }
}

impl SourceRecord {
    pub fn reconcile(&mut self, other: Self, id: &str) -> CompileResult<()> {
        if self.revision != other.revision {
            return Err(format!("source identity collision '{id}'"));
        }
        self.locators.extend(other.locators);
        self.locators.sort_by(|a, b| {
            (&a.requested, &a.tier, a.portable, &a.resolved).cmp(&(
                &b.requested,
                &b.tier,
                b.portable,
                &b.resolved,
            ))
        });
        self.locators.dedup();
        self.locator = self.locators[0].clone();
        Ok(())
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locator_reconciliation_stays_canonical_and_refuses_revision_conflicts() {
        let mut a = SourceIdentity::unavailable("/a");
        a.locator.portable = false;
        let mut b = a.clone();
        b.locator.requested = "/b".into();
        let id = a.id().unwrap();
        assert_eq!(id, b.id().unwrap());
        let mut first = SourceRecord::from(&a);
        first.reconcile(SourceRecord::from(&b), &id).unwrap();
        let mut second = SourceRecord::from(&b);
        second.reconcile(SourceRecord::from(&a), &id).unwrap();
        second.reconcile(SourceRecord::from(&a), &id).unwrap();
        assert_eq!(first.locator, a.locator);
        assert_eq!(first.locators.len(), 2);
        assert_eq!(serde_json::to_value(&first).unwrap(), serde_json::to_value(&second).unwrap());
        b.revision.replay = "exact".into();
        assert!(first
            .reconcile(SourceRecord::from(&b), &id)
            .unwrap_err()
            .contains("source identity collision"));
    }
}
