//! The optional `vmce` build's setup module without VMCE: the lobby dispatch
//! still names the mode's event owner, and this stub makes every VMCE slot a
//! no-op so the shared setup compiles and behaves as an ordinary mode. No VMCE
//! mode is registered in this build.

use super::AppState;
use crate::skirmish_launch::SkirmishHumanSlot;
use crate::skirmish_modes::MpModeClass;

/// No VMCE mode exists in this build, so the shared lobby dispatch has no VMCE
/// event owner to hand an event to.
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
