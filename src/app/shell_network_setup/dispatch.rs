//! Which game mode a LAN/WOL setup event belongs to.
//!
//! Native runs a few lobby events through the game-mode object's slots: the
//! host's accepted join (`0x005E2FA0`) and accepted departure (`0x005E11DC`)
//! call Choose Map's selected game type while the chooser is on top, else the
//! admitted room's mode; the host's setup open (BC `0x005ED6FE..0x005ED717`),
//! the join's `+0x1C`/`+0x28` slot rules (`0x005E1945`), an options parse's
//! mode entry (`0x005E541D`), every parse's `+24` refresh and Use Map
//! (`0x005E7160`) call the current mode (`[0xA8B23C]`).
//!
//! This module owns that selection and hands each event to the mode's own
//! event owner. VERA's room, host, roster, admission, network and reliable
//! queue stay in the shared setup; this is a mode-event split, not a plugin
//! framework.

use crate::app::AppState;
use crate::skirmish_launch::SkirmishHumanSlot;
use crate::skirmish_modes::MpModeClass;

/// The owner a mode-specific setup event reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EventOwner {
    /// The stock Cooperative mode object.
    Cooperative,
    /// VERA's extension mode object (VMCE); its optional build supplies the
    /// slots, and an ordinary build's stub claims none.
    Extension,
    /// The event's mode object has no slot of its own, or no mode is selected.
    Other,
}

/// A setup event native dispatches through a mode object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupModeEvent {
    /// Setup open's mode init.
    HostOpened,
    /// The host's join accept: the mode's `+0x1C`/`+0x28` slot rules.
    JoinSlot,
    /// Every host-options parse's `+24` refresh.
    ParseRefresh,
    /// The host's accepted join: the mode's `+0x94`.
    GuestJoined,
    /// The accepted departure: the mode's `+0x90`.
    GuestLeft,
    /// A changed mode's `+0x9C`/`+0x20`.
    ModeEntered,
    /// Use Map's mode slots.
    MapUsed,
    /// A guest's own choices (or the host checking them): the mode's `+0x1C`
    /// country admission.
    Choices,
    /// A received Cooperative campaign `C0` text: the mode's `+0x58`/`+0x5C`.
    CampaignText,
}

impl SetupModeEvent {
    /// The join accept and accepted departure name Choose Map's selected type
    /// while it is on top; every other event names the admitted room's mode.
    fn uses_peer_mode(self) -> bool {
        matches!(self, Self::GuestJoined | Self::GuestLeft)
    }
}

/// The mode a join or departure event names. An open Choose Map is
/// authoritative even when its selected row names no game type (VERA's
/// Favorites/Online Maps rows); only a closed chooser falls back to the
/// admitted room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerMode {
    Chooser(Option<MpModeClass>),
    Room(Option<MpModeClass>),
}

impl PeerMode {
    /// The class the event's mode object lookup reads.
    fn class(self) -> Option<MpModeClass> {
        match self {
            Self::Chooser(mode) | Self::Room(mode) => mode,
        }
    }
}

/// The event's owner from the modes native would read. The two mode predicates
/// keep this file mode-agnostic: each mode module says whether the event's mode
/// class belongs to it. Pure so the routing rules stay testable without a live
/// `AppState`.
fn event_owner(
    event: SetupModeEvent,
    peer: PeerMode,
    room: Option<MpModeClass>,
    cooperative: impl Fn(Option<MpModeClass>) -> bool,
    extension: impl Fn(Option<MpModeClass>) -> bool,
) -> EventOwner {
    let mode = if event.uses_peer_mode() {
        peer.class()
    } else {
        room
    };
    if cooperative(mode) {
        EventOwner::Cooperative
    } else if extension(mode) {
        EventOwner::Extension
    } else {
        EventOwner::Other
    }
}

/// The admitted room's mode class (`[0xA8B23C]`).
pub(super) fn room_mode_class(state: &AppState) -> Option<MpModeClass> {
    state
        .match_state
        .session
        .network_setup()
        .map(|launch| launch.mode.class)
}

/// The mode a join or departure event names: Choose Map's selected game type
/// while the chooser is on top, else the admitted room's mode. A chooser row
/// that names no game type (Favorites/Online Maps) keeps `Chooser(None)`, so
/// the event does not fall back to the room's mode.
fn peer_mode(state: &AppState) -> PeerMode {
    match state
        .frontend
        .skirmish_shell_state
        .choose_map_modal
        .as_ref()
    {
        Some(modal) => PeerMode::Chooser(mode_class(state, modal.selected_mode_id)),
        None => PeerMode::Room(room_mode_class(state)),
    }
}

fn mode_class(state: &AppState, mode_id: i32) -> Option<MpModeClass> {
    crate::skirmish_modes::mode_by_id(&state.frontend.skirmish_modes, mode_id)
        .map(|mode| mode.class)
}

