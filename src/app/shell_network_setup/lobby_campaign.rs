//! Who owns a received Cooperative campaign text (`C0`) and when it applies.
//!
//! Native's `'C'` arm (`0x005E43CE`) hands the text to every mode object's
//! `+0x58`; only MPCooperative takes it. VERA receives the same text on two
//! transports with different shapes, and the message itself names the setup
//! it belongs to:
//!
//! - WOL carries it inside the host-options payload (`'L'`, `0x5E8EA0`), so
//!   the owning mode is that payload's mode. The host publishes it ahead of
//!   the same payload's options (`+0x94` at the join accept `0x007A6E5A`,
//!   Use Map's `+0x60` before its publish); the guest applies it to
//!   Cooperative before the mode entry because that is the order the host
//!   sent, even though the guest's room mode is still the old one.
//! - LAN sends it as its own short options packet (`0x17`), whose session
//!   proof names the setup scope it followed. The host emits one only in a
//!   Cooperative room. When the packet overtakes the options revision that
//!   carries it (a mode switch into Cooperative, or reordered reliable
//!   delivery), the text is held until the next HostSetup: that setup's mode
//!   decides whether the text was an early Cooperative text or a stale text
//!   from a room that changed away. A superseded revision, a non-Cooperative
//!   setup and leaving the room discard the hold.
//!
//! This module owns the pending texts and the applicability decision; the
//! Cooperative progress owner performs the application through
//! [`super::cooperative::apply_received_campaign`]. It does not keep a second
//! campaign record.

use crate::net::lan_messages::SessionScope;
use crate::skirmish_modes::MpModeClass;

/// The setup identity a received `C0` belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CampaignOrigin {
    /// The text rode in this host-options payload; its mode owns it.
    Options(MpModeClass),
    /// A LAN short options packet published under this admitted setup scope.
    Lan(SessionScope),
}

impl CampaignOrigin {
    pub(crate) fn options(mode: MpModeClass) -> Self {
        Self::Options(mode)
    }

    pub(crate) fn lan(scope: SessionScope) -> Self {
        Self::Lan(scope)
    }
}

/// A held text per future setup revision. Revision numbers only grow inside
/// one admitted epoch, so a newer text for the same scope replaces the older
/// stage/counters and later texts never displace an older-handled scope.
#[derive(Debug, Default)]
pub(crate) struct CampaignInbox {
    held: Vec<(SessionScope, String)>,
    /// The setup revision the previous HostSetup applied. A delayed text at or
    /// after this revision still describes the current Cooperative setup; an
    /// older one is superseded. The transition into Cooperative publishes its
    /// text under the previous revision, so equal-to-floor is applicable.
    floor: Option<SessionScope>,
    /// The setup revision the latest HostSetup applied.
    observed: Option<SessionScope>,
}

impl CampaignInbox {
    /// Keep a text until the HostSetup that owns it arrives. At most four
    /// future revisions can be in flight (a host publishes one setup batch at
    /// a time); a longer hold is a stale queue, so the oldest goes.
    pub(crate) fn hold(&mut self, scope: SessionScope, text: String) {
        const MAX_HELD: usize = 4;
        if let Some((_, held)) = self.held.iter_mut().find(|(known, _)| *known == scope) {
            *held = text;
            return;
        }
        if self.held.len() == MAX_HELD {
            self.held.remove(0);
        }
        self.held.push((scope, text));
    }

    pub(crate) fn take(&mut self) -> Vec<(SessionScope, String)> {
        let mut held = std::mem::take(&mut self.held);
        held.sort_by_key(|(scope, _)| scope.revision);
        held
    }

    /// A HostSetup committed at `current`. The revision it replaced becomes
    /// the floor for a delayed text that arrives after it. Production applies
    /// the same options revision again for a roster/choices receipt
    /// (`reapply_received_options`); that is not a new setup and must keep the
    /// floor, or a transition text still in flight would be rejected.
    pub(crate) fn observe_setup(&mut self, current: Option<SessionScope>) {
        let Some(current) = current else {
            self.floor = None;
            self.observed = None;
            return;
        };
        match self.observed {
            Some(previous)
                if previous.epoch == current.epoch && previous.revision == current.revision =>
            {
                return;
            }
            Some(previous) if previous.epoch != current.epoch => self.floor = None,
            Some(previous) => self.floor = Some(previous),
            None => self.floor = None,
        }
        self.observed = Some(current);
    }

