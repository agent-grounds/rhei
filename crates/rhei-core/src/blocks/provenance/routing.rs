//! Keep routing contributors aligned with typed lowering until final emission.
//! §FS-rhei-library.4.1 §AR-rhei-library.4
use super::*;
use crate::state_machine::{NodePolicy, NodePolicyOverride};

impl ProvenanceStore {
    pub(crate) fn expand_overrides(
        &mut self,
        rules: &[NodePolicyOverride],
        kinds: usize,
    ) -> CompileResult<()> {
        self.check_override_count(rules.len())?;
        let mut expanded = Vec::new();
        for (rule, origins) in rules.iter().zip(std::mem::take(&mut self.override_origins)) {
            if rule.match_.node_type.is_some() {
                expanded.push(origins);
            } else {
                let origins = synthesized(origins, "expanded-level-routing");
                expanded.extend(std::iter::repeat_n(origins, kinds));
            }
        }
        self.override_origins = expanded;
        Ok(())
    }

    pub(crate) fn fold_primary_profiles(
        &mut self,
        policy: &NodePolicy,
        primary: &BTreeSet<String>,
    ) -> CompileResult<()> {
        self.check_override_count(policy.overrides.len())?;
        // The outer lane derives from all primary profiles. §FS-rhei-library.4.1
        let contributors = primary
            .iter()
            .flat_map(|profile| self.nodes.profiles.get(profile).into_iter().flatten().cloned())
            .collect::<Vec<_>>();
        let fold = |origins: &mut Vec<Origin>| {
            origins.extend(contributors.iter().cloned());
            *origins = synthesized(std::mem::take(origins), "folded-primary-routing");
        };
        for (kind, profile) in &policy.by_type {
            if primary.contains(profile) {
                let key = format!("by_type/{kind}");
                let origins = self.nodes.routing.get_mut(&key).ok_or_else(|| {
                    format!("routing provenance missing for '{key}' during primary-profile folding")
                })?;
                fold(origins);
            }
        }
        for (rule, origins) in policy.overrides.iter().zip(&mut self.override_origins) {
            if primary.contains(&rule.profile) {
                fold(origins);
            }
        }
        Ok(())
    }

    pub(super) fn finalize_overrides(&mut self, policy: Option<&NodePolicy>) -> CompileResult<()> {
        let rules = policy.map(|policy| policy.overrides.as_slice()).unwrap_or_default();
        self.check_override_count(rules.len())?;
        self.nodes.routing.retain(|key, _| !key.starts_with("overrides/"));
        let mut duplicate = BTreeMap::<String, usize>::new();
        for (rule, origins) in rules.iter().zip(&self.override_origins) {
            let rendered = serde_json::to_value(rule).map_err(|e| e.to_string())?;
            let digest = digest_value(&rendered)?;
            let ordinal = duplicate.entry(digest.clone()).or_default();
            *ordinal += 1;
            let key = format!("overrides/sha256:{digest}~{ordinal}");
            add_origins(&mut self.nodes.routing, key, origins.clone());
        }
        Ok(())
    }

    fn check_override_count(&self, count: usize) -> CompileResult<()> {
        if count != self.override_origins.len() {
            return Err("routing override provenance lost alignment during lowering".into());
        }
        Ok(())
    }
}

fn synthesized(origins: Vec<Origin>, reason: &str) -> Vec<Origin> {
    canonical(
        origins
            .into_iter()
            .map(|origin| Origin {
                via: "synthesized".into(),
                reason: Some(reason.into()),
                ..origin
            })
            .collect(),
    )
}
