//! Typed block-composition primitives.
//!
//! Text manifests deserialize into these values before the CLI resolves or
//! renders any mounted template.  Runtime code never reads these types: the
//! block compiler lowers them to an ordinary plan and state machine first.
//! §AR-rhei-library.1 §AR-rhei-library.4

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The identifier grammar shared by block aliases, ports, and endpoints.
/// §FS-rhei-library.1 §FS-rhei-library.2
pub fn valid_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// One recursively mounted block.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Mount {
    pub block: String,
    #[serde(rename = "as")]
    pub alias: String,
}

/// Public control surface of a block.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ControlPorts {
    pub entry: String,
    pub exits: BTreeMap<String, String>,
}

/// The two runtime handoff mechanisms supported by composition.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DataKind {
    StateFile,
    TaskExport,
}

impl fmt::Display for DataKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::StateFile => "state-file",
            Self::TaskExport => "task-export",
        })
    }
}

/// A public runtime-data endpoint. Exactly one of `state` and `task` is
/// meaningful for its corresponding kind and is validated by the compiler.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DataEndpoint {
    pub kind: DataKind,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    pub name: String,
}

/// Public runtime inputs and outputs of a block.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct DataPorts {
    #[serde(default)]
    pub inputs: BTreeMap<String, DataEndpoint>,
    #[serde(default)]
    pub outputs: BTreeMap<String, DataEndpoint>,
}

/// Compile-time partial application from a parent input to a child input.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct InputBinding {
    pub input: String,
    pub to: String,
}

/// Completion-only control edge, with optional runtime data wiring.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Seam {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub pass: BTreeMap<String, String>,
}

/// Root-only stable-name lowering used by compatibility wrappers.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CompatibilityMap {
    #[serde(default)]
    pub states: BTreeMap<String, String>,
    #[serde(default)]
    pub tasks: BTreeMap<String, String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, String>,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
}

impl CompatibilityMap {
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
            && self.tasks.is_empty()
            && self.profiles.is_empty()
            && self.settings.is_empty()
            && self.artifacts.is_empty()
    }

    /// Return the first pair of stable identities that target one compiled
    /// identity. Silent many-to-one lowering would make references ambiguous.
    /// §FS-rhei-library.7
    pub fn collision(&self) -> Option<(&str, &str, &str)> {
        for map in [&self.states, &self.tasks, &self.profiles, &self.settings, &self.artifacts] {
            let mut targets = BTreeMap::<&str, &str>::new();
            for (stable, target) in map {
                if let Some(previous) = targets.insert(target, stable) {
                    return Some((previous, stable, target));
                }
            }
        }
        None
    }
}

/// Composition fields embedded in the existing `template.yaml` manifest.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct BlockManifest {
    #[serde(default)]
    pub ports: Option<ControlPorts>,
    #[serde(default)]
    pub data: DataPorts,
    #[serde(default, rename = "use")]
    pub mounts: Vec<Mount>,
    #[serde(default)]
    pub bind: Vec<InputBinding>,
    /// `None` means default ordered chaining; `Some` is the complete authored
    /// seam set, including the deliberately empty set for one mount.
    #[serde(default)]
    pub seams: Option<Vec<Seam>>,
    #[serde(default)]
    pub compatibility: CompatibilityMap,
}

impl BlockManifest {
    pub fn is_block(&self) -> bool {
        self.ports.is_some()
            || !self.mounts.is_empty()
            || !self.bind.is_empty()
            || self.seams.is_some()
            || !self.data.inputs.is_empty()
            || !self.data.outputs.is_empty()
            || !self.compatibility.is_empty()
    }
}

/// Injective alias-chain qualifier used for every block-owned identifier.
/// §FS-rhei-library.4
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Qualifier {
    chain: Vec<String>,
}

impl Qualifier {
    pub fn new(chain: Vec<String>) -> Self {
        Self { chain }
    }

    pub fn chain(&self) -> &[String] {
        &self.chain
    }

    pub fn child(&self, alias: impl Into<String>) -> Self {
        let mut chain = self.chain.clone();
        chain.push(alias.into());
        Self { chain }
    }

