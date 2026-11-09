//! The LAN Cooperative result dialog's round state: BC6's readiness bytes
//! `0x89E000[8]`, the local ready flag `0x89E05C`, the ack count
//! `0x89E058`, the begin flag and the loop's result `0x89E04C`, held
//! with the round's roster as one network phase.
//!
//! Bodies: the loop `CampaignScoreClass vt+4` (`0x0046FC00`) and
//! `Dialog0xBC6__DlgProc` (`0x0046DFD0`). Peers arrive as decoded
//! [`CooperativeRoundEvent`]s and every packet this side sends is a
//! [`NetworkLoadingRequest`]; `lan_service::cooperative` moves both.
//!
//! This is the generic LAN Cooperative protocol (BC6's ready/continue/begin
//! exchange). VMCE projections live behind the narrow
//! [`super::vmce_round`] facade: the four-row defeat page, the 0x39 retry-vote
//! record and its host gate. No VMCE-only state machine exists here; the
//! shared round carries the `VMCE` flag and ending projection, and VMCE
//! callers delegate through the facade.

use super::{
    MatchSession, NetworkLoadingRequest, NetworkPhase, NetworkReturnContext, NetworkSession,
};
use crate::skirmish_launch::{
    LaunchStartPosition, SkirmishAiSlot, SkirmishLaunchSession, SkirmishLocalSlot,
};
use std::sync::Arc;

/// BC6's round state, owned by the `CooperativeRound` phase. The row
/// status statics (`TXT_READY`/`TXT_WAITING`/`TXT_ABORTED` in
/// `0x70A`/`0x70B`) are template-hidden and never shown, so their texts are
/// not kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CooperativeRoundState {
    /// `0x89E000[8]`, by raw slot: empty slots start ready
    /// (`0x0046EF06..0x0046EF4A`), and every un-ready pass clears human
    /// slots only.
    ready: [bool; 8],
    /// `0x89E05C`.
    local_ready: bool,
    /// The master assigned the begin/options once; a repeated Continue must
    /// not repeat the begin or count its own ack twice.
    assigned: bool,
    /// Per raw slot: that participant acked the begin. A repeated ack is a
    /// no-op, so the count never passes `rows` from a duplicated packet.
    acked: [bool; 8],
    /// The loop's local begin flag (`[ESP+0x13]`), set by `0x34`.
    begin: bool,
    /// Quit `0x5C0`'s enable state: the `0x32` arm disables it once the
    /// begin arrived (`0x0047086E`), and the `0x24` arm enables it again
    /// (`0x00470583`).
    quit_enabled: bool,
    /// `0x89E05D`: false on the campaign's last mission.
    can_continue: bool,
    /// `A8D580`, the human House count of the mission's rows.
    rows: usize,
    /// Authored VMCE: the host is auto-ready and only its Continue
    /// starts the next stage; a guest Continue is its ready action.
    vera: bool,
    /// Authored VMCE: the next stage this peer resolved when the round
    /// opened. Both peers compare it against the host's `0x36` record before
    /// either loads the mission; `None` on stock Cooperative and on a VMCE
    /// last mission.
    #[cfg(feature = "vmce")]
    vera_stage: Option<crate::app::match_bootstrap::VmceStageIdentity>,
    /// `0x89E04C` once the loop left: true runs the next mission.
    outcome: Option<bool>,
    /// Authored VMCE: the page's verdict. The 0x103 layout is shared by win
    /// and defeat, so this is the authoritative record for input and receipts.
    #[cfg(feature = "vmce")]
    page_victory: bool,
    /// Authored VMCE: what Continue starts. `Next` is the existing next
    /// stage, `CampaignEnd` plays the campaign's completion tail, and
    /// `Retry` re-enters the same frozen stage.
    vera_ending: VmceRoundEnding,
    /// Authored VMCE defeat page: per raw slot, that guest voted Retry. The
    /// host starts only once every online guest has voted; the host itself
    /// never votes. Disconnect clears the slot's vote.
    #[cfg(feature = "vmce")]
    retry_votes: [bool; 4],
}

/// VMCE's result-page continuation. Kept in the round so every peer's
/// begin/ack tail reaches the same local step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmceRoundEnding {
    Next,
    CampaignEnd,
    Retry,
}

fn vera_result_step(round: &CooperativeRoundState) -> CooperativeRoundStep {
    match round.vera_ending {
        VmceRoundEnding::Next => CooperativeRoundStep::Exit(true),
        VmceRoundEnding::CampaignEnd => CooperativeRoundStep::CampaignEnd,
        VmceRoundEnding::Retry => CooperativeRoundStep::Retry,
    }
}

/// VMCE's next-mission seed: a deterministic mix of the previously agreed
/// seed and the resolved next stage's locally verified identity. Every peer
/// derives it from facts it already holds, so no extra field travels; the
/// same stage always yields the same seed, and a different mission does not
/// replay the previous mission's random stream.
#[cfg(feature = "vmce")]
fn next_stage_seed(previous: u32, next: &crate::app::match_bootstrap::VmceNextStage) -> u32 {
    let digest = crate::util::sha256::sha256_hex(
        format!(
            "vera-vmce-next-seed\n{previous}\n{}\n{}\n{}",
            next.map_file, next.config.source_sha256, next.effective_digest
        )
        .as_bytes(),
    );
    u32::from_str_radix(&digest[..8], 16).expect("sha256 hex prefix")
}

impl CooperativeRoundState {
    pub(crate) fn local_ready(&self) -> bool {
        self.local_ready
    }

    /// Authored VMCE: the host is auto-ready and only its Continue starts the
    /// next stage; a guest Continue is its ready action.
    pub(crate) fn vera(&self) -> bool {
        self.vera
    }

    pub(crate) fn quit_enabled(&self) -> bool {
        self.outcome.is_none() && self.quit_enabled
    }

    /// Authored VMCE: the page's verdict.
    #[cfg(feature = "vmce")]
    pub(crate) fn page_victory(&self) -> bool {
        self.page_victory
    }

    /// Authored VMCE: what this page's Continue starts.
    #[cfg(feature = "vmce")]
    pub(crate) fn vera_ending(&self) -> VmceRoundEnding {
        self.vera_ending
    }

    /// Authored VMCE defeat: this slot's Retry vote.
    #[cfg(feature = "vmce")]
    pub(crate) fn retry_voted(&self, row: usize) -> bool {
        self.retry_votes.get(row).copied().unwrap_or(false)
    }

    fn all_ready(&self) -> bool {
        self.ready.iter().all(|ready| *ready)
    }
}

/// The master's options packet `0x32` (`0x0046D4A0`) as the transport
/// decodes it: the difficulty byte `+0x2F`, each human slot's start,
/// country and colour (`+0x30`/`+0x50`/`+0x70`), and the AI arrays
/// (`+0x90..+0x110`) as the AI rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CooperativeRoundOptions {
    pub(crate) difficulty: i32,
    /// `(raw slot, choices)` of every human row.
    pub(crate) humans: Vec<(u8, SkirmishLocalSlot)>,
    pub(crate) ai_player_count: i32,
    pub(crate) opponents: Vec<SkirmishAiSlot>,
}

/// A decoded BC6 packet from participant `source`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CooperativeRoundEvent {
    /// `0x26`.
    PeerReady { source: usize },
    /// `0x25`.
    PeerWaiting { source: usize },
    /// `0x24`.
    PeerLeft { source: usize },
    /// `0x34`, the master's begin.
    Begin,
    /// `0x32`.
    Options(CooperativeRoundOptions),
    /// `0x23`, a guest's ack of the begin.
    Ack { source: usize },
    /// `0x31`, the master's go.
    Go,
    /// `0x39`, VERA-only: a guest's retry vote on the BC6 defeat page.
    #[cfg(feature = "vmce")]
    RetryVote { source: usize },
    /// `0x0C` of this game (its `+0x114` hash matched), with the line
    /// colour at `+0x110`.
    Chat {
        source: usize,
        text: String,
        color: u32,
    },
    /// `0x0D`, the 20 s keepalive: only the loop's tail reads it.
    KeepAlive,
    /// VERA-only `0x36`: the host's resolved next stage identity.
    #[cfg(feature = "vmce")]
    StageIdentity(crate::app::match_bootstrap::VmceStageIdentity),
    /// VERA-only `0x38`: this peer resolved no next stage (defeat or the
    /// campaign's last mission).
    #[cfg(feature = "vmce")]
    StageAbsent,
    /// VERA-only `0x37`: a peer's next stage differs, so the round is
    /// abandoned for the lobby instead of loading different missions.
    #[cfg(feature = "vmce")]
    StageRejected,
}

/// What the app must do after a round step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CooperativeRoundStep {
    None,
    /// The master's all-ready (`0x0046FD47`): run mode `+0x68` and
    /// `0x0069B8C0`, then [`MatchSession::cooperative_round_assigned`],
    /// which queues `0x34` and `0x32`.
    Assign,
    /// A `0x32` was applied; `difficulty` is its byte, and `changed` cleared
    /// every human's readiness.
    OptionsApplied {
        difficulty: i32,
        changed: bool,
    },
    /// `0x24`: the row shows `TXT_ABORTED`; post `GUI:PlayerLeftCoop`.
    PeerLeft {
        source: usize,
    },
    /// `0x0C`: post the line in the packet's colour.
    Chat {
        source: usize,
        text: String,
        color: u32,
    },
    /// The loop left with `0x89E04C`.
    Exit(bool),
    /// VERA: Continue on WOL left with result 0 for the room the players
    /// return to, which its host sets to the next stage.
    ContinueInRoom,
    /// VMCE last mission: play the campaign completion tail (credits) on
    /// every peer after the all-ready gate.
    CampaignEnd,
    /// VMCE defeat: re-enter the same frozen stage without a ready gate.
    Retry,
}

