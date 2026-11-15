//! The optional `vmce` build's setup module without VMCE: the lobby dispatch
//! still names the mode's event owner, and this stub makes every VMCE slot a
//! no-op so the shared setup compiles. The VMCE identity is registered, but
//! this build cannot admit its setup.

use super::AppState;
use crate::skirmish_launch::{
    LaunchHumanRole, SkirmishHumanSlot, SkirmishLaunchSession, SkirmishLocalSlot,
};
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

/// This migration bridge has no playable VMCE backend. The shared start command
/// still owns the ordering and prints the reason before GO.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VmceStartPermission {
    /// The complete VMCE lobby constructs this; the migration bridge never
    /// grants it.
    #[allow(dead_code)]
    Allowed,
    Denied(&'static str),
}

pub(super) fn start_permission(
    _state: &mut AppState,
    _launch: &SkirmishLaunchSession,
) -> VmceStartPermission {
    VmceStartPermission::Denied(
        "VMCE cannot start because this build has no playable VMCE backend for the selected campaign.",
    )
}

/// This bridge cannot admit a VMCE room, but the shared command's minimum
/// check runs before the mode's start rule; keep the mode's own player rule so
/// the backend refusal stays the reachable reason once a room has players.
pub(super) fn minimum_players_met(launch: &SkirmishLaunchSession) -> bool {
    !launch.humans.is_empty() && launch.humans.iter().all(|human| !human.is_observer())
}

pub(super) enum VmceRoleSelection {
    Host {
        index: usize,
        role: LaunchHumanRole,
    },
    Guest {
        index: usize,
        row: u8,
        slot: SkirmishLocalSlot,
        role: LaunchHumanRole,
    },
}

pub(super) fn role_selection(
    _launch: &SkirmishLaunchSession,
    _row: u8,
    _value: i32,
    _host: bool,
) -> anyhow::Result<VmceRoleSelection> {
    anyhow::bail!("VMCE is not built in")
}

pub(super) fn validate_peer_role(
    _state: &AppState,
    _index: usize,
    _role: LaunchHumanRole,
) -> anyhow::Result<()> {
    anyhow::bail!("VMCE is not built in")
}

pub(super) fn accept_peer_role(
    _state: &mut AppState,
    _index: usize,
    _role: LaunchHumanRole,
) -> anyhow::Result<()> {
    anyhow::bail!("VMCE is not built in")
}

pub(super) fn reset_roles_after_map_change(_state: &mut AppState) -> anyhow::Result<()> {
    Ok(())
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