    pub fn prefix(&self) -> String {
        use std::fmt::Write;
        self.chain.iter().fold(String::new(), |mut prefix, alias| {
            write!(prefix, "m{}_{}__", alias.len(), alias).expect("writing a string cannot fail");
            prefix
        })
    }

    pub fn qualify(&self, local: &str) -> String {
        format!("{}{}", self.prefix(), local)
    }
}

/// Parse `<mount>.<public-name>` without accepting internal or ambiguous
/// names. §FS-rhei-library.1–2
pub fn split_endpoint(value: &str) -> Option<(&str, &str)> {
    let (mount, endpoint) = value.split_once('.')?;
    (!mount.is_empty() && !endpoint.is_empty() && !endpoint.contains('.'))
        .then_some((mount, endpoint))
}

/// Validate that explicit seams are one complete linear ordering of all
/// sibling aliases and return that order. §FS-rhei-library.2
pub fn linear_seam_order(aliases: &[String], seams: &[Seam]) -> Result<Vec<String>, String> {
    if aliases.len() <= 1 {
        return if seams.is_empty() {
            Ok(aliases.to_vec())
        } else {
            Err("a single mount cannot declare an outer seam".to_string())
        };
    }
    if seams.len() != aliases.len() - 1 {
        return Err(format!(
            "explicit seams must form one complete chain over {}; got {} seam(s)",
            aliases.join(", "),
            seams.len()
        ));
    }

    let known = aliases.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut outgoing = BTreeMap::<&str, &str>::new();
    let mut incoming = BTreeMap::<&str, &str>::new();
    for seam in seams {
        let (from, _) = split_endpoint(&seam.from)
            .ok_or_else(|| format!("invalid seam source '{}'", seam.from))?;
        let (to, _) =
            split_endpoint(&seam.to).ok_or_else(|| format!("invalid seam target '{}'", seam.to))?;
        if !known.contains(from) || !known.contains(to) {
            return Err(format!("seam '{}={}' names an unknown mount", seam.from, seam.to));
        }
        if outgoing.insert(from, to).is_some() || incoming.insert(to, from).is_some() {
            return Err("explicit seams must form one complete linear chain".to_string());
        }
    }

    let heads =
        aliases.iter().filter(|alias| !incoming.contains_key(alias.as_str())).collect::<Vec<_>>();
    if heads.len() != 1 {
        return Err("explicit seams must have exactly one head and be acyclic".to_string());
    }
    let mut order = Vec::with_capacity(aliases.len());
    let mut cursor = heads[0].as_str();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(cursor) {
            return Err("explicit seams contain a cycle".to_string());
        }
        order.push(cursor.to_string());
        let Some(next) = outgoing.get(cursor) else { break };
        cursor = next;
    }
    (order.len() == aliases.len())
        .then_some(order)
        .ok_or_else(|| "explicit seams must form one complete chain over every mount".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualifier_is_injective_for_nested_aliases() {
        let nested = Qualifier::new(vec!["a".into(), "bc".into()]).qualify("done");
        let other = Qualifier::new(vec!["ab".into(), "c".into()]).qualify("done");
        assert_eq!(nested, "m1_a__m2_bc__done");
        assert_eq!(other, "m2_ab__m1_c__done");
        assert_ne!(nested, other);
    }

    #[test]
    fn explicit_seams_choose_the_whole_linear_order() {
        let aliases = vec!["review".into(), "fix".into(), "ship".into()];
        let seams = vec![
            Seam { from: "fix.done".into(), to: "ship.entry".into(), pass: BTreeMap::new() },
            Seam { from: "review.done".into(), to: "fix.entry".into(), pass: BTreeMap::new() },
        ];
        assert_eq!(linear_seam_order(&aliases, &seams).unwrap(), aliases);
    }
}

mod compatibility;
mod compiler;
mod data;
mod emit;
mod links;
mod qualify;
mod references;
mod settings;
pub use compiler::{
    Block, CompileResult, CompiledBlock, CompiledFile, Endpoint, Fragment, TaskFile,
};

#[cfg(test)]
mod compiler_tests;