impl MatchSession {
    /// GameExit's co-op arm enters BC6 from the finished match
    /// (`0x0046FC00` setup and the 0x497 init `0x0046EBA7`). The round
    /// takes the frozen roster with `next_map_file` (`+0xA0`'s
    /// SelectScenario) and every human's and AI's start reset to Random
    /// (`0x00696FF0(-2)`, `A8B2DC[i] = -2`). `rows` is the mission's human
    /// House count.
    pub(crate) fn enter_cooperative_round(
        &mut self,
        next_map_file: Option<String>,
        can_continue: bool,
        rows: usize,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "vmce")]
        return self.enter_cooperative_round_with_vera(next_map_file, can_continue, rows, None);
        #[cfg(not(feature = "vmce"))]
        return self.enter_cooperative_round_with_vera(next_map_file, can_continue, rows);
    }

    /// VMCE's BC6 entry: the next external campaign stage was resolved
    /// locally by both peers, so carry it through the round and into the
    /// frozen next start.
    #[cfg(feature = "vmce")]
    pub(crate) fn enter_vmce_round(
        &mut self,
        next: Option<crate::app::match_bootstrap::VmceNextStage>,
        can_continue: bool,
        rows: usize,
    ) -> anyhow::Result<()> {
        let map_file = next.as_ref().map(|stage| stage.map_file.clone());
        self.enter_cooperative_round_with_vera(map_file, can_continue, rows, next)
    }

    /// Enter the VMCE result round with an explicit continuation: campaign
    /// end or a same-stage retry instead of the next stage.
    #[cfg(feature = "vmce")]
    pub(crate) fn enter_vmce_round_ending(
        &mut self,
        next: Option<crate::app::match_bootstrap::VmceNextStage>,
        can_continue: bool,
        ending: VmceRoundEnding,
        rows: usize,
    ) -> anyhow::Result<()> {
        self.enter_vmce_round(next, can_continue, rows)?;
        if let Some(super::NetworkSession {
            phase: super::NetworkPhase::CooperativeRound { round, .. },
            ..
        }) = self.network.as_mut()
        {
            round.vera_ending = ending;
        }
        Ok(())
    }

    fn enter_cooperative_round_with_vera(
        &mut self,
        next_map_file: Option<String>,
        can_continue: bool,
        rows: usize,
        #[cfg(feature = "vmce")] vera_next: Option<crate::app::match_bootstrap::VmceNextStage>,
    ) -> anyhow::Result<()> {
        let network = self
            .network
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
        let NetworkPhase::Frozen { startup, connected } = &network.phase else {
            anyhow::bail!("the co-op round follows a frozen match");
        };
        let mut launch = startup.launch_session().clone();
        if let Some(file) = next_map_file {
            launch.selected_map_file = Some(file);
        }
        for human in &mut launch.humans {
            human.slot.start_position = LaunchStartPosition::Auto;
        }
        // RESIDUAL: the 0x497 init (`0x0046EBA7`) also writes every AI
        // country and colour to -1; only the starts reset here. Rows below
        // the next map's NumCoopHumanStartSpots keep their last values in
        // `prepare_ai`, which changes the `0x0069B8C0` draws only when that
        // count grows between missions. Every retail co-op map sets 2.
        for ai in &mut launch.opponents {
            ai.start_position = LaunchStartPosition::Auto;
        }
        launch.rebuild_house_roster();
        // 0x0046EF06: empty slots ready, every bound human slot not.
        let mut ready = [true; 8];
        for (index, human) in launch.humans.iter().enumerate() {
            if let Some(row) = human.raw_slot_index
                && connected.contains(&index)
            {
                ready[usize::from(row)] = false;
            }
        }
        let vera = launch.mode.class.is_vmce();
        #[cfg(feature = "vmce")]
        let vera_stage = crate::app::match_bootstrap::VmceStageIdentity::of(vera_next.as_ref());
        let mut round = CooperativeRoundState {
            ready,
            local_ready: false,
            assigned: false,
            acked: [false; 8],
            begin: false,
            quit_enabled: true,
            can_continue,
            rows,
            vera,
            #[cfg(feature = "vmce")]
            vera_stage: vera_stage.clone(),
            #[cfg(feature = "vmce")]
            page_victory: false,
            vera_ending: VmceRoundEnding::Next,
            #[cfg(feature = "vmce")]
            retry_votes: [false; 4],
            outcome: None,
        };
        let host = self.host;
        if vera && host {
            // VMCE's authoring decision: the host is ready from the
            // start and waits for the guests; only its Continue starts the
            // next stage.
            round.local_ready = true;
            if let Some(row) = launch
                .humans
                .get(launch.local_player_index)
                .and_then(|human| human.raw_slot_index)
            {
                round.ready[usize::from(row)] = true;
            }
        }
        network.phase = NetworkPhase::CooperativeRound {
            previous: Arc::clone(startup),
            connected: connected.clone(),
            launch,
            round,
            #[cfg(feature = "vmce")]
            vera_next,
        };
        network.loading_requests.clear();
        if vera && host {
            // Let every guest see the host's auto-ready row.
            network
                .loading_requests
                .push(NetworkLoadingRequest::CooperativeReady);
        }
        // Every VMCE peer advertises whether it resolved a next stage before
        // the native begin: whichever identity arrives first exposes a
        // mismatch, so neither side loads a different mission.
        #[cfg(feature = "vmce")]
        {
            if let Some(stage) = vera_stage {
                network
                    .loading_requests
                    .push(NetworkLoadingRequest::CooperativeStage(stage));
            } else if vera {
                network
                    .loading_requests
                    .push(NetworkLoadingRequest::CooperativeStageAbsent);
            }
        }
        Ok(())
    }

    /// The repaint's country pass `0x0046F6D0`: each human slot's country
    /// goes through `admit`, which may replace a country the next stage
    /// does not list (`SetCountry`, a state write).
    pub(crate) fn admit_cooperative_round_countries(
        &mut self,
        admit: impl Fn(&mut SkirmishLocalSlot),
    ) -> anyhow::Result<()> {
        let RoundParts { launch, .. } = round_parts(self)?;
        for human in &mut launch.humans {
            admit(&mut human.slot);
        }
        launch.rebuild_house_roster();
        Ok(())
    }

    /// The master's bound human slots begin unready, and `0x89E000` is a
    /// slot array: whether `source_index`'s slot is ready.
    pub(crate) fn cooperative_round_slot_ready(&self, source_index: usize) -> bool {
        match self.network.as_ref().map(|network| &network.phase) {
            Some(NetworkPhase::CooperativeRound { launch, round, .. }) => launch
                .humans
                .get(source_index)
                .and_then(|human| human.raw_slot_index)
                .is_some_and(|row| round.ready[usize::from(row)]),
            _ => false,
        }
    }

    pub(crate) fn cooperative_round(&self) -> Option<&CooperativeRoundState> {
        match &self.network.as_ref()?.phase {
            NetworkPhase::CooperativeRound { round, .. } => Some(round),
            _ => None,
        }
    }

    /// Record the verdict that opened the authored VMCE page.
    #[cfg(feature = "vmce")]
    pub(crate) fn set_cooperative_round_page_victory(&mut self, victory: bool) {
        if let Some(network) = self.network.as_mut()
            && let NetworkPhase::CooperativeRound { round, .. } = &mut network.phase
        {
            round.page_victory = victory;
        }
    }

    /// `0x89E05D`: whether the next stage exists. The native campaign's last
    /// mission sets this false; VMCE keeps the same gate for its 0x103 page.
    pub(crate) fn cooperative_round_can_continue(&self) -> bool {
        self.cooperative_round()
            .is_some_and(|round| round.can_continue)
    }

    /// The host's `0x0046EF06` readiness rows as `(ready, total, local)`.
    /// Empty slots are always ready; only connected, non-observer humans
    /// count toward the status shown by VMCE's victory page.
    pub(crate) fn cooperative_round_ready_progress(&self) -> Option<(usize, usize, bool)> {
        let Some(NetworkPhase::CooperativeRound {
            connected,
            launch,
            round,
            ..
        }) = self.network.as_ref().map(|network| &network.phase)
        else {
            return None;
        };
        let mut ready = 0;
        let mut total = 0;
        for (index, human) in launch.humans.iter().enumerate() {
            if human.is_observer() || !connected.contains(&index) {
                continue;
            }
            let Some(row) = human.raw_slot_index else {
                continue;
            };
            total += 1;
            ready += usize::from(round.ready[usize::from(row)]);
        }
        Some((ready, total, round.local_ready()))
    }

    /// `0x0046FB50`: Continue is enabled while every row's player is still
    /// connected and the local player is not ready. VMCE's host is
    /// auto-ready, so its Continue enables only after every guest is ready;
    /// a guest's Continue remains its ready action.
    pub(crate) fn cooperative_round_continue_enabled(&self) -> bool {
        match self.network.as_ref().map(|network| &network.phase) {
            Some(NetworkPhase::CooperativeRound {
                connected, round, ..
            }) => {
                if round.outcome.is_some() {
                    return false;
                }
                if round.vera && !round.can_continue {
                    // Defeat and the last mission need no next-stage gate:
                    // the page only acknowledges and leaves, so a peer that
                    // already left must not disable the remaining Continue.
                    return true;
                }
                if connected.len() < round.rows {
                    return false;
                }
                if round.vera {
                    if !round.can_continue {
                        // A defeat or the last mission has no next stage to
                        // wait for: Continue acknowledges the page and leaves.
                        true
                    } else if self.host {
                        // Retry is the host's decision and needs no guest
                        // readiness; campaign end keeps the all-ready gate.
                        (round.vera_ending == VmceRoundEnding::Retry || round.all_ready())
                            && !round.assigned
                    } else {
                        !round.local_ready
                    }
                } else {
                    !round.local_ready
                }
            }
            _ => false,
        }
    }

    /// Continue `0x6D1` (`0x0046E743`) and the loop's slot-1 pass
    /// (`0x0046FD61`). The button disables itself once ready, so only the
    /// ready arm is reachable; `difficulty` is `[0xA8EB68]` for the
    /// master's `0x32`.
    /// VERA: a WOL match's relay ends with its mission, so BC6's records
    /// have no transport and `+0xAC` no session to continue. A round with a
    /// stage left leaves at once with result 0 for the players' room.
    pub(crate) fn continue_cooperative_round(
        &mut self,
        difficulty: i32,
    ) -> anyhow::Result<CooperativeRoundStep> {
        let host = self.host;
        let online = self
            .network
            .as_ref()
            .is_some_and(|network| network.return_context == NetworkReturnContext::Wol);
        let RoundParts {
            requests,
            connected: _,
            launch,
            round,
        } = round_parts(self)?;
        anyhow::ensure!(round.outcome.is_none(), "the result dialog has closed");
        if round.vera {
            if !round.can_continue {
                // Defeat and the last mission: every peer acknowledges the
                // page and leaves for the lobby, with no ready/begin gate.
                round.outcome = Some(false);
                requests.push(NetworkLoadingRequest::CooperativeLeave);
                return Ok(CooperativeRoundStep::Exit(false));
            }
            if host && round.rows == 1 {
                // A solo host has no guest row to ack its begin, so Continue
                // starts the next stage directly instead of waiting forever.
                round.outcome = Some(true);
                return Ok(vera_result_step(round));
            }
            if round.vera_ending == VmceRoundEnding::Retry {
                // Defeat retry is a host decision; guests follow the begin
                // exchange without pressing Continue.
                if host {
                    return Ok(CooperativeRoundStep::Assign);
                }
                return Ok(CooperativeRoundStep::None);
            }
        }
        if online && round.can_continue {
            round.outcome = Some(false);
            return Ok(CooperativeRoundStep::ContinueInRoom);
        }
        if round.vera && host {
            // Host auto-ready: Continue is the start gate, not a readiness
            // write. The button is enabled only once every guest is ready;
            // cooperative_round_assigned sends the begin and options.
            anyhow::ensure!(
                round.can_continue && round.all_ready() && !round.assigned,
                "the host Continue waits for every guest"
            );
            log::debug!("VMCE round: host Continue pressed; starting the next stage");
            return Ok(CooperativeRoundStep::Assign);
        }
        let local = launch.local_player_index;
        round.local_ready = true;
        if round.vera {
            log::debug!("VMCE round: guest ready pressed");
        }
        if let Some(row) = launch
            .humans
            .get(local)
            .and_then(|human| human.raw_slot_index)
        {
            round.ready[usize::from(row)] = true;
        }
        if host && round.can_continue {
            let options = round_options(launch, difficulty);
            requests.push(NetworkLoadingRequest::CooperativeOptions(options));
        }
        requests.push(NetworkLoadingRequest::CooperativeReady);
        if host && round.all_ready() {
            if !round.can_continue {
                round.outcome = Some(false);
                return Ok(CooperativeRoundStep::Exit(false));
            }
            return Ok(CooperativeRoundStep::Assign);
        }
        if !round.can_continue {
            round.outcome = Some(false);
            return Ok(CooperativeRoundStep::Exit(false));
        }
        Ok(CooperativeRoundStep::None)
    }

    /// The master's all-ready tail after mode `+0x68` and `0x0069B8C0`
    /// changed the round's roster: the begin `0x34`, its own ack and the
    /// options `0x32`. Native sends `0x34` before the assignment, which
    /// cannot fail there; VERA queues it only once the assignment
    /// succeeded, so a refused one sends no begin. Both leave in the same
    /// order before any packet is read.
    pub(crate) fn cooperative_round_assigned(&mut self, difficulty: i32) -> anyhow::Result<()> {
        anyhow::ensure!(self.host, "only the master assigns the round");
        let RoundParts {
            requests,
            connected: _,
            launch,
            round,
        } = round_parts(self)?;
        // A repeated Continue must not send a second begin or count the
        // host's own ack twice.
        if round.assigned {
            return Ok(());
        }
        round.assigned = true;
        requests.push(NetworkLoadingRequest::CooperativeBegin);
        if let Some(row) = launch
            .humans
            .get(launch.local_player_index)
            .and_then(|human| human.raw_slot_index)
        {
            round.acked[usize::from(row)] = true;
        }
        let options = round_options(launch, difficulty);
        requests.push(NetworkLoadingRequest::CooperativeOptions(options));
        Ok(())
    }

    /// Quit `0x5C0` (`0x0046E28C`): `0x24` to every node and result 0.
    pub(crate) fn quit_cooperative_round(&mut self) -> anyhow::Result<()> {
        let RoundParts {
            requests, round, ..
        } = round_parts(self)?;
        if round.outcome.is_none() {
            round.outcome = Some(false);
            requests.push(NetworkLoadingRequest::CooperativeLeave);
        }
        Ok(())
    }

    /// VERA-only BC6 defeat Continue, guest side: cast this guest's one
    /// retry vote. A repeated vote is a no-op (the lit button is not
    /// clickable again); the host starts only once every online guest voted.
    #[cfg(feature = "vmce")]
    pub(crate) fn cooperative_round_retry_vote(&mut self) -> anyhow::Result<CooperativeRoundStep> {
        anyhow::ensure!(!self.host, "the host does not cast a retry vote");
        let RoundParts {
            requests,
            launch,
            round,
            ..
        } = round_parts(self)?;
        anyhow::ensure!(
            round.vera && round.vera_ending == VmceRoundEnding::Retry,
            "not the VMCE retry page"
        );
        if round.outcome.is_some() {
            return Ok(CooperativeRoundStep::None);
        }
        let Some(row) = launch
            .humans
            .get(launch.local_player_index)
            .and_then(|human| human.raw_slot_index)
            .map(usize::from)
            .filter(|row| *row < 4)
        else {
            anyhow::bail!("the local guest has no raw slot");
        };
        if round.retry_votes[row] {
            return Ok(CooperativeRoundStep::None);
        }
        round.retry_votes[row] = true;
        requests.push(NetworkLoadingRequest::CooperativeRetryVote);
        Ok(CooperativeRoundStep::None)
    }

    /// VERA-only BC6 defeat Continue, host side: start the retry once every
    /// connected guest voted. The host never votes itself; a solo host is
    /// trivially all-voted. The existing retry step is used unchanged.
    #[cfg(feature = "vmce")]
    pub(crate) fn cooperative_round_retry_start(
        &mut self,
        difficulty: i32,
    ) -> anyhow::Result<CooperativeRoundStep> {
        anyhow::ensure!(self.host, "only the host starts the retry");
        {
            let RoundParts {
                connected,
                launch,
                round,
                ..
            } = round_parts(self)?;
            anyhow::ensure!(
                round.vera && round.vera_ending == VmceRoundEnding::Retry,
                "not the VMCE retry page"
            );
            anyhow::ensure!(
                retry_guests_voted(launch, connected, round),
                "the host retry waits for every online guest"
            );
        }
        self.continue_cooperative_round(difficulty)
    }

    /// Capture/test fixture: install the per-slot retry votes on the open
    /// round. Never called by gameplay; the vote events own production data.
    #[cfg(feature = "vmce")]
    pub(crate) fn cooperative_round_set_retry_votes(
        &mut self,
        votes: [bool; 4],
    ) -> anyhow::Result<()> {
        let RoundParts { round, .. } = round_parts(self)?;
        round.retry_votes = votes;
        Ok(())
    }

    /// VERA-only BC6 defeat page: `(is_host, lit, enabled)` for the Retry
    /// button. A guest lights and disables on its own vote; the host lights
    /// and enables only after every online guest voted.
    #[cfg(feature = "vmce")]
    pub(crate) fn cooperative_round_retry_state(&self) -> Option<(bool, bool, bool)> {
        let NetworkPhase::CooperativeRound {
            connected,
            launch,
            round,
            ..
        } = &self.network.as_ref()?.phase
        else {
            return None;
        };
        if !round.vera || round.vera_ending != VmceRoundEnding::Retry || round.outcome.is_some() {
            return None;
        }
        if self.host {
            let voted = retry_guests_voted(launch, connected, round);
            Some((true, voted, voted))
        } else {
            let voted = launch
                .humans
                .get(launch.local_player_index)
                .and_then(|human| human.raw_slot_index)
                .is_some_and(|row| round.retry_voted(usize::from(row)));
            Some((false, voted, !voted))
        }
    }

    /// One packet through the loop's dispatch (`0x0047009F`).
    /// `difficulty` is the current `[0xA8EB68]`, which `0x32` compares.
    pub(crate) fn cooperative_round_event(
        &mut self,
        event: CooperativeRoundEvent,
        difficulty: i32,
    ) -> anyhow::Result<CooperativeRoundStep> {
        let host = self.host;
        let RoundParts {
            requests,
            connected,
            launch,
            round,
        } = round_parts(self)?;
        if round.outcome.is_some() {
            return Ok(CooperativeRoundStep::None);
        }
        let slot_of = |launch: &SkirmishLaunchSession, source: usize| {
            launch
                .humans
                .get(source)
                .and_then(|human| human.raw_slot_index)
                .map(usize::from)
        };
        match event {
            #[cfg(feature = "vmce")]
            CooperativeRoundEvent::StageIdentity(advertised) => {
                if round.vera_stage.as_ref() == Some(&advertised) {
                    return Ok(CooperativeRoundStep::None);
                }
                Ok(reject_vera_stage(round, requests))
            }
            #[cfg(feature = "vmce")]
            CooperativeRoundEvent::StageAbsent => {
                if round.vera_stage.is_none() {
                    return Ok(CooperativeRoundStep::None);
                }
                Ok(reject_vera_stage(round, requests))
            }
            #[cfg(feature = "vmce")]
            CooperativeRoundEvent::StageRejected => {
                round.outcome = Some(false);
                requests.push(NetworkLoadingRequest::CooperativeLeave);
                Ok(CooperativeRoundStep::Exit(false))
            }
            #[cfg(feature = "vmce")]
            CooperativeRoundEvent::RetryVote { source } => {
                if let Some(row) = slot_of(launch, source) {
                    round.retry_votes[row] = true;
                }
                Ok(CooperativeRoundStep::None)
            }
            CooperativeRoundEvent::PeerReady { source } => {
                if let Some(row) = slot_of(launch, source) {
                    round.ready[row] = true;
                }
                if round.vera {
                    log::debug!(
                        "VMCE round: PeerReady source={source}, all_ready={}, host={host}",
                        round.all_ready()
                    );
                }
                if host && round.all_ready() && round.can_continue && !round.vera {
                    return Ok(CooperativeRoundStep::Assign);
                }
                Ok(CooperativeRoundStep::None)
            }
            CooperativeRoundEvent::PeerWaiting { source } => {
                if let Some(row) = slot_of(launch, source) {
                    round.ready[row] = false;
                }
                Ok(CooperativeRoundStep::None)
            }
            CooperativeRoundEvent::PeerLeft { source } => {
                if let Some(row) = slot_of(launch, source) {
                    round.ready[row] = false;
                    #[cfg(feature = "vmce")]
                    if row < 4 {
                        round.retry_votes[row] = false;
                    }
                }
                connected.retain(|index| *index != source);
                round.quit_enabled = true;
                #[cfg(feature = "vmce")]
                if round.vera {
                    // VMCE's 0x103 page has no Quit path and the next stage
                    // cannot start without every participant. Return both
                    // sides to the lobby instead of stranding the page.
                    if round.outcome.is_none() {
                        round.outcome = Some(false);
                        requests.push(NetworkLoadingRequest::CooperativeLeave);
                    }
                    return Ok(CooperativeRoundStep::Exit(false));
                }
                // The node is deleted (A8DA84--); its slot stays unready,
                // so all-ready can no longer pass. Continue is disabled
                // (by the connected count) and Quit enabled again.
                Ok(CooperativeRoundStep::PeerLeft { source })
            }
            CooperativeRoundEvent::Begin => {
                round.begin = true;
                Ok(tail(requests, host, round))
            }
            CooperativeRoundEvent::Options(options) => {
                let changed = apply_round_options(launch, &options, difficulty)?;
                if changed {
                    round.local_ready = false;
                    clear_human_readiness(launch, round);
                }
                if round.begin {
                    round.quit_enabled = false;
                    requests.push(NetworkLoadingRequest::CooperativeAck);
                }
                Ok(CooperativeRoundStep::OptionsApplied {
                    difficulty: options.difficulty,
                    changed,
                })
            }
            CooperativeRoundEvent::Ack { source } => {
                if let Some(row) = slot_of(launch, source) {
                    round.acked[row] = true;
                }
                // Count distinct acked rows: a duplicated ack never reaches
                // `rows` early and never overshoots it after the real ones.
                if round.acked.iter().filter(|acked| **acked).count() >= round.rows {
                    round.outcome = Some(true);
                    requests.push(NetworkLoadingRequest::CooperativeGo);
                    return Ok(vera_result_step(round));
                }
                Ok(tail(requests, host, round))
            }
            CooperativeRoundEvent::Go => {
                round.outcome = Some(true);
                Ok(vera_result_step(round))
            }
            CooperativeRoundEvent::Chat {
                source,
                text,
                color,
            } => Ok(CooperativeRoundStep::Chat {
                source,
                text,
                color,
            }),
            CooperativeRoundEvent::KeepAlive => Ok(tail(requests, host, round)),
        }
    }

    /// Cooperative `+0xAC` and the next Start_Scenario: a won (or retried)
    /// round freezes its roster as the next immutable start, with the same
    /// agreed seed (`g_RngSeed = mode+0x9C`, `0x005C4890`), the
    /// continuation's MaxAhead and LatencyFudge, no WOL terms
    /// (`0x005C46F5`) and the live membership.
    /// `saved` is a won mission's fresh carry-over record (`0x004C6180`); a
    /// defeat restores the stale one: the last continuation's, or the
    /// cleared record of `0x004C6140`.
    pub(crate) fn freeze_cooperative_round(
        &mut self,
        max_ahead_frames: i32,
        latency_fudge: i32,
        saved: Option<crate::sim::campaign_carry_over::CampaignCarryOver>,
    ) -> anyhow::Result<crate::app::match_bootstrap::LoadingStartup> {
        let host = self.host;
        let network = self
            .network
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
        #[cfg(feature = "vmce")]
        let NetworkPhase::CooperativeRound {
            previous,
            launch,
            round,
            vera_next,
            ..
        } = &network.phase
        else {
            anyhow::bail!("no co-op round to continue");
        };
        #[cfg(not(feature = "vmce"))]
        let NetworkPhase::CooperativeRound {
            previous,
            launch,
            round,
            ..
        } = &network.phase
        else {
            anyhow::bail!("no co-op round to continue");
        };
        anyhow::ensure!(round.outcome == Some(true), "the round did not continue");
        super::admit_network_start_roster(launch)?;
        #[cfg(feature = "vmce")]
        let seed = match vera_next.as_ref() {
            Some(next) => next_stage_seed(previous.seed.value, next),
            None => previous.seed.value,
        };
        #[cfg(not(feature = "vmce"))]
        let seed = previous.seed.value;
        let mode = previous.mode();
        #[cfg(feature = "vmce")]
        let previous_vera_houses = previous.vmce_houses().cloned();
        // A defeat retry reopens the SAME frozen stage. Capture the admitted
        // map identity before any network field is mutated: the rebuilt
        // startup must carry the same config, physical source and effective
        // digest as the previous frozen startup (no next-stage resolution,
        // re-randomisation or campaign.ini re-read).
        #[cfg(feature = "vmce")]
        let retry_map_identity = vera_next.is_none().then(|| {
            (
                previous.vmce_map_config().cloned(),
                previous.local_map_source().cloned(),
                previous.effective_map_digest().map(str::to_owned),
            )
        });
        #[cfg(feature = "vmce")]
        let vera_next = vera_next.clone();
        #[cfg(feature = "vmce")]
        let next_assignments = {
            if let Some(next) = vera_next.as_ref() {
                network.vmce_map_config = Some(next.config.clone());
                network.map_metadata = Some(next.metadata.clone());
                network.local_map_source = Some(next.local_map_source.clone());
                network.local_map_effective_digest = Some(next.effective_digest.clone());
            } else if let Some((config, source, digest)) = retry_map_identity.as_ref() {
                // Retry keeps the current mission: restore the exact admitted
                // map identity the retired startup held. The startup below is
                // rebuilt from this same snapshot.
                if let Some(config) = config {
                    network.vmce_map_config = Some(config.clone());
                }
                network.local_map_source = source.clone();
                network.local_map_effective_digest = digest.clone();
            }
            // The next map owns its own `[Basic] PlayerN`/`[Houses]` binding,
            // so re-resolve each role against it instead of carrying the
            // previous stage's House names. Keep the frozen lobby difficulty
            // row: it is a start input, not a map-derived field.
            match vera_next.as_ref() {
                Some(next) => {
                    let mut houses = crate::app::match_bootstrap::resolve_vmce_houses_with_config(
                        &next.map_ini,
                        &next.map_file,
                        &next.config.source_sha256,
                        &next.config,
                        launch,
                        seed,
                    )?;
                    if let Some(difficulty) = previous_vera_houses
                        .as_ref()
                        .map(|houses| houses.difficulty)
                    {
                        houses.difficulty = difficulty;
                    }
                    Some(houses)
                }
                None => None,
            }
        };
        let carry_over = saved
            .or_else(|| previous.carry_over().copied())
            .unwrap_or(crate::sim::campaign_carry_over::CampaignCarryOver::CLEARED);
        let NetworkPhase::CooperativeRound {
            launch, connected, ..
        } = std::mem::replace(&mut network.phase, NetworkPhase::Lobby)
        else {
            unreachable!()
        };
        let mut prepared = crate::app::match_bootstrap::PreparedNetworkStartup::new(
            launch,
            seed,
            mode,
            host,
            max_ahead_frames,
            super::WolStartTerms::default(),
        )
        .with_carry_over(carry_over);
        #[cfg(feature = "vmce")]
        {
            if let Some(houses) = next_assignments {
                prepared = prepared.with_vmce_houses(houses);
            } else if let Some(houses) = previous_vera_houses {
                prepared = prepared.with_vmce_houses(houses);
            }
            if let Some(next) = vera_next {
                prepared = prepared
                    .with_local_map_source(Some(next.local_map_source), Some(next.effective_digest))
                    .with_vmce_map_config(next.config);
            } else if let Some((config, source, digest)) = retry_map_identity {
                // Retry must not fall back to a map-less startup: inherit the
                // frozen config/source/digest exactly.
                if let Some(config) = config {
                    prepared = prepared.with_vmce_map_config(config);
                }
                prepared = prepared.with_local_map_source(source, digest);
            }
        }
        let startup = Arc::new(prepared);
        network.max_ahead_frames = Some(max_ahead_frames);
        network.wol_start = super::WolStartTerms::default();
        network.phase = NetworkPhase::Frozen {
            startup: Arc::clone(&startup),
            connected,
        };
        self.latency_fudge = latency_fudge;
        Ok(crate::app::match_bootstrap::LoadingStartup::Network(
            startup,
        ))
    }
}

