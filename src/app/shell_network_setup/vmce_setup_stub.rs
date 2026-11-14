//! The optional `vmce` build's setup module without VMCE: the lobby dispatch
//! still names the mode's event owner, and this stub makes every VMCE slot a
//! no-op so the shared setup compiles. The VMCE identity is registered, but
//! this build cannot admit its setup.

use super::AppState;
use crate::skirmish_launch::{SkirmishHumanSlot, SkirmishLaunchSession};
use crate::skirmish_modes::MpModeClass;

/// Preserve the host's VMCE identity, but reject it before any shared session
/// state changes: this build has no complete VMCE setup/admission backend.
pub(super) fn ensure_host_setup_supported(
    state: &mut AppState,
    launch: &SkirmishLaunchSession,
) -> anyhow::Result<()> {
    if !supports_host_setup(launch.mode.class) {
        super::chat_notice(state, "VERA:VmceBlockingVersionMismatch", &[]);
        anyhow::bail!("VMCE host setup requires the complete VMCE backend");
    }
    Ok(())
}

pub(crate) fn supports_host_setup(mode: MpModeClass) -> bool {
    !mode.policy().authored_campaign_map_catalog
}

/// This migration bridge recognizes the VMCE mode, but it has no complete
/// VMCE lobby event owner to hand an event to.
pub(super) fn handles_mode(_mode: Option<MpModeClass>) -> bool {
    false
}

pub(super) fn admit_join_slot(_human: &mut SkirmishHumanSlot) {}

pub(super) fn host_opened(_state: &mut AppState) -> anyhow::Result<()> {
    Ok(())
}

pub(super) fn guest_joined(_state: &mut AppState) -> anyhow::Result<()> {
    Ok(())
}

pub(super) fn guest_left(_state: &mut AppState) -> anyhow::Result<()> {
    Ok(())
}

pub(super) fn mode_entered(_state: &mut AppState) -> anyhow::Result<()> {
    Ok(())
}

pub(super) fn parse_refresh(_state: &mut AppState) {}

pub(super) fn map_used(
    _state: &mut AppState,
    _mode_changed: bool,
    _row: usize,
) -> anyhow::Result<()> {
    Ok(())
}
