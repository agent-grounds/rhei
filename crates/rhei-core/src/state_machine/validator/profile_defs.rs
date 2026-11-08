// The profile and node-policy blocks of a state machine. §FS-rhei-states.8 §FS-rhei-states.9

/// §FS-rhei-states.8: Named reusable `{initial, allowed}` state policy.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Profile {
    /// Initial state that nodes bound to this profile start in.
    pub initial: String,
    /// Complete set of state names that nodes bound to this profile may hold.
    pub allowed: Vec<String>,
    /// Applied transitions a node of this profile may make over the lifetime of
    /// its identity.
    ///
    /// Optional: a profile that omits it resolves the settings chain, so every
    /// node of every profile is bounded whether or not the machine's author
    /// thought about it. A value above the machine's ceiling is not a
    /// validation error — the node gets the machine's value and the run reports
    /// both. §FS-rhei-budgets.2.2 §FS-rhei-states.8.1
    ///
    /// Presence is the declaration, so the field is omitted rather than
    /// serialized as `null`: a profile that declares no bound renders
    /// byte-for-byte what it did, which is what lets a union promise a diff of
    /// added lines and nothing else. §FS-rhei-library.7.1
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_limit: Option<u64>,
}

/// Node-policy resolution: maps node type/level selectors to named profiles.
///
/// Resolution order: `overrides`, `by_type[<kind>]`, then `default`.
// §FS-rhei-states.9.2: Node-policy resolution order.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodePolicy {
    /// Profile bound to the project-root node (the virtual `panta` root of
    /// every load; a bare rhei is its one-rhei implicit Panta). §AR-rhei-panta.2
    pub root: String,
    /// Fallback profile for non-root nodes that match neither `overrides`
    /// nor `by_type`.
    pub default: String,
    /// Optional map from declared node kind to profile name.
    #[serde(default)]
    pub by_type: IndexMap<String, String>,
    /// Optional ordered list of `{match, profile}` overrides that win over
    /// `by_type` and `default`.
    #[serde(default)]
    pub overrides: Vec<NodePolicyOverride>,
}

/// §FS-rhei-states.9.1: Ordered node-policy override selector.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodePolicyOverride {
    /// Type/level selector. An empty selector matches every non-root node.
    #[serde(rename = "match")]
    pub match_: NodePolicyMatch,
    /// Profile name bound to matched nodes.
    pub profile: String,
}

/// §FS-rhei-states.9.3: Reject unknown node-policy match keys.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodePolicyMatch {
    /// Optional node kind selector.
    #[serde(default, rename = "type")]
    pub node_type: Option<String>,
    /// Optional node level selector (`1` for a top-level task node).
    #[serde(default)]
    pub level: Option<u8>,
}

impl NodePolicyMatch {
    fn matches(&self, kind: &str, level: u8) -> bool {
        self.node_type.as_deref().is_none_or(|want| want.eq_ignore_ascii_case(kind))
            && self.level.is_none_or(|want| want == level)
    }
}