/// The round's fields, split from the visit so a step can queue requests
/// while it edits the round.
struct RoundParts<'a> {
    requests: &'a mut Vec<NetworkLoadingRequest>,
    connected: &'a mut Vec<usize>,
    launch: &'a mut SkirmishLaunchSession,
    round: &'a mut CooperativeRoundState,
}

fn round_parts(session: &mut MatchSession) -> anyhow::Result<RoundParts<'_>> {
    let NetworkSession {
        loading_requests,
        phase,
        ..
    } = session
        .network
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
    let NetworkPhase::CooperativeRound {
        connected,
        launch,
        round,
        ..
    } = phase
    else {
        anyhow::bail!("no co-op result dialog");
    };
    Ok(RoundParts {
        requests: loading_requests,
        connected,
        launch,
        round,
    })
}

/// The loop's tail `0x004702FD`: a master whose every slot is ready leaves
/// on the next packet, sending `0x31`.
fn tail(
    requests: &mut Vec<NetworkLoadingRequest>,
    host: bool,
    round: &mut CooperativeRoundState,
) -> CooperativeRoundStep {
    if host && round.all_ready() && round.can_continue && !round.vera {
        round.outcome = Some(true);
        requests.push(NetworkLoadingRequest::CooperativeGo);
        return CooperativeRoundStep::Exit(true);
    }
    CooperativeRoundStep::None
}