    /// Whether a text published under `scope` still belongs to the current
    /// Cooperative setup. Before any HostSetup is observed, every text up to
    /// the applied revision is acceptable; after it, the revision that this
    /// setup replaced is the floor.
    pub(crate) fn may_apply(&self, scope: SessionScope, current: SessionScope) -> bool {
        if current.epoch != scope.epoch {
            return false;
        }
        match self.floor {
            Some(floor) if floor.epoch == scope.epoch => scope.revision >= floor.revision,
            Some(_) => false,
            None => scope.revision <= current.revision,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.held.clear();
        self.floor = None;
        self.observed = None;
    }

    #[cfg(test)]
    pub(crate) fn held_len(&self) -> usize {
        self.held.len()
    }
}

/// The setup facts and owners a campaign delivery reads and mutates. AppState
/// implements it; the campaign tests implement it with the same real session,
/// shell runtime and inbox, so they drive this production handler without a
/// window or GPU.
pub(super) trait CampaignVisit {
    /// The admitted LAN setup revision this receiver has applied. WOL has no
    /// LAN service scope and resolves ownership from the payload instead.
    fn setup_scope(&self) -> Option<SessionScope>;
    /// The mode of the room this receiver has admitted.
    fn room_mode(&self) -> Option<MpModeClass>;
    fn hold_campaign(&mut self, scope: SessionScope, text: String);
    fn take_held_campaigns(&mut self) -> Vec<(SessionScope, String)>;
    /// Whether a text published under `scope` still belongs to the setup
    /// revision `current` (the previous HostSetup revision is the floor).
    fn campaign_may_apply(&self, scope: SessionScope, current: SessionScope) -> bool;
    /// The HostSetup at `current` committed; it becomes the next floor.
    fn observe_campaign_setup(&mut self, current: Option<SessionScope>);
    /// Apply one Cooperative `C0` through the progress owner.
    fn apply_campaign(&mut self, text: &str) -> anyhow::Result<()>;
}

/// Deliver one received `C0`. A host-options text is owned by that payload's
/// mode. A LAN text is owned by the next Cooperative setup at or after its
/// scope: a text from another admission, or below the revision the current
/// setup replaced, never reaches Cooperative; an early text is held for the
/// next HostSetup, and a transition text delayed past that HostSetup but at or
/// after its floor still applies.
pub(super) fn deliver<V: CampaignVisit>(
    visit: &mut V,
    origin: &CampaignOrigin,
    text: &str,
) -> anyhow::Result<()> {
    match origin {
        CampaignOrigin::Options(mode) => {
            if super::cooperative::handles_mode(Some(*mode)) {
                visit.apply_campaign(text)?;
            }
        }
        CampaignOrigin::Lan(scope) => {
            let Some(current) = visit.setup_scope() else {
                return Ok(());
            };
            if current.epoch != scope.epoch {
                // A previous admission's text can never own this setup.
                return Ok(());
            }
            if scope.revision < current.revision {
                // A text delayed past the HostSetup it preceded can still
                // belong to this room if it is at or after the revision the
                // setup replaced; an older one is the stale case.
                if super::cooperative::handles_mode(visit.room_mode())
                    && visit.campaign_may_apply(*scope, current)
                {
                    visit.apply_campaign(text)?;
                }
                return Ok(());
            }
            if scope.revision == current.revision
                && super::cooperative::handles_mode(visit.room_mode())
            {
                visit.apply_campaign(text)?;
            } else {
                // The options revision this text followed is not applied yet,
                // or this guest still holds the old non-Cooperative mode while
                // the host has already committed Cooperative. The next
                // HostSetup decides.
                visit.hold_campaign(*scope, text.to_owned());
            }
        }
    }
    Ok(())
}

/// A HostSetup transaction committed. The applied setup now owns or discards
/// every held text. A Cooperative setup applies the texts that were published
/// before it (in revision order, so a later text wins); a non-Cooperative
/// setup discards the texts it superseded. Texts for later revisions stay
/// held for their own setup.
pub(super) fn options_applied<V: CampaignVisit>(visit: &mut V) -> anyhow::Result<()> {
    let applied = visit.setup_scope();
    let held = visit.take_held_campaigns();
    visit.observe_campaign_setup(applied);
    if held.is_empty() {
        return Ok(());
    }
    let cooperative = super::cooperative::handles_mode(visit.room_mode());
    for (scope, text) in held {
        let Some(current) = applied.filter(|current| current.epoch == scope.epoch) else {
            continue;
        };
        if scope.revision > current.revision {
            visit.hold_campaign(scope, text);
            continue;
        }
        if cooperative && visit.campaign_may_apply(scope, current) {
            visit.apply_campaign(&text)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::frontend::skirmish_session::cooperative::{
        CooperativeProgressState, CooperativeRegistry,
    };
    use crate::app::frontend::skirmish_session::{SetupProfile, cooperative_runtime_for_test};
    use crate::app::match_runtime::session::{
        MatchSession, NetworkMapMetadata, NetworkReturnContext,
    };
    use crate::rules::ini_parser::IniFile;
    use crate::sim::rng::SimRng;
    use crate::skirmish_launch::{
        LaunchCountry, LaunchHumanRole, LaunchStartPosition, LaunchTeam, PreFillHouseRoster,
        SkirmishHumanSlot, SkirmishLaunchMode, SkirmishLaunchOptions, SkirmishLaunchSession,
        SkirmishLocalSlot,
    };
    use crate::skirmish_modes::MpModeClass::{Battle, Cooperative, Vmce};
    use crate::skirmish_modes::SkirmishGameMode;

    /// `C0,<campaign>,<stage>,<time>,<two tallies>,<variants>,-1,<names>`.
    /// Campaign 1 is `World` in the registry, stage 1, distinct counters so a
    /// first-stage default or a second record is visible.
    const EARLY_C0: &str = "C0,1,1,4321,10,20,30,40,1,2,3,4,0,0,-1,Host,Guest";
    /// A clearly different campaign/stage for stale-text tests: if an
    /// implementation applies it, the asserted description changes.
    const STALE_C0: &str = "C0,0,0,777,90,91,92,93,94,95,96,97,0,0,-1,Host,Guest";

    fn registry() -> CooperativeRegistry {
        CooperativeRegistry::from_ini(&IniFile::from_str(
            "[Campaigns]\n1=Allied\n2=World\n\
             [Allied]\nNumberOfCampaignMaps=2\n\
             CampaignPlayer1=Americans\nCampaignEnemy1=Russians\n\
             CampaignPlayer2=Americans\nCampaignEnemy2=Russians\n\
             Map1=A1,A1B,A1C\nMap2=A2,A2B,A2C\n\
             [World]\nNumberOfCampaignMaps=2\n\
             CampaignPlayer1=Americans\nCampaignEnemy1=Russians\n\
             CampaignPlayer2=Americans\nCampaignEnemy2=Russians\n\
             Map1=W1,W1B,W1C\nMap2=W2,W2B,W2C\n",
        ))
        .expect("Cooperative fixture")
    }

    fn mode(class: MpModeClass) -> SkirmishGameMode {
        SkirmishGameMode {
            class,
            id: match class {
                Cooperative => 7,
                Vmce => 8,
                _ => 1,
            },
            ui_name_key: "GUI:Test".to_owned(),
            tooltip_key: "STT:Test".to_owned(),
            override_file: String::new(),
            map_filter: "standard".to_owned(),
            random_maps_allowed: false,
            allies_allowed: true,
            must_ally: false,
            wonline_tournament_allowed: true,
            wonline_clan_tournament_allowed: true,
        }
    }

    fn human(name: &str, row: u8, country: LaunchCountry) -> SkirmishHumanSlot {
        SkirmishHumanSlot {
            raw_slot_index: Some(row),
            name: name.to_owned(),
            slot: SkirmishLocalSlot {
                country,
                country_random: false,
                color_index: row,
                color_random: false,
                start_position: LaunchStartPosition::Auto,
                team: LaunchTeam::None,
            },
            role: LaunchHumanRole::Player { group: 0 },
        }
    }

    /// A LAN guest's admitted two-player setup in `class`, with map metadata
    /// so the country rows exist as they do in a real BD/C9.
    fn session(class: MpModeClass) -> MatchSession {
        let mut launch = SkirmishLaunchSession {
            mode: SkirmishLaunchMode::from_game_mode(&mode(class)),
            selected_map_file: Some("c0.map".to_owned()),
            humans: vec![
                human("Host", 0, LaunchCountry::America),
                human("Guest", 1, LaunchCountry::Russia),
            ],
            local_player_index: 1,
            opponents: Vec::new(),
            ai_player_count: 0,
            pre_fill_house_roster: PreFillHouseRoster::from_compact_skirmish(0),
            options: SkirmishLaunchOptions::default(),
        };
        launch.rebuild_house_roster();
        let mut session = MatchSession::default();
        session.enter_network(NetworkReturnContext::LanLobby);
        session
            .accept_network_setup(launch, false, None, false)
            .expect("admitted guest setup");
        session.set_network_start_seed(7).expect("host seed");
        // A guest's HostSetup sets the mode-capped row capacity before the
        // map metadata arrives; `select_network_scenario` is that same
        // capacity owner.
        session
            .select_network_scenario("c0.map".to_owned(), 8)
            .expect("setup capacity");
        session
            .accept_network_map_metadata(
                NetworkMapMetadata {
                    display_name: "C0 map".to_owned(),
                    player_capacity: 8,
                    official: true,
                    byte_length: 1,
                    digest: "d".to_owned(),
                    source_sha256: String::new(),
                    effective_digest: String::new(),
                },
                None,
                true,
            )
            .expect("map metadata");
        session
    }

    fn scope(revision: u64) -> SessionScope {
        SessionScope {
            epoch: [7; 16],
            revision,
        }
    }

    struct Fixture {
        runtime: crate::app::frontend::skirmish_session::MultiplayerShellRuntime,
        session: MatchSession,
        inbox: CampaignInbox,
        current: Option<SessionScope>,
        room: MpModeClass,
        setup_dialog_open: bool,
    }

    impl Fixture {
        fn new(room: MpModeClass, revision: u64) -> Self {
            let registry = registry();
            let mut rng = SimRng::new(11);
            let progress = CooperativeProgressState::construct(&registry, &mut rng)
                .expect("preconstructed Cooperative records");
            Self {
                runtime: cooperative_runtime_for_test(registry, progress, rng, SetupProfile::Lan),
                session: session(room),
                inbox: CampaignInbox::default(),
                current: Some(scope(revision)),
                room,
                setup_dialog_open: true,
            }
        }

        fn deliver(&mut self, origin: CampaignOrigin, text: &str) -> anyhow::Result<()> {
            deliver(self, &origin, text)
        }

        fn options_applied(&mut self) -> anyhow::Result<()> {
            options_applied(self)
        }

        fn enter_cooperative_options(&mut self, revision: u64) {
            // The same admitted-setup transaction the guest's HostSetup runs,
            // with the mode and capacity the Cooperative launch carries.
            let mut launch = self
                .session
                .network_setup()
                .expect("admitted setup")
                .clone();
            launch.mode = SkirmishLaunchMode::from_game_mode(&mode(Cooperative));
            self.session
                .accept_network_host_setup(
                    launch,
                    [crate::app::match_runtime::session::NetworkVacancy::Open;
                        crate::skirmish_launch::NETWORK_PLAYER_SLOT_COUNT],
                    2,
                    None,
                    false,
                )
                .expect("Cooperative host setup");
            self.room = Cooperative;
            self.current = Some(scope(revision));
        }

        fn description(&self) -> Option<(i32, i32)> {
            self.runtime.cooperative_setup_view().description
        }

        fn countries(&self) -> Option<Vec<String>> {
            self.runtime
                .cooperative_setup_view()
                .player_countries
                .map(|countries| countries.iter().cloned().collect())
        }

        fn row_countries(&self) -> Vec<(LaunchCountry, bool)> {
            self.session
                .network_setup()
                .expect("admitted setup")
                .humans
                .iter()
                .map(|human| (human.slot.country, human.slot.country_random))
                .collect()
        }
    }

    impl CampaignVisit for Fixture {
        fn setup_scope(&self) -> Option<SessionScope> {
            self.current
        }

        fn room_mode(&self) -> Option<MpModeClass> {
            Some(self.room)
        }

        fn hold_campaign(&mut self, scope: SessionScope, text: String) {
            self.inbox.hold(scope, text);
        }

        fn take_held_campaigns(&mut self) -> Vec<(SessionScope, String)> {
            self.inbox.take()
        }

        fn campaign_may_apply(&self, scope: SessionScope, current: SessionScope) -> bool {
            self.inbox.may_apply(scope, current)
        }

        fn observe_campaign_setup(&mut self, current: Option<SessionScope>) {
            self.inbox.observe_setup(current);
        }

        fn apply_campaign(&mut self, text: &str) -> anyhow::Result<()> {
            crate::app::shell_network_setup::cooperative::apply_received_campaign(
                &mut self.runtime,
                &mut self.session,
                self.setup_dialog_open,
                text,
            )?;
            Ok(())
        }
    }

    /// An ordinary mode switches into Cooperative: the host publishes its
    /// `C0` before the HostSetup that carries the new mode, so the guest holds
    /// it. The Cooperative HostSetup then applies it before the mode entry,
    /// and the guest ends with campaign 1's stage, map and country rows.
    #[test]
    fn a_lan_campaign_before_the_cooperative_host_setup_reaches_the_guest() {
        let mut fixture = Fixture::new(Battle, 7);
        fixture
            .deliver(CampaignOrigin::Lan(scope(7)), EARLY_C0)
            .expect("early campaign delivery");
        assert_eq!(fixture.inbox.held_len(), 1, "the early C0 is held");
        assert_eq!(fixture.description(), None, "no Cooperative state yet");

        // The Cooperative options revision lands; its mode entry runs after
        // the held campaign as `0x005E541D`/`+0x20` does.
        fixture.enter_cooperative_options(8);
        fixture.options_applied().expect("apply held campaign");
        crate::app::shell_network_setup::cooperative::enter_mode_for(
            &mut fixture.runtime,
            &fixture.session,
            &[],
            0,
            false,
            false,
        );
        crate::app::shell_network_setup::cooperative::refresh_countries_for(
            &mut fixture.session,
            &fixture.runtime,
        )
        .expect("refresh the open setup");

        assert_eq!(fixture.inbox.held_len(), 0);
        assert_eq!(fixture.description(), Some((1, 1)));
        assert_eq!(fixture.countries(), Some(vec!["Americans".to_owned()]));
        assert_eq!(
            fixture.row_countries(),
            vec![
                (LaunchCountry::America, false),
                (LaunchCountry::America, false)
            ],
            "0x005C2500 writes the stage's single country to both rows"
        );
    }

    /// A WOL `C0` rides in the same options payload whose mode it names, so it
    /// applies before that payload's HostSetup even though this guest is still
    /// in an ordinary mode. This is the early order the WOL transport creates.
    #[test]
    fn a_wol_campaign_applies_before_the_cooperative_host_setup() {
        let mut fixture = Fixture::new(Battle, 7);
        fixture
            .deliver(CampaignOrigin::Options(Cooperative), EARLY_C0)
            .expect("options-owned campaign");
        assert_eq!(fixture.inbox.held_len(), 0);
        assert_eq!(fixture.description(), Some((1, 1)));
        assert_eq!(fixture.countries(), Some(vec!["Americans".to_owned()]));
    }

    /// The reordered form: the Cooperative HostSetup arrives before the
    /// transition's `C0`. The text was published under the previous setup
    /// revision, which the committed setup recorded as its floor, so it still
    /// belongs to the room and applies.
    #[test]
    fn a_lan_campaign_delayed_past_the_cooperative_host_setup_applies() {
        let mut fixture = Fixture::new(Battle, 7);
        fixture
            .options_applied()
            .expect("the old room's setup is observed");
        fixture.enter_cooperative_options(8);
        fixture
            .options_applied()
            .expect("the Cooperative setup is observed");
        fixture
            .deliver(CampaignOrigin::Lan(scope(7)), EARLY_C0)
            .expect("late transition campaign");
        assert_eq!(fixture.inbox.held_len(), 0);
        assert_eq!(fixture.description(), Some((1, 1)));
        assert_eq!(fixture.countries(), Some(vec!["Americans".to_owned()]));
        fixture
            .deliver(CampaignOrigin::Lan(scope(6)), STALE_C0)
            .expect("older campaign");
        assert_eq!(
            fixture.description(),
            Some((1, 1)),
            "a text before the setup's floor is stale"
        );
    }

    /// A repeated application of the same options revision (a roster or
    /// player-choices receipt runs `reapply_received_options`) is not a new
    /// setup and must not raise the floor under a transition text still in
    /// flight.
    #[test]
    fn a_repeated_host_setup_keeps_the_floor_for_a_delayed_transition_text() {
        let mut fixture = Fixture::new(Battle, 7);
        fixture
            .options_applied()
            .expect("the old room's setup is observed");
        fixture.enter_cooperative_options(8);
        fixture
            .options_applied()
            .expect("the Cooperative setup is observed");
        fixture
            .options_applied()
            .expect("the same revision is re-applied for a roster");
        fixture
            .deliver(CampaignOrigin::Lan(scope(7)), EARLY_C0)
            .expect("late transition campaign");
        assert_eq!(
            fixture.description(),
            Some((1, 1)),
            "the re-application kept the floor for the transition text"
        );
    }

    /// The ordinary order: the Cooperative HostSetup is already applied when
    /// its `C0` arrives, so the same revision's text is applied at once.
    #[test]
    fn a_lan_campaign_after_the_cooperative_host_setup_applies_at_once() {
        let mut fixture = Fixture::new(Cooperative, 8);
        fixture
            .deliver(CampaignOrigin::Lan(scope(8)), EARLY_C0)
            .expect("ordinary campaign delivery");
        assert_eq!(fixture.inbox.held_len(), 0);
        assert_eq!(fixture.description(), Some((1, 1)));
        assert_eq!(fixture.countries(), Some(vec!["Americans".to_owned()]));
    }

    /// A non-Cooperative room's C0 — a same-revision delayed text or a
    /// payload-owned WOL text — never enters Cooperative's campaign state. A
    /// stale revision is discarded without even a hold.
    #[test]
    fn a_non_cooperative_room_never_applies_a_cooperative_campaign() {
        let mut fixture = Fixture::new(Vmce, 9);
        fixture
            .deliver(CampaignOrigin::Options(Vmce), EARLY_C0)
            .expect("inapplicable WOL campaign");
        assert_eq!(fixture.inbox.held_len(), 0, "a WOL text is not held");
        assert_eq!(fixture.description(), None);

        fixture
            .deliver(CampaignOrigin::Lan(scope(9)), EARLY_C0)
            .expect("delayed LAN campaign");
        assert_eq!(
            fixture.inbox.held_len(),
            1,
            "a delayed text waits for its setup"
        );
        fixture
            .options_applied()
            .expect("the VMCE setup resolves the hold");
        assert_eq!(fixture.inbox.held_len(), 0);
        assert_eq!(
            fixture.description(),
            None,
            "the VMCE setup discards the inapplicable text"
        );

        fixture
            .deliver(CampaignOrigin::Lan(scope(8)), EARLY_C0)
            .expect("superseded campaign");
        assert_eq!(fixture.inbox.held_len(), 0, "an old revision is not held");
        assert_eq!(fixture.description(), None);
    }

    /// Cooperative -> VMCE -> Cooperative: a text from the first Cooperative
    /// setup is stale after the VMCE revision, and the next Cooperative setup
    /// accepts only its own revision's text.
    #[test]
    fn a_mode_switch_again_discards_the_old_rooms_campaign() {
        let mut fixture = Fixture::new(Cooperative, 7);
        fixture
            .deliver(CampaignOrigin::Lan(scope(7)), EARLY_C0)
            .expect("first Cooperative campaign");
        assert_eq!(fixture.description(), Some((1, 1)));

        fixture.room = Vmce;
        fixture.current = Some(scope(8));
        fixture.options_applied().expect("VMCE options");
        fixture
            .deliver(CampaignOrigin::Lan(scope(7)), EARLY_C0)
            .expect("stale text after the switch");
        assert_eq!(fixture.inbox.held_len(), 0);
        assert_eq!(
            fixture.description(),
            Some((1, 1)),
            "the stale text changes nothing"
        );

        fixture.enter_cooperative_options(9);
        fixture
            .options_applied()
            .expect("Cooperative options again");
        fixture
            .deliver(
                CampaignOrigin::Lan(scope(9)),
                "C0,0,0,9,5,6,7,8,1,2,3,4,0,0,-1,Host,Guest",
            )
            .expect("new campaign");
        assert_eq!(fixture.description(), Some((0, 0)));
        // The old Cooperative revision is below the floor set by the VMCE
        // revision that replaced it, even though the room is Cooperative
        // again: a wrong implementation would apply this distinct text.
        fixture
            .deliver(CampaignOrigin::Lan(scope(7)), EARLY_C0)
            .expect("old room's campaign after returning");
        assert_eq!(
            fixture.description(),
            Some((0, 0)),
            "the first Cooperative room's text stays stale after the switch"
        );
        assert_eq!(fixture.inbox.held_len(), 0);
    }

    /// Leaving Cooperative retires both the held text and a `C0` still waiting
    /// in the protocol's publication queue; the next room cannot publish or
    /// apply the old one.
    #[test]
    fn leaving_cooperative_retires_held_and_queued_campaigns() {
        let mut protocol = crate::app::shell_network_setup::NetworkSetupProtocol::new(
            crate::ui::multiplayer_setup::SetupDialog {
                host: true,
                flavor: crate::ui::multiplayer_setup::SetupFlavor::Lan,
                modern: true,
            },
            false,
        );
        protocol.hold_cooperative_campaign(scope(7), EARLY_C0.to_owned());
        protocol.requests.push(
            crate::app::shell_network_setup::NetworkSetupRequest::PublishCooperativeCampaign(
                EARLY_C0.to_owned(),
            ),
        );

        protocol.discard_cooperative_campaigns();

        assert!(
            protocol
                .take_service_requests()
                .iter()
                .all(|request| !matches!(
                    request,
                    crate::app::shell_network_setup::NetworkSetupRequest::PublishCooperativeCampaign(
                        _
                    )
                )),
            "a queued C0 publication survived the mode change"
        );
        assert!(
            protocol.take_held_cooperative_campaigns().is_empty(),
            "a held C0 survived the mode change"
        );
    }

    /// Leaving the room drops whatever the next HostSetup still owned; a
    /// later delivery cannot revive it.
    #[test]
    fn leaving_the_room_clears_a_held_campaign() {
        let mut fixture = Fixture::new(Battle, 7);
        fixture
            .deliver(CampaignOrigin::Lan(scope(8)), EARLY_C0)
            .expect("held campaign");
        assert_eq!(fixture.inbox.held_len(), 1);
        fixture.inbox.clear();
        fixture.enter_cooperative_options(9);
        fixture.options_applied().expect("after leaving");
        assert_eq!(fixture.description(), None);
    }
}
