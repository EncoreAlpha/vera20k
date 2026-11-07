//! Feature-off stand-in for the VMCE supplemental map-config parser.
//!
//! Without the mode no VMCE map can be selected, so the parser is not
//! compiled. The type keeps the same shape for the shared startup/protocol
//! types, and every entry fails closed instead of duplicating validation.

use crate::rules::ini_parser::IniFile;

/// Normalized external settings admitted for a VMCE map (absent without the
/// VMCE feature).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmceMapConfig {}

impl VmceMapConfig {
    pub fn from_bytes(_map_ini: &IniFile, _supplemental: &[u8]) -> anyhow::Result<Self> {
        anyhow::bail!("VMCE is not built in")
    }

    pub fn from_ini(_map_ini: &IniFile, _supplemental: &IniFile) -> anyhow::Result<Self> {
        anyhow::bail!("VMCE is not built in")
    }

    pub fn apply_to_map(&self, _map_ini: &IniFile) -> anyhow::Result<IniFile> {
        anyhow::bail!("VMCE is not built in")
    }

    pub fn replace_map_cooperative_values(&mut self) {}
}