/// A peer's VMCE next stage differs from this peer's: refuse it and leave the
/// round, so both peers return to the lobby instead of loading different
/// missions.
#[cfg(feature = "vmce")]
fn reject_vera_stage(
    round: &mut CooperativeRoundState,
    requests: &mut Vec<NetworkLoadingRequest>,
) -> CooperativeRoundStep {
    log::warn!(
        "VMCE next-stage identity mismatch; returning both peers to the lobby instead of loading"
    );
    round.outcome = Some(false);
    requests.push(NetworkLoadingRequest::CooperativeStageRejected);
    CooperativeRoundStep::Exit(false)
}

/// Every connected, non-observer guest row (not raw slot 0) has voted Retry.
#[cfg(feature = "vmce")]
fn retry_guests_voted(
    launch: &SkirmishLaunchSession,
    connected: &[usize],
    round: &CooperativeRoundState,
) -> bool {
    connected.iter().all(|index| {
        let Some(human) = launch.humans.get(*index) else {
            return true;
        };
        if human.is_observer() {
            return true;
        }
        match human.raw_slot_index {
            Some(0) | None => true,
            Some(row) => round.retry_voted(usize::from(row)),
        }
    })
}

fn clear_human_readiness(launch: &SkirmishLaunchSession, round: &mut CooperativeRoundState) {
    for human in &launch.humans {
        if let Some(row) = human.raw_slot_index {
            round.ready[usize::from(row)] = false;
        }
    }
}