/// The owner for one event of the live setup visit.
fn live_owner(state: &AppState, event: SetupModeEvent) -> EventOwner {
    event_owner(
        event,
        peer_mode(state),
        room_mode_class(state),
        super::cooperative::handles_mode,
        super::vmce_setup::handles_mode,
    )
}

/// Whether the admitted room itself is Cooperative (the slots that read
/// `[0xA8B23C]`, not Choose Map's selection).
pub(super) fn room_is_cooperative(state: &AppState) -> bool {
    super::cooperative::handles_mode(room_mode_class(state))
}

/// The host's setup open (`BC`'s mode init once the room is admitted).
pub(super) fn host_opened(state: &mut AppState) -> anyhow::Result<()> {
    match live_owner(state, SetupModeEvent::HostOpened) {
        EventOwner::Cooperative => super::cooperative::host_opened(state),
        EventOwner::Extension => super::vmce_setup::host_opened(state),
        EventOwner::Other => Ok(()),
    }
}

/// The host's join accept: the mode's slot default before the shared bans and
/// roster admission run.
pub(super) fn admit_join_slot(state: &AppState, human: &mut SkirmishHumanSlot) {
    match live_owner(state, SetupModeEvent::JoinSlot) {
        EventOwner::Cooperative => super::cooperative::admit_join_country(state, &mut human.slot),
        EventOwner::Extension => super::vmce_setup::admit_join_slot(human),
        EventOwner::Other => {}
    }
}

/// Every host-options parse's `+24` refresh.
pub(super) fn parse_refresh(state: &mut AppState) {
    match live_owner(state, SetupModeEvent::ParseRefresh) {
        EventOwner::Cooperative => super::cooperative::parse_refresh(state),
        EventOwner::Extension => super::vmce_setup::parse_refresh(state),
        EventOwner::Other => {}
    }
}

/// The host's accepted join.
pub(super) fn guest_joined(state: &mut AppState) -> anyhow::Result<()> {
    match live_owner(state, SetupModeEvent::GuestJoined) {
        EventOwner::Cooperative => super::cooperative::guest_joined(state),
        EventOwner::Extension => super::vmce_setup::guest_joined(state),
        EventOwner::Other => Ok(()),
    }
}

/// The accepted departure, on the host and on a WOL guest alike.
pub(super) fn guest_left(state: &mut AppState) -> anyhow::Result<()> {
    match live_owner(state, SetupModeEvent::GuestLeft) {
        EventOwner::Cooperative => super::cooperative::guest_left(state),
        EventOwner::Extension => super::vmce_setup::guest_left(state),
        EventOwner::Other => Ok(()),
    }
}

/// A changed mode's entry.
pub(super) fn mode_entered(state: &mut AppState) -> anyhow::Result<()> {
    match live_owner(state, SetupModeEvent::ModeEntered) {
        EventOwner::Cooperative => super::cooperative::mode_entered(state),
        EventOwner::Extension => super::vmce_setup::mode_entered(state),
        EventOwner::Other => Ok(()),
    }
}

/// Use Map's mode slots.
pub(super) fn map_used(state: &mut AppState, mode_changed: bool, row: usize) -> anyhow::Result<()> {
    match live_owner(state, SetupModeEvent::MapUsed) {
        EventOwner::Cooperative => super::cooperative::map_used(state, mode_changed, row),
        EventOwner::Extension => super::vmce_setup::map_used(state, mode_changed, row),
        EventOwner::Other => Ok(()),
    }
}

/// The countries the current mode admits for a guest's own choices. Only
/// Cooperative's campaign narrows them; the extension mode's roles decide their
/// own countries.
pub(super) fn choice_countries(
    state: &AppState,
) -> Option<Vec<crate::skirmish_launch::LaunchCountry>> {
    match live_owner(state, SetupModeEvent::Choices) {
        EventOwner::Cooperative => state
            .frontend
            .multiplayer_shell_runtime
            .cooperative_player_countries(),
        _ => None,
    }
}

