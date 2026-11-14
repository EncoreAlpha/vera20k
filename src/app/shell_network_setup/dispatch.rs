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
//! event owner. One received message is not a mode-dispatched event at all:
//! Cooperative's campaign text (`C0`) is named by its own `'C'` arm
//! (`0x005E43CE`), which dispatches to every mode object's `+0x58` and only
//! MPCooperative takes. [`campaign_received`] therefore resolves the text's
//! own setup identity (the WOL options payload's mode, or the LAN setup scope
//! it followed) through [`lobby_campaign`], so an early Cooperative text is
//! kept while a stale or non-Cooperative one never enters Cooperative's
//! state. VERA's room, host, roster, admission, network and reliable queue stay
//! in the shared setup; this is a mode-event split, not a plugin framework.

use crate::app::AppState;
use crate::net::lan_messages::SessionScope;
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
/// class belongs to it. The extension predicate runs first, so a VMCE event
/// never evaluates (or indirectly calls) Cooperative's owner predicate. Pure so
/// the routing rules stay testable without a live `AppState`.
fn event_owner(
    event: SetupModeEvent,
    peer: PeerMode,
    room: Option<MpModeClass>,
    extension: impl Fn(Option<MpModeClass>) -> bool,
    cooperative: impl Fn(Option<MpModeClass>) -> bool,
) -> EventOwner {
    let mode = if event.uses_peer_mode() {
        peer.class()
    } else {
        room
    };
    if extension(mode) {
        EventOwner::Extension
    } else if cooperative(mode) {
        EventOwner::Cooperative
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

/// The owner for one event of the live setup visit, resolved by the production
/// mode predicates. Split from [`live_owner`] so the chooser/room boundary can
/// be exercised without a window or GPU; the handlers still receive the
/// result through the same table.
fn live_owner_for(event: SetupModeEvent, peer: PeerMode, room: Option<MpModeClass>) -> EventOwner {
    event_owner(
        event,
        peer,
        room,
        super::vmce_setup::handles_mode,
        super::cooperative::handles_mode,
    )
}

/// The owner for one event of the live setup visit.
fn live_owner(state: &AppState, event: SetupModeEvent) -> EventOwner {
    live_owner_for(event, peer_mode(state), room_mode_class(state))
}

/// Whether the admitted room itself is Cooperative (the slots that read
/// `[0xA8B23C]`, not Choose Map's selection). The extension predicate runs
/// first, so a VMCE room never evaluates Cooperative's owner predicate.
pub(crate) fn room_is_cooperative(state: &AppState) -> bool {
    let mode = room_mode_class(state);
    if super::vmce_setup::handles_mode(mode) {
        false
    } else {
        super::cooperative::handles_mode(mode)
    }
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

/// A received Cooperative campaign `C0` text. The text's own setup identity
/// decides its owner: a WOL payload's mode, or the LAN setup scope it followed.
/// An early Cooperative text is held for the HostSetup that brings the mode;
/// a superseded or non-Cooperative text is discarded. The room mode this guest
/// currently holds is not the owner.
pub(super) fn campaign_received(
    state: &mut AppState,
    text: &str,
    origin: &super::CampaignOrigin,
) -> anyhow::Result<()> {
    super::lobby_campaign::deliver(&mut AppCampaignVisit { state }, origin, text)
}

/// A HostSetup transaction committed. The applied setup revision now decides
/// every held campaign text: a Cooperative setup applies those published
/// before it (in revision order), a non-Cooperative one discards them.
pub(super) fn campaign_options_applied(state: &mut AppState) -> anyhow::Result<()> {
    super::lobby_campaign::options_applied(&mut AppCampaignVisit { state })
}

/// The setup owners the campaign mailbox reaches in production. The trait's
/// methods are the same setup facts and Cooperative application the tests'
/// real-owner fixture uses.
struct AppCampaignVisit<'a> {
    state: &'a mut AppState,
}

impl super::lobby_campaign::CampaignVisit for AppCampaignVisit<'_> {
    fn setup_scope(&self) -> Option<SessionScope> {
        self.state
            .match_state
            .lan_service
            .as_ref()
            .and_then(|service| service.setup_scope())
    }

    fn room_mode(&self) -> Option<MpModeClass> {
        room_mode_class(self.state)
    }

    fn hold_campaign(&mut self, scope: SessionScope, text: String) {
        if let Some(protocol) = self.state.frontend.network_setup_protocol.as_mut() {
            protocol.hold_cooperative_campaign(scope, text);
        }
    }

    fn take_held_campaigns(&mut self) -> Vec<(SessionScope, String)> {
        self.state
            .frontend
            .network_setup_protocol
            .as_mut()
            .map(|protocol| protocol.take_held_cooperative_campaigns())
            .unwrap_or_default()
    }

    fn campaign_may_apply(&self, scope: SessionScope, current: SessionScope) -> bool {
        self.state
            .frontend
            .network_setup_protocol
            .as_ref()
            .is_some_and(|protocol| protocol.campaign_may_apply(scope, current))
    }

    fn observe_campaign_setup(&mut self, current: Option<SessionScope>) {
        if let Some(protocol) = self.state.frontend.network_setup_protocol.as_mut() {
            protocol.observe_campaign_setup(current);
        }
    }

    fn apply_campaign(&mut self, text: &str) -> anyhow::Result<()> {
        super::cooperative::campaign_received(self.state, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::skirmish_modes::MpModeClass::{Battle, Cooperative, Vmce};

    fn owner(event: SetupModeEvent, peer: PeerMode, room: Option<MpModeClass>) -> EventOwner {
        #[cfg(feature = "vmce")]
        {
            live_owner_for(event, peer, room)
        }
        #[cfg(not(feature = "vmce"))]
        {
            // A non-vmce build has no extension mode: its production stub
            // claims none, so this config's fixture keeps the would-be
            // extension predicate to still check the chooser/room rule.
            event_owner(
                event,
                peer,
                room,
                |mode| mode.is_some_and(|mode| mode.is_vmce()),
                |mode| mode == Some(Cooperative),
            )
        }
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

    #[test]
    fn a_vmce_event_never_asks_the_cooperative_owner() {
        // Every mode-dispatched event with a VMCE mode resolves through the
        // extension owner before Cooperative's predicate is evaluated.
        for event in [
            SetupModeEvent::HostOpened,
            SetupModeEvent::JoinSlot,
            SetupModeEvent::ParseRefresh,
            SetupModeEvent::GuestJoined,
            SetupModeEvent::GuestLeft,
            SetupModeEvent::ModeEntered,
            SetupModeEvent::MapUsed,
            SetupModeEvent::Choices,
        ] {
            let resolved = event_owner(
                event,
                PeerMode::Chooser(Some(Vmce)),
                Some(Vmce),
                |mode| mode == Some(Vmce),
                |_| panic!("a VMCE event must not evaluate the Cooperative owner predicate"),
            );
            assert_eq!(resolved, EventOwner::Extension, "{event:?}");
        }
    }
}