/// `0x0046D4A0`'s payload from the round roster.
fn round_options(launch: &SkirmishLaunchSession, difficulty: i32) -> CooperativeRoundOptions {
    CooperativeRoundOptions {
        difficulty,
        humans: launch
            .humans
            .iter()
            .filter_map(|human| Some((human.raw_slot_index?, human.slot.clone())))
            .collect(),
        ai_player_count: launch.ai_player_count,
        opponents: launch.opponents.clone(),
    }
}

/// `0x0046D650`: the master's values replace every bound slot's start,
/// country and colour (the local slot's too) and the AI rows. True when
/// anything, the difficulty included, changed.
fn apply_round_options(
    launch: &mut SkirmishLaunchSession,
    options: &CooperativeRoundOptions,
    difficulty: i32,
) -> anyhow::Result<bool> {
    let mut candidate = launch.clone();
    let mut changed = options.difficulty != difficulty;
    for (row, choices) in &options.humans {
        let Some(human) = candidate
            .humans
            .iter_mut()
            .find(|human| human.raw_slot_index == Some(*row))
        else {
            continue;
        };
        let slot = &mut human.slot;
        let before = slot.clone();
        slot.start_position = choices.start_position;
        slot.country = choices.country;
        slot.country_random = choices.country_random;
        slot.color_index = choices.color_index;
        slot.color_random = choices.color_random;
        changed |= *slot != before;
    }
    changed |= candidate.opponents != options.opponents
        || candidate.ai_player_count != options.ai_player_count;
    candidate.opponents = options.opponents.clone();
    candidate.ai_player_count = options.ai_player_count;
    super::validate_network_rows(&candidate)?;
    candidate.opponents.sort_by_key(|ai| ai.raw_slot_index);
    candidate.rebuild_house_roster();
    *launch = candidate;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::match_bootstrap::LoadingStartup;
    use crate::app::match_runtime::session::NetworkReturnContext;

    const SEED: u32 = 0x1234_abcd;

    /// A frozen two-player LAN start; `local` is the local source index.
    fn frozen(
        local: usize,
    ) -> (
        MatchSession,
        Arc<crate::app::match_bootstrap::PreparedNetworkStartup>,
    ) {
        frozen_in(NetworkReturnContext::LanLobby, local)
    }

    fn frozen_in(
        context: NetworkReturnContext,
        local: usize,
    ) -> (
        MatchSession,
        Arc<crate::app::match_bootstrap::PreparedNetworkStartup>,
    ) {
        let mut launch = super::super::tests::network_fixture();
        launch.local_player_index = local;
        let mut session = MatchSession::default();
        session.enter_network(context);
        session
            .accept_network_setup(launch, local == 0, Some(7), false)
            .unwrap();
        session.set_network_start_seed(SEED).unwrap();
        session.prepare_network_timing(2, || 0).unwrap();
        let LoadingStartup::Network(startup) = session.freeze_network_start(SEED).unwrap() else {
            panic!("network start");
        };
        (session, startup)
    }

    /// The same frozen two-player LAN start in the authored VMCE mode.
    #[cfg(feature = "vmce")]
    fn frozen_vera(
        local: usize,
    ) -> (
        MatchSession,
        Arc<crate::app::match_bootstrap::PreparedNetworkStartup>,
    ) {
        frozen_vera_rows(local, 2)
    }

    /// A frozen VMCE LAN start with `rows` admitted human rows; `local` is
    /// this process's index, mirroring one peer of a four-player session.
    #[cfg(feature = "vmce")]
    fn frozen_vera_rows(
        local: usize,
        rows: usize,
    ) -> (
        MatchSession,
        Arc<crate::app::match_bootstrap::PreparedNetworkStartup>,
    ) {
        assert!((2..=4).contains(&rows) && local < rows);
        let mut launch = super::super::tests::network_fixture();
        launch.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
        while launch.humans.len() < rows {
            let mut guest = launch.humans[1].clone();
            guest.name = format!("Guest{}", launch.humans.len());
            guest.raw_slot_index = Some(launch.humans.len() as u8);
            guest.slot.color_index = launch.humans.len() as u8;
            launch.humans.push(guest);
        }
        launch.humans.truncate(rows);
        launch.rebuild_house_roster();
        for (index, human) in launch.humans.iter_mut().enumerate() {
            human.role = crate::skirmish_launch::LaunchHumanRole::Player {
                group: index as i32,
            };
        }
        launch.local_player_index = local;
        let mut session = MatchSession::default();
        session.enter_network(NetworkReturnContext::LanLobby);
        session
            .accept_network_setup(launch, local == 0, Some(7), false)
            .unwrap();
        let map_file = session
            .network_setup()
            .unwrap()
            .selected_map_file
            .clone()
            .unwrap();
        let source_sha256 = "a".repeat(64);
        session.network.as_mut().unwrap().map_metadata = Some(super::super::NetworkMapMetadata {
            display_name: map_file.clone(),
            player_capacity: 8,
            official: false,
            byte_length: 100,
            digest: String::new(),
            source_sha256: source_sha256.clone(),
            effective_digest: "b".repeat(64),
        });
        session
            .accept_network_vmce_map_config(Some(
                crate::app::match_bootstrap::NetworkVmceMapConfig::new(
                    map_file,
                    source_sha256,
                    crate::map::vmce_config::VmceMapConfig::default(),
                )
                .unwrap(),
            ))
            .unwrap();
        session.network.as_mut().unwrap().local_map_source =
            Some(crate::map::source::LoadedMapSource::Loose {
                path: std::path::PathBuf::from(
                    session
                        .network_setup()
                        .unwrap()
                        .selected_map_file
                        .clone()
                        .unwrap(),
                ),
                payload_len: 100,
                source_sha256: "a".repeat(64),
                cache_lease: Default::default(),
            });
        session.network.as_mut().unwrap().local_map_effective_digest = Some("b".repeat(64));
        session.set_network_player_ready(0, true).unwrap();
        session.set_network_map_status(0, 2).unwrap();
        session.set_network_start_seed(SEED).unwrap();
        session.prepare_network_timing(2, || 0).unwrap();
        let assignments = crate::skirmish_launch::VmceHouseAssignments {
            participants: session
                .network_setup()
                .unwrap()
                .humans
                .iter()
                .enumerate()
                .map(
                    |(index, human)| crate::skirmish_launch::VmceHouseAssignment {
                        participant_name: human.name.clone(),
                        raw_slot_index: human.raw_slot_index.expect("fixture lobby row"),
                        player_index: index as u8,
                        owner_name: format!("VeraHouse{index}"),
                    },
                )
                .collect(),
            ..Default::default()
        };
        let LoadingStartup::Network(startup) = session
            .freeze_network_start_with_vmce(SEED, Some(assignments))
            .unwrap()
        else {
            panic!("network start");
        };
        (session, startup)
    }

    /// A frozen one-human VMCE LAN start (the host plus its AI seats): no
    /// guest row exists to ack a begin.
    #[cfg(feature = "vmce")]
    fn frozen_vera_solo() -> (
        MatchSession,
        Arc<crate::app::match_bootstrap::PreparedNetworkStartup>,
    ) {
        let mut launch = super::super::tests::network_fixture();
        launch.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
        launch.humans.truncate(1);
        launch.local_player_index = 0;
        launch.humans[0].role = crate::skirmish_launch::LaunchHumanRole::Player { group: 0 };
        launch.rebuild_house_roster();
        let mut session = MatchSession::default();
        session.enter_network(NetworkReturnContext::LanLobby);
        session
            .accept_network_setup(launch, true, Some(7), false)
            .unwrap();
        let map_file = session
            .network_setup()
            .unwrap()
            .selected_map_file
            .clone()
            .unwrap();
        let source_sha256 = "a".repeat(64);
        session.network.as_mut().unwrap().map_metadata = Some(super::super::NetworkMapMetadata {
            display_name: map_file.clone(),
            player_capacity: 8,
            official: false,
            byte_length: 100,
            digest: String::new(),
            source_sha256: source_sha256.clone(),
            effective_digest: "b".repeat(64),
        });
        session
            .accept_network_vmce_map_config(Some(
                crate::app::match_bootstrap::NetworkVmceMapConfig::new(
                    map_file,
                    source_sha256,
                    crate::map::vmce_config::VmceMapConfig::default(),
                )
                .unwrap(),
            ))
            .unwrap();
        let assignments = crate::skirmish_launch::VmceHouseAssignments {
            participants: session
                .network_setup()
                .unwrap()
                .humans
                .iter()
                .enumerate()
                .map(
                    |(index, human)| crate::skirmish_launch::VmceHouseAssignment {
                        participant_name: human.name.clone(),
                        raw_slot_index: human.raw_slot_index.expect("fixture lobby row"),
                        player_index: index as u8,
                        owner_name: format!("VeraHouse{index}"),
                    },
                )
                .collect(),
            ..Default::default()
        };
        session.set_network_player_ready(0, true).unwrap();
        session.set_network_map_status(0, 2).unwrap();
        session.set_network_start_seed(SEED).unwrap();
        session.prepare_network_timing(2, || 0).unwrap();
        let LoadingStartup::Network(startup) = session
            .freeze_network_start_with_vmce(SEED, Some(assignments))
            .unwrap()
        else {
            panic!("network start");
        };
        (session, startup)
    }

    /// A resolved VMCE next stage whose mini map binds Player1=Alpha and
    /// Player2=Beta. The map name is the only identity the tests vary.
    #[cfg(feature = "vmce")]
    fn next_stage(map_file: &str) -> crate::app::match_bootstrap::VmceNextStage {
        crate::app::match_bootstrap::VmceNextStage {
            map_file: map_file.into(),
            config: crate::app::match_bootstrap::NetworkVmceMapConfig::new(
                map_file.into(),
                "a".repeat(64),
                crate::map::vmce_config::VmceMapConfig::default(),
            )
            .unwrap(),
            metadata: super::super::NetworkMapMetadata {
                display_name: map_file.into(),
                player_capacity: 8,
                official: false,
                byte_length: 100,
                digest: String::new(),
                source_sha256: "a".repeat(64),
                effective_digest: "b".repeat(64),
            },
            local_map_source: crate::map::source::LoadedMapSource::Loose {
                path: map_file.into(),
                payload_len: 100,
                source_sha256: "a".repeat(64),
                cache_lease: Default::default(),
            },
            effective_digest: "b".repeat(64),
            map_ini: crate::rules::ini_parser::IniFile::from_str(
                "[Basic]\nPlayer1=Alpha\nPlayer2=Beta\n[Houses]\n0=Alpha\n1=Beta\n",
            ),
        }
    }

    /// The same resolved stage with a map INI that binds four roles, for the
    /// four-player round's role re-resolution.
    #[cfg(feature = "vmce")]
    fn next_stage_players(
        map_file: &str,
        players: usize,
    ) -> crate::app::match_bootstrap::VmceNextStage {
        let mut next = next_stage(map_file);
        let mut ini = String::from("[Basic]\n");
        let mut houses = String::from("[Houses]\n");
        for index in 0..players {
            ini.push_str(&format!("Player{}=House{index}\n", index + 1));
            houses.push_str(&format!("{index}=House{index}\n"));
        }
        ini.push_str(&houses);
        next.map_ini = crate::rules::ini_parser::IniFile::from_str(&ini);
        next
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_retry_vote_waits_for_every_guest_and_starts_once() {
        let (mut host, _) = frozen_vera_rows(0, 4);
        host.enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 4)
            .unwrap();
        host.take_network_loading_requests();
        assert_eq!(
            host.cooperative_round_retry_state(),
            Some((true, false, false))
        );

        let (mut guest1, _) = frozen_vera_rows(1, 4);
        guest1
            .enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 4)
            .unwrap();
        guest1.take_network_loading_requests();
        assert_eq!(
            guest1.cooperative_round_retry_state(),
            Some((false, false, true))
        );
        assert_eq!(
            guest1.cooperative_round_retry_vote().unwrap(),
            CooperativeRoundStep::None
        );
        assert!(
            guest1
                .take_network_loading_requests()
                .contains(&NetworkLoadingRequest::CooperativeRetryVote)
        );
        assert_eq!(
            guest1.cooperative_round_retry_state(),
            Some((false, true, false))
        );
        // A second click is a no-op: the lit button is not clickable again.
        assert_eq!(
            guest1.cooperative_round_retry_vote().unwrap(),
            CooperativeRoundStep::None
        );
        assert!(guest1.take_network_loading_requests().is_empty());

        // One guest is not enough for the host; each vote arrives as 0x39.
        host.cooperative_round_event(CooperativeRoundEvent::RetryVote { source: 1 }, 0)
            .unwrap();
        assert_eq!(
            host.cooperative_round_retry_state(),
            Some((true, false, false))
        );
        host.cooperative_round_event(CooperativeRoundEvent::RetryVote { source: 2 }, 0)
            .unwrap();
        // A duplicate vote for the same slot is idempotent.
        host.cooperative_round_event(CooperativeRoundEvent::RetryVote { source: 2 }, 0)
            .unwrap();
        assert_eq!(
            host.cooperative_round_retry_state(),
            Some((true, false, false))
        );
        host.cooperative_round_event(CooperativeRoundEvent::RetryVote { source: 3 }, 0)
            .unwrap();
        assert_eq!(
            host.cooperative_round_retry_state(),
            Some((true, true, true))
        );

        assert_eq!(
            host.cooperative_round_retry_start(0).unwrap(),
            CooperativeRoundStep::Assign
        );
        // The app's Assign step sends the native begin/options tail.
        host.cooperative_round_assigned(0).unwrap();
        let requests = host.take_network_loading_requests();
        assert!(requests.contains(&NetworkLoadingRequest::CooperativeBegin));
        assert!(
            requests
                .iter()
                .any(|request| matches!(request, NetworkLoadingRequest::CooperativeOptions(_)))
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_retry_freeze_inherits_the_frozen_startup() {
        use crate::app::match_bootstrap::LoadingStartup;
        let (mut session, first) = frozen_vera_rows(0, 2);
        let first_config = first.vmce_map_config().cloned();
        let first_source = first.local_map_source().cloned();
        let first_digest = first.effective_map_digest().map(str::to_owned);
        let first_houses = first.vmce_houses().cloned();
        let first_seed = first.seed;
        let first_launch = first.launch_session().clone();
        assert!(
            first_config.is_some(),
            "fixture startup carries the map config"
        );
        assert!(
            first_source.is_some(),
            "fixture startup carries the map source"
        );
        assert_eq!(first_digest.as_deref().map(str::len), Some(64));

        session
            .enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 2)
            .unwrap();
        session
            .cooperative_round_set_retry_votes([false, true, false, false])
            .unwrap();
        assert!(matches!(
            session.cooperative_round_retry_start(0).unwrap(),
            CooperativeRoundStep::Assign
        ));
        session.cooperative_round_assigned(0).unwrap();
        session
            .cooperative_round_event(CooperativeRoundEvent::Ack { source: 1 }, 0)
            .unwrap();
        // The retry must come from the previous frozen startup, not from
        // whatever the networking fields hold now (a campaign.ini re-read or a
        // cleared config would otherwise produce a map-less startup).
        let network = session.network.as_mut().unwrap();
        network.vmce_map_config = None;
        network.local_map_source = None;
        network.local_map_effective_digest = None;
        network.map_metadata = None;
        let LoadingStartup::Network(next) = session.freeze_cooperative_round(30, 2, None).unwrap()
        else {
            panic!("retry freezes a network startup");
        };
        assert_eq!(next.vmce_map_config().cloned(), first_config);
        assert_eq!(next.local_map_source().cloned(), first_source);
        assert_eq!(next.effective_map_digest().map(str::to_owned), first_digest);
        assert_eq!(next.vmce_houses().cloned(), first_houses);
        assert_eq!(next.seed, first_seed, "retry reuses the frozen seed");
        // Roles, countries and colours are the frozen roster; start positions
        // and derived pre-fill order may be resolved by the finished mission.
        let next_launch = next.launch_session();
        assert_eq!(next_launch.mode, first_launch.mode);
        assert_eq!(
            next_launch.local_player_index,
            first_launch.local_player_index
        );
        assert_eq!(next_launch.humans.len(), first_launch.humans.len());
        for (next_human, first_human) in next_launch.humans.iter().zip(&first_launch.humans) {
            assert_eq!(next_human.role, first_human.role);
            assert_eq!(next_human.raw_slot_index, first_human.raw_slot_index);
            assert_eq!(next_human.slot.country, first_human.slot.country);
            assert_eq!(next_human.slot.color_index, first_human.slot.color_index);
        }
        assert_eq!(next_launch.options, first_launch.options);
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_retry_vote_clears_when_its_guest_disconnects() {
        let (mut host, _) = frozen_vera_rows(0, 2);
        host.enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 2)
            .unwrap();
        host.take_network_loading_requests();
        host.cooperative_round_event(CooperativeRoundEvent::RetryVote { source: 1 }, 0)
            .unwrap();
        assert_eq!(
            host.cooperative_round_retry_state(),
            Some((true, true, true))
        );
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::PeerLeft { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::Exit(false)
        );
        assert!(
            !host.cooperative_round().expect("round").retry_voted(1),
            "a departed guest's vote is cleared"
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn stock_cooperative_round_has_no_retry_vote() {
        let (mut guest, _) = frozen(1);
        guest.enter_cooperative_round(None, true, 2).unwrap();
        assert_eq!(guest.cooperative_round_retry_state(), None);
        assert!(guest.cooperative_round_retry_vote().is_err());
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_retry_page_leave_sends_the_native_left_record() {
        let (mut guest, _) = frozen_vera_rows(1, 2);
        guest
            .enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 2)
            .unwrap();
        guest.take_network_loading_requests();
        guest.quit_cooperative_round().unwrap();
        assert!(
            guest
                .take_network_loading_requests()
                .contains(&NetworkLoadingRequest::CooperativeLeave)
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_round_identities_must_agree_before_the_next_stage_loads() {
        let next = next_stage("mission02.map");
        let identity = crate::app::match_bootstrap::VmceStageIdentity::of(Some(&next)).unwrap();

        // Every peer advertises its resolved stage when the round opens.
        let (mut host, _) = frozen_vera(0);
        host.enter_vmce_round(Some(next.clone()), true, 2).unwrap();
        assert!(host.take_network_loading_requests().iter().any(
            |request| matches!(request, NetworkLoadingRequest::CooperativeStage(stage) if stage == &identity)
        ));

        // A guest that resolved the same stage accepts it.
        let (mut guest, _) = frozen_vera(1);
        guest.enter_vmce_round(Some(next.clone()), true, 2).unwrap();
        guest.take_network_loading_requests();
        assert_eq!(
            guest
                .cooperative_round_event(CooperativeRoundEvent::StageIdentity(identity.clone()), 0)
                .unwrap(),
            CooperativeRoundStep::None
        );

        // A guest that resolved a different mission refuses the host's
        // stage instead of loading a different map.
        let (mut guest, _) = frozen_vera(1);
        guest
            .enter_vmce_round(Some(next_stage("mission03.map")), true, 2)
            .unwrap();
        guest.take_network_loading_requests();
        assert_eq!(
            guest
                .cooperative_round_event(CooperativeRoundEvent::StageIdentity(identity), 0)
                .unwrap(),
            CooperativeRoundStep::Exit(false)
        );
        assert!(
            guest.take_network_loading_requests().iter().any(|request| {
                matches!(request, NetworkLoadingRequest::CooperativeStageRejected)
            })
        );

        // The host treats the refusal as terminal and releases its guest.
        let (mut host, _) = frozen_vera(0);
        host.enter_vmce_round(Some(next), true, 2).unwrap();
        host.take_network_loading_requests();
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::StageRejected, 0)
                .unwrap(),
            CooperativeRoundStep::Exit(false)
        );
        assert!(
            host.take_network_loading_requests()
                .contains(&NetworkLoadingRequest::CooperativeLeave)
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_solo_host_continue_starts_without_waiting_for_a_guest_ack() {
        let (mut session, _) = frozen_vera_solo();
        session
            .enter_vmce_round(Some(next_stage("mission02.map")), true, 1)
            .unwrap();
        session.take_network_loading_requests();
        assert!(session.cooperative_round_continue_enabled());
        assert_eq!(
            session.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Exit(true),
            "a solo host has no guest ack to wait for"
        );
        assert!(
            !session
                .take_network_loading_requests()
                .iter()
                .any(|request| matches!(request, NetworkLoadingRequest::CooperativeBegin))
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_defeat_and_last_mission_continue_leaves_for_the_lobby() {
        let (mut host, _) = frozen_vera(0);
        host.enter_vmce_round(None, false, 2).unwrap();
        host.take_network_loading_requests();
        assert!(
            host.cooperative_round_continue_enabled(),
            "a defeat or last mission acknowledges the page instead of waiting"
        );
        assert_eq!(
            host.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Exit(false)
        );
        assert!(
            host.take_network_loading_requests()
                .contains(&NetworkLoadingRequest::CooperativeLeave)
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_host_continue_is_idempotent_until_the_guests_ack() {
        let (mut host, _) = frozen_vera(0);
        host.enter_vmce_round(Some(next_stage("mission02.map")), true, 2)
            .unwrap();
        host.take_network_loading_requests();
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::PeerReady { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::None
        );
        assert_eq!(
            host.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Assign
        );
        host.cooperative_round_assigned(0).unwrap();
        assert!(matches!(
            host.take_network_loading_requests().as_slice(),
            [
                NetworkLoadingRequest::CooperativeBegin,
                NetworkLoadingRequest::CooperativeOptions(_)
            ]
        ));
        // A repeated Continue/assignment cannot send a second begin or count
        // the host's own ack twice.
        assert!(!host.cooperative_round_continue_enabled());
        assert!(host.continue_cooperative_round(0).is_err());
        host.cooperative_round_assigned(0).unwrap();
        assert!(host.take_network_loading_requests().is_empty());
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::Ack { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::Exit(true),
            "the round still completes on the guest's single ack"
        );
        assert!(
            host.take_network_loading_requests()
                .contains(&NetworkLoadingRequest::CooperativeGo)
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_four_player_round_waits_for_three_guest_acks() {
        let next = next_stage_players("mission02.map", 4);
        let (mut host, _) = frozen_vera_rows(0, 4);
        host.enter_vmce_round(Some(next.clone()), true, 4).unwrap();
        host.take_network_loading_requests();
        for source in 1..=3 {
            assert_eq!(
                host.cooperative_round_event(CooperativeRoundEvent::PeerReady { source }, 0)
                    .unwrap(),
                CooperativeRoundStep::None
            );
        }
        assert_eq!(
            host.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Assign
        );
        host.cooperative_round_assigned(0).unwrap();
        host.take_network_loading_requests();
        for source in 1..=2 {
            assert_eq!(
                host.cooperative_round_event(CooperativeRoundEvent::Ack { source }, 0)
                    .unwrap(),
                CooperativeRoundStep::None,
                "two of three guest acks must not leave"
            );
        }
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::Ack { source: 3 }, 0)
                .unwrap(),
            CooperativeRoundStep::Exit(true)
        );
        let round = host.cooperative_round().unwrap();
        assert_eq!(round.rows, 4);
        assert_eq!(host.network_connected_source_indices(), &[0, 1, 2, 3]);
        let LoadingStartup::Network(startup) = host.freeze_cooperative_round(30, 2, None).unwrap()
        else {
            panic!("network start");
        };
        assert_eq!(startup.seed.value, super::next_stage_seed(SEED, &next));
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_next_stage_freezes_a_new_deterministic_seed() {
        let next = next_stage("mission02.map");
        let seed = super::next_stage_seed(SEED, &next);
        assert_ne!(seed, SEED, "a new mission starts a new random stream");
        assert_eq!(
            seed,
            super::next_stage_seed(SEED, &next.clone()),
            "the same stage identity always derives the same seed"
        );

        let (mut host, first) = frozen_vera(0);
        host.enter_vmce_round(Some(next.clone()), true, 2).unwrap();
        host.take_network_loading_requests();
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::PeerReady { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::None
        );
        assert_eq!(
            host.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Assign
        );
        host.cooperative_round_assigned(0).unwrap();
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::Ack { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::Exit(true)
        );
        let LoadingStartup::Network(startup) = host.freeze_cooperative_round(30, 2, None).unwrap()
        else {
            panic!("network start");
        };
        assert_eq!(startup.seed.value, seed);
        assert_ne!(startup.seed.value, first.seed.value);

        // The map identity is part of the mix: a different next stage does
        // not reuse this mission's stream.
        let other = next_stage("mission03.map");
        assert_ne!(super::next_stage_seed(SEED, &other), seed);
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_round_rejects_a_peer_with_a_different_next_stage_availability() {
        let (mut guest, _) = frozen_vera(1);
        guest
            .enter_vmce_round(Some(next_stage("mission02.map")), true, 2)
            .unwrap();
        guest.take_network_loading_requests();
        assert_eq!(
            guest
                .cooperative_round_event(CooperativeRoundEvent::StageAbsent, 0)
                .unwrap(),
            CooperativeRoundStep::Exit(false),
            "a peer without a next stage must not load this one"
        );
        assert!(
            guest
                .take_network_loading_requests()
                .contains(&NetworkLoadingRequest::CooperativeStageRejected)
        );

        let (mut guest, _) = frozen_vera(1);
        guest.enter_vmce_round(None, false, 2).unwrap();
        guest.take_network_loading_requests();
        let identity =
            crate::app::match_bootstrap::VmceStageIdentity::of(Some(&next_stage("mission02.map")))
                .unwrap();
        assert_eq!(
            guest
                .cooperative_round_event(CooperativeRoundEvent::StageIdentity(identity), 0)
                .unwrap(),
            CooperativeRoundStep::Exit(false)
        );

        // Both peers absent agree and wait for no stage.
        let (mut host, _) = frozen_vera(0);
        host.enter_vmce_round(None, false, 2).unwrap();
        host.take_network_loading_requests();
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::StageAbsent, 0)
                .unwrap(),
            CooperativeRoundStep::None
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_retry_and_campaign_end_pick_their_final_step() {
        use crate::app::match_runtime::session::VmceRoundEnding;

        // Retry: the host skips the ready gate and the guest's ack reaches
        // the same local step.
        let (mut host, _) = frozen_vera(0);
        host.enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 2)
            .unwrap();
        host.take_network_loading_requests();
        assert_eq!(
            host.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Assign
        );
        host.cooperative_round_assigned(0).unwrap();
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::Ack { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::Retry
        );

        // Campaign end: the normal all-ready gate, then the completion step.
        let (mut host, _) = frozen_vera(0);
        host.enter_vmce_round_ending(None, true, VmceRoundEnding::CampaignEnd, 2)
            .unwrap();
        host.take_network_loading_requests();
        host.cooperative_round_event(CooperativeRoundEvent::PeerReady { source: 1 }, 0)
            .unwrap();
        assert!(host.cooperative_round_continue_enabled());
        assert_eq!(
            host.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Assign
        );
        host.cooperative_round_assigned(0).unwrap();
        assert_eq!(
            host.cooperative_round_event(CooperativeRoundEvent::Ack { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::CampaignEnd
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_host_is_auto_ready_and_its_continue_waits_for_guests() {
        let (mut session, _) = frozen_vera(0);
        session
            .enter_vmce_round(Some(next_stage("next.map")), true, 2)
            .unwrap();
        assert!(session.cooperative_round().unwrap().local_ready());
        assert!(session.cooperative_round_slot_ready(0));
        let requests = session.take_network_loading_requests();
        assert_eq!(
            requests[0],
            NetworkLoadingRequest::CooperativeReady,
            "the host announces its auto-ready slot"
        );
        assert!(
            matches!(requests[1], NetworkLoadingRequest::CooperativeStage(_)),
            "the host advertises the next stage it resolved"
        );
        assert!(
            !session.cooperative_round_continue_enabled(),
            "the host's Continue stays disabled while a guest is unready"
        );
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::PeerReady { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::None,
            "the host must press Continue, not leave on the guest's ready"
        );
        assert!(session.cooperative_round_continue_enabled());
        assert_eq!(
            session.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Assign
        );
        assert!(
            session.take_network_loading_requests().is_empty(),
            "the begin/options wait for the assignment"
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_guest_continue_only_readies_its_slot() {
        let (mut session, _) = frozen_vera(1);
        session
            .enter_vmce_round(Some(next_stage("next.map")), true, 2)
            .unwrap();
        session.take_network_loading_requests();
        assert!(!session.cooperative_round().unwrap().local_ready());
        assert!(
            session
                .cooperative_round_event(CooperativeRoundEvent::PeerReady { source: 0 }, 0)
                .unwrap()
                == CooperativeRoundStep::None
        );
        assert!(session.cooperative_round_slot_ready(0));
        assert!(session.cooperative_round_continue_enabled());
        assert_eq!(
            session.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::None
        );
        assert!(session.cooperative_round().unwrap().local_ready());
        assert!(!session.cooperative_round_continue_enabled());
        assert_eq!(
            session.take_network_loading_requests(),
            vec![NetworkLoadingRequest::CooperativeReady]
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_round_freezes_the_resolved_next_stage_into_the_next_start() {
        let (mut session, _) = frozen_vera(0);
        let next = next_stage("mission02.map");
        session
            .enter_vmce_round(Some(next.clone()), true, 2)
            .unwrap();
        session.take_network_loading_requests();
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::PeerReady { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::None
        );
        assert_eq!(
            session.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Assign
        );
        session.cooperative_round_assigned(0).unwrap();
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::Ack { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::Exit(true)
        );
        let LoadingStartup::Network(startup) =
            session.freeze_cooperative_round(30, 2, None).unwrap()
        else {
            panic!("network start");
        };
        assert_eq!(
            startup.launch_session().selected_map_file.as_deref(),
            Some("mission02.map")
        );
        assert_eq!(
            startup
                .vmce_map_config()
                .map(|config| config.map_file.as_str()),
            Some("mission02.map")
        );
        let houses = startup
            .vmce_houses()
            .expect("the next stage carries re-resolved assignments");
        assert_eq!(
            houses
                .participants
                .iter()
                .map(|assignment| (
                    assignment.raw_slot_index,
                    assignment.player_index,
                    assignment.owner_name.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![(0, 0, "Alpha"), (1, 1, "Beta")],
            "the next map's own Basic Player Houses bind each role"
        );
        assert_eq!(
            houses.difficulty, 1,
            "the frozen lobby difficulty row is carried into the next stage"
        );
    }

    #[test]
    fn master_round_begins_on_the_guests_ready_and_leaves_on_its_ack() {
        let (mut session, first) = frozen(0);
        session
            .enter_cooperative_round(Some("C2S02MD.MAP".into()), true, 2)
            .unwrap();
        let launch = session.network_setup().unwrap();
        assert_eq!(launch.selected_map_file.as_deref(), Some("C2S02MD.MAP"));
        assert!(
            launch
                .humans
                .iter()
                .all(|human| human.slot.start_position == LaunchStartPosition::Auto)
        );
        assert!(session.cooperative_round_continue_enabled());

        assert_eq!(
            session.continue_cooperative_round(1).unwrap(),
            CooperativeRoundStep::None
        );
        assert!(!session.cooperative_round_continue_enabled());
        let requests = session.take_network_loading_requests();
        assert!(matches!(
            requests.as_slice(),
            [
                NetworkLoadingRequest::CooperativeOptions(CooperativeRoundOptions {
                    difficulty: 1,
                    ..
                }),
                NetworkLoadingRequest::CooperativeReady,
            ]
        ));

        // A keepalive before all-ready reaches the tail with nothing to do.
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::KeepAlive, 1)
                .unwrap(),
            CooperativeRoundStep::None
        );
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::PeerReady { source: 1 }, 1)
                .unwrap(),
            CooperativeRoundStep::Assign
        );
        assert!(
            session.take_network_loading_requests().is_empty(),
            "the begin waits for the assignment"
        );
        session.cooperative_round_assigned(1).unwrap();
        let requests = session.take_network_loading_requests();
        assert!(matches!(
            requests.as_slice(),
            [
                NetworkLoadingRequest::CooperativeBegin,
                NetworkLoadingRequest::CooperativeOptions(_),
            ]
        ));
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::Ack { source: 1 }, 1)
                .unwrap(),
            CooperativeRoundStep::Exit(true)
        );
        let LoadingStartup::Network(next) = session.freeze_cooperative_round(30, 2, None).unwrap()
        else {
            panic!("network start");
        };
        assert_eq!(
            session.take_network_loading_requests(),
            vec![NetworkLoadingRequest::CooperativeGo],
            "the go outlives the freeze for the transport"
        );
        session.assert_network_start(&next);
        assert_eq!(next.seed.value, first.seed.value);
        assert_eq!(next.max_ahead_frames(), 30);
        assert_eq!(
            next.carry_over(),
            Some(&crate::sim::campaign_carry_over::CampaignCarryOver::CLEARED),
            "a first continuation with no saved record restores the cleared one"
        );
        assert_eq!(session.latency_fudge(), 2);
        assert_eq!(
            next.launch_session().selected_map_file.as_deref(),
            Some("C2S02MD.MAP")
        );
        assert_eq!(session.network_connected_source_indices(), &[0, 1]);
    }

    #[test]
    fn master_leaves_on_any_packet_once_every_slot_is_ready() {
        let (mut session, _) = frozen(0);
        session.enter_cooperative_round(None, true, 3).unwrap();
        session.continue_cooperative_round(0).unwrap();
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::PeerReady { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::Assign
        );
        session.cooperative_round_assigned(0).unwrap();
        // Two acks are not three rows; the tail still leaves.
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::Ack { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::Exit(true)
        );
    }

    #[test]
    fn guest_applies_the_masters_options_and_acks_after_the_begin() {
        let (mut session, _) = frozen(1);
        session.enter_cooperative_round(None, true, 2).unwrap();
        session.continue_cooperative_round(0).unwrap();
        assert_eq!(
            session.take_network_loading_requests(),
            vec![NetworkLoadingRequest::CooperativeReady],
            "only the master sends options"
        );
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::Begin, 0)
                .unwrap(),
            CooperativeRoundStep::None
        );
        let mut options = round_options(session.network_setup().unwrap(), 2);
        options.humans[0].1.color_index = 5;
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::Options(options), 0)
                .unwrap(),
            CooperativeRoundStep::OptionsApplied {
                difficulty: 2,
                changed: true
            }
        );
        let round = session.cooperative_round().unwrap();
        assert!(!round.local_ready(), "a changed 0x32 unreadies everyone");
        assert_eq!(
            session.network_setup().unwrap().humans[0].slot.color_index,
            5
        );
        assert_eq!(
            session.take_network_loading_requests(),
            vec![NetworkLoadingRequest::CooperativeAck]
        );
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::Go, 2)
                .unwrap(),
            CooperativeRoundStep::Exit(true)
        );
        assert_eq!(session.cooperative_round().unwrap().outcome, Some(true));
    }

    #[test]
    fn a_departed_peer_leaves_only_quit() {
        let (mut session, _) = frozen(0);
        session.enter_cooperative_round(None, true, 2).unwrap();
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::PeerLeft { source: 1 }, 0)
                .unwrap(),
            CooperativeRoundStep::PeerLeft { source: 1 }
        );
        assert!(!session.cooperative_round_continue_enabled());
        assert_eq!(session.network_connected_source_indices(), &[0]);
        session.quit_cooperative_round().unwrap();
        assert_eq!(
            session.take_network_loading_requests(),
            vec![NetworkLoadingRequest::CooperativeLeave]
        );
        assert_eq!(session.cooperative_round().unwrap().outcome, Some(false));
        assert!(session.freeze_cooperative_round(30, 1, None).is_err());
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_peer_left_returns_the_remaining_peer_to_the_lobby() {
        let (mut session, _) = frozen_vera(1);
        session.enter_cooperative_round(None, true, 2).unwrap();
        assert!(
            session.cooperative_round().unwrap().vera(),
            "fixture must be the authored VMCE round"
        );
        assert_eq!(
            session
                .cooperative_round_event(CooperativeRoundEvent::PeerLeft { source: 0 }, 0)
                .unwrap(),
            CooperativeRoundStep::Exit(false)
        );
        let requests = session.take_network_loading_requests();
        assert!(
            requests.contains(&NetworkLoadingRequest::CooperativeLeave),
            "the remaining peer must leave for the lobby: {requests:?}"
        );
        assert_eq!(session.cooperative_round().unwrap().outcome, Some(false));
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_defeat_page_keeps_continue_after_a_peer_is_gone() {
        let (mut session, _) = frozen_vera(1);
        session.enter_cooperative_round(None, false, 2).unwrap();
        let Some(NetworkPhase::CooperativeRound { connected, .. }) =
            session.network.as_mut().map(|network| &mut network.phase)
        else {
            panic!("VMCE round");
        };
        connected.retain(|index| *index != 0);
        assert!(
            session.cooperative_round_continue_enabled(),
            "a defeat/last-mission page must still leave with a missing peer"
        );
        assert_eq!(
            session.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Exit(false)
        );
        let requests = session.take_network_loading_requests();
        assert!(
            requests.contains(&NetworkLoadingRequest::CooperativeLeave),
            "the defeat page must leave with a missing peer: {requests:?}"
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vera_round_records_the_page_verdict_for_receipts_and_input() {
        let (mut session, _) = frozen_vera(0);
        session.enter_vmce_round(None, false, 2).unwrap();
        assert!(
            !session.cooperative_round().unwrap().page_victory(),
            "the 0x103 layout is shared, so the verdict starts false"
        );
        session.set_cooperative_round_page_victory(true);
        assert!(session.cooperative_round().unwrap().page_victory());
    }

    #[test]
    fn a_master_leaving_after_the_begin_gives_the_guest_quit_back() {
        let (mut session, _) = frozen(1);
        session.enter_cooperative_round(None, true, 2).unwrap();
        session.continue_cooperative_round(0).unwrap();
        session
            .cooperative_round_event(CooperativeRoundEvent::Begin, 0)
            .unwrap();
        assert!(
            session.cooperative_round().unwrap().quit_enabled(),
            "0x34 alone leaves Quit enabled"
        );
        let options = round_options(session.network_setup().unwrap(), 0);
        session
            .cooperative_round_event(CooperativeRoundEvent::Options(options), 0)
            .unwrap();
        assert!(!session.cooperative_round().unwrap().quit_enabled());
        session
            .cooperative_round_event(CooperativeRoundEvent::PeerLeft { source: 0 }, 0)
            .unwrap();
        assert!(session.cooperative_round().unwrap().quit_enabled());
        assert!(!session.cooperative_round_continue_enabled());
    }

    /// A WOL relay carries no BC6 record: Continue leaves for the room at
    /// once, and the last mission keeps BC6's own result 0.
    #[test]
    fn an_online_round_continues_in_its_room() {
        let (mut session, _) = frozen_in(NetworkReturnContext::Wol, 0);
        session
            .enter_cooperative_round(Some("C2S02MD.MAP".into()), true, 2)
            .unwrap();
        assert_eq!(
            session.continue_cooperative_round(1).unwrap(),
            CooperativeRoundStep::ContinueInRoom
        );
        assert_eq!(session.cooperative_round().unwrap().outcome, Some(false));
        assert!(session.take_network_loading_requests().is_empty());

        let (mut last, _) = frozen_in(NetworkReturnContext::Wol, 0);
        last.enter_cooperative_round(None, false, 2).unwrap();
        assert_eq!(
            last.continue_cooperative_round(1).unwrap(),
            CooperativeRoundStep::Exit(false)
        );
    }

    #[test]
    fn the_last_mission_cannot_continue() {
        let (mut session, _) = frozen(1);
        session.enter_cooperative_round(None, false, 2).unwrap();
        assert_eq!(
            session.continue_cooperative_round(0).unwrap(),
            CooperativeRoundStep::Exit(false)
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_round_facade_delegates_to_the_shared_owner() {
        use super::super::vmce_round;
        let (mut host, _) = frozen_vera_rows(0, 2);
        vmce_round::enter_ending(&mut host, None, true, vmce_round::VmceRoundEnding::Retry, 2)
            .unwrap();
        assert_eq!(
            vmce_round::retry_state(&host),
            host.cooperative_round_retry_state()
        );
        assert_eq!(
            vmce_round::continue_enabled(&host),
            host.cooperative_round_continue_enabled()
        );
        vmce_round::set_retry_votes(&mut host, [false, true, false, false]).unwrap();
        assert_eq!(
            vmce_round::retry_state(&host),
            host.cooperative_round_retry_state()
        );

        let (mut guest, _) = frozen_vera_rows(1, 2);
        vmce_round::enter_ending(
            &mut guest,
            None,
            true,
            vmce_round::VmceRoundEnding::Retry,
            2,
        )
        .unwrap();
        vmce_round::retry_vote(&mut guest).unwrap();
        assert_eq!(
            vmce_round::retry_state(&guest),
            guest.cooperative_round_retry_state()
        );
    }
}