/// A received Cooperative campaign `C0` text: only Cooperative's mode object
/// takes it (`0x005E43CE`); other modes' `+0x58` slot does nothing.
pub(super) fn campaign_received(state: &mut AppState, text: &str) -> anyhow::Result<()> {
    match live_owner(state, SetupModeEvent::CampaignText) {
        EventOwner::Cooperative => super::cooperative::campaign_received(state, text),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::skirmish_modes::MpModeClass::{Battle, Cooperative, Vmce};

    fn owner(event: SetupModeEvent, peer: PeerMode, room: Option<MpModeClass>) -> EventOwner {
        event_owner(
            event,
            peer,
            room,
            |mode| mode == Some(Cooperative),
            |mode| mode == Some(Vmce),
        )
    }

    #[test]
    fn host_join_accept_uses_the_open_choosers_mode() {
        // Choose Map can stay on top with a game type other than the admitted
        // room's; native 0x005E2FA0 calls that selected type, so the join must
        // not fall back to the room.
        assert_eq!(
            owner(
                SetupModeEvent::GuestJoined,
                PeerMode::Chooser(Some(Vmce)),
                Some(Cooperative)
            ),
            EventOwner::Extension
        );
        assert_eq!(
            owner(
                SetupModeEvent::GuestJoined,
                PeerMode::Chooser(Some(Cooperative)),
                Some(Vmce)
            ),
            EventOwner::Cooperative
        );
        // A closed chooser falls back to the admitted room's mode.
        assert_eq!(
            owner(
                SetupModeEvent::GuestJoined,
                PeerMode::Room(Some(Vmce)),
                Some(Vmce)
            ),
            EventOwner::Extension
        );
        // A selected mode whose slots are no-ops is not the admitted mode.
        assert_eq!(
            owner(
                SetupModeEvent::GuestJoined,
                PeerMode::Chooser(Some(Battle)),
                Some(Cooperative)
            ),
            EventOwner::Other
        );
    }

    #[test]
    fn host_and_guest_departures_use_the_open_choosers_mode() {
        // 0x005E11DC reads the same chooser-selected type as the join.
        assert_eq!(
            owner(
                SetupModeEvent::GuestLeft,
                PeerMode::Chooser(Some(Vmce)),
                Some(Cooperative)
            ),
            EventOwner::Extension
        );
        assert_eq!(
            owner(
                SetupModeEvent::GuestLeft,
                PeerMode::Chooser(Some(Cooperative)),
                Some(Vmce)
            ),
            EventOwner::Cooperative
        );
        assert_eq!(
            owner(
                SetupModeEvent::GuestLeft,
                PeerMode::Room(Some(Cooperative)),
                Some(Cooperative)
            ),
            EventOwner::Cooperative
        );
        assert_eq!(
            owner(
                SetupModeEvent::GuestLeft,
                PeerMode::Chooser(Some(Battle)),
                None
            ),
            EventOwner::Other
        );
    }

    #[test]
    fn chooser_own_rows_do_not_fall_back_to_the_room_mode() {
        // Choose Map lists VERA's Favorites/Online Maps rows
        // (`FAVORITES_ROW`/`ONLINE_MAPS_ROW`); they name no game type, and an
        // open chooser is authoritative: a join/leave there must run no mode
        // slot instead of the admitted room's.
        assert_eq!(
            owner(
                SetupModeEvent::GuestJoined,
                PeerMode::Chooser(None),
                Some(Cooperative)
            ),
            EventOwner::Other
        );
        assert_eq!(
            owner(
                SetupModeEvent::GuestLeft,
                PeerMode::Chooser(None),
                Some(Vmce)
            ),
            EventOwner::Other
        );
        // Room-mode events ignore the chooser's row entirely.
        assert_eq!(
            owner(
                SetupModeEvent::ModeEntered,
                PeerMode::Chooser(None),
                Some(Cooperative)
            ),
            EventOwner::Cooperative
        );
    }

    #[test]
    fn guest_join_mode_entry_uses_the_admitted_room_mode() {
        // The guest's HostSetup parse enters the new admitted mode; an open
        // chooser is not that mode.
        assert_eq!(
            owner(
                SetupModeEvent::ModeEntered,
                PeerMode::Chooser(Some(Cooperative)),
                Some(Vmce)
            ),
            EventOwner::Extension
        );
        assert_eq!(
            owner(
                SetupModeEvent::ModeEntered,
                PeerMode::Chooser(Some(Vmce)),
                Some(Cooperative)
            ),
            EventOwner::Cooperative
        );
        assert_eq!(
            owner(SetupModeEvent::ModeEntered, PeerMode::Room(None), None),
            EventOwner::Other
        );
    }

    #[test]
    fn host_setup_open_join_slot_refresh_use_map_and_choices_follow_the_room_mode() {
        for event in [
            SetupModeEvent::HostOpened,
            SetupModeEvent::JoinSlot,
            SetupModeEvent::ParseRefresh,
            SetupModeEvent::MapUsed,
            SetupModeEvent::Choices,
            SetupModeEvent::CampaignText,
        ] {
            assert_eq!(
                owner(event, PeerMode::Chooser(Some(Vmce)), Some(Cooperative)),
                EventOwner::Cooperative,
                "{event:?} reads the admitted room's mode"
            );
            assert_eq!(
                owner(event, PeerMode::Chooser(Some(Cooperative)), Some(Vmce)),
                EventOwner::Extension
            );
        }
        // No admitted mode and a mode whose slots do nothing have no owner.
        assert_eq!(
            owner(SetupModeEvent::JoinSlot, PeerMode::Room(None), Some(Battle)),
            EventOwner::Other
        );
        assert_eq!(
            owner(
                SetupModeEvent::HostOpened,
                PeerMode::Chooser(Some(Vmce)),
                None
            ),
            EventOwner::Other
        );
    }
}
