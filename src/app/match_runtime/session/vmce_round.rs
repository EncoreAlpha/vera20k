//! Thin VMCE facade over the shared LAN Cooperative round protocol.
//!
//! The BC6 result dialog, its records, ready/continue and the retry vote are
//! generic LAN Cooperative mechanisms owned by [`super::cooperative_round`].
//! VMCE adds only projections on top (the 0x103 result page, the four-row BC6
//! defeat page and the 0x39 retry-vote record). This module exposes those
//! VMCE call sites without duplicating state or protocol logic: every function
//! delegates to the existing shared owner.
//!
//! A full VMCE-only round state machine is deliberately deferred: the shared
//! round already carries the VMCE flag and ending projection, and splitting it
//! would create two owners for the same wire exchange. Revisit only if VMCE
//! gains a message or transition the generic round cannot express, with a
//! documented threshold and tests.

pub(crate) use super::VmceRoundEnding;
use super::{CooperativeRoundEvent, CooperativeRoundStep, MatchSession};

#[cfg(feature = "vmce")]
pub(crate) fn enter_ending(
    session: &mut MatchSession,
    next: Option<crate::app::match_bootstrap::VmceNextStage>,
    can_continue: bool,
    ending: VmceRoundEnding,
    rows: usize,
) -> anyhow::Result<()> {
    session.enter_vmce_round_ending(next, can_continue, ending, rows)
}

#[cfg(feature = "vmce")]
pub(crate) fn continue_enabled(session: &MatchSession) -> bool {
    session.cooperative_round_continue_enabled()
}

#[cfg(feature = "vmce")]
pub(crate) fn retry_state(session: &MatchSession) -> Option<(bool, bool, bool)> {
    session.cooperative_round_retry_state()
}

#[cfg(feature = "vmce")]
pub(crate) fn retry_vote(session: &mut MatchSession) -> anyhow::Result<CooperativeRoundStep> {
    session.cooperative_round_retry_vote()
}

#[cfg(feature = "vmce")]
pub(crate) fn retry_start(
    session: &mut MatchSession,
    difficulty: i32,
) -> anyhow::Result<CooperativeRoundStep> {
    session.cooperative_round_retry_start(difficulty)
}

#[cfg(feature = "vmce")]
pub(crate) fn set_retry_votes(session: &mut MatchSession, votes: [bool; 4]) -> anyhow::Result<()> {
    session.cooperative_round_set_retry_votes(votes)
}

#[cfg(feature = "vmce")]
pub(crate) fn assigned(session: &mut MatchSession, difficulty: i32) -> anyhow::Result<()> {
    session.cooperative_round_assigned(difficulty)
}

#[cfg(feature = "vmce")]
pub(crate) fn event(
    session: &mut MatchSession,
    event: CooperativeRoundEvent,
    difficulty: i32,
) -> anyhow::Result<CooperativeRoundStep> {
    session.cooperative_round_event(event, difficulty)
}

#[cfg(feature = "vmce")]
pub(crate) fn freeze(
    session: &mut MatchSession,
    max_ahead: i32,
    latency_fudge: i32,
    saved: Option<crate::sim::campaign_carry_over::CampaignCarryOver>,
) -> anyhow::Result<crate::app::match_bootstrap::LoadingStartup> {
    session.freeze_cooperative_round(max_ahead, latency_fudge, saved)
}
