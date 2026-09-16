/// Native reasoning-effort translation declared by one agent profile.
// §FS-rhei-agents.1.1.2
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AgentEffortProfile {
    pub values: BTreeMap<String, String>,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<Vec<String>>,
}
