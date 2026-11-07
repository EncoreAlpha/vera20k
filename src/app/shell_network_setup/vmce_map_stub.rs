//! Feature-off stand-in for the VMCE map/series loader.
//!
//! A build without the `vmce` capability never registers the mode row and
//! never scans `Maps/VMCE`, so the shared LAN options/actions call sites only
//! need a closed failure path; the real loader in `vmce_map.rs` is compiled
//! out entirely.

use anyhow::{Result, bail};

use crate::app::AppState;

pub(crate) fn resolve_host_vmce_map_config(
    _state: &AppState,
    _map_file: &str,
) -> Result<crate::app::match_bootstrap::NetworkVmceMapConfig> {
    bail!("VMCE is not built in")
}

pub(crate) fn host_vmce_identity(
    _state: &AppState,
    _map_file: &str,
    _expected_source_sha256: &str,
) -> Result<(String, String)> {
    bail!("VMCE is not built in")
}

pub(crate) fn resolve_local_vmce_config(
    _state: &AppState,
    _map_file: &str,
    _directory: &str,
    _expected_source_sha256: &str,
) -> Result<(crate::app::match_bootstrap::NetworkVmceMapConfig, String)> {
    bail!("VMCE is not built in")
}

pub(crate) fn refresh_vmce_entries(_state: &mut AppState) {}
