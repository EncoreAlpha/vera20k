//! VMCE's settlement statistics owner.
//!
//! VMCE decides its own player rows from the frozen role/House assignment and
//! the map's authored role metadata; the shared
//! [`crate::sim::score::HouseScoreFacts`] projection supplies the per-House
//! counters. The stock Cooperative row selection is not read, and no second
//! statistics state is kept: every page content is projected on demand from the
//! live session, the simulation and the existing `0x103` model.
//!
//! One admitted player gets one row. A House granted to another source in
//! addition to its primary ("shared House") is not a player row, and a House
//! named twice by the same projection is counted once.

use crate::app::AppState;
use crate::app::shell_network_setup::view::VmceResultRoleInfo;
use crate::sim::intern::InternedId;
use crate::sim::score::{HouseScoreFacts, TerminalScoreSnapshot};
use crate::sim::world::Simulation;
use crate::skirmish_launch::VmceHouseAssignments;

use super::types::{VmceStatistics, VmceStatisticsRow};

/// Victory statistics use terminal scores; defeat shows the live, unbonused
/// House values.
#[derive(Clone, Copy)]
pub(crate) enum VmceScoreSource {
    Terminal,
    Live,
}

/// One admitted VMCE seat before the settlement projection.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VmceSettlementSeat<'a> {
    pub name: &'a str,
    /// `LaunchHumanRole::native_value()`: the seat's map role before the frozen
    /// assignment resolves it.
    pub role: i32,
    pub raw_slot_index: Option<u8>,
}

/// One projected player row's identity and its role House's shared facts.
#[derive(Debug, Clone)]
pub(crate) struct VmceRoleFacts {
    pub player: String,
    pub owner: String,
    pub owner_id: Option<InternedId>,
    pub rgb: Option<[u8; 3]>,
    /// `None` when the role's House is absent or `MultiplayPassive`, matching
    /// the Cooperative map's missing row (the page then shows zeros).
    pub facts: Option<HouseScoreFacts>,
}

/// Project VMCE's player rows from its own inputs: admitted seats in seat
/// order, the frozen assignment's primary House per seat, and the authored role
/// metadata. A shared House granted in addition to a primary is not a row.
pub(crate) fn role_rows_from(
    mut seats: Vec<VmceSettlementSeat<'_>>,
    assignments: Option<&VmceHouseAssignments>,
    roles: Option<&[VmceResultRoleInfo; 4]>,
    sim: &Simulation,
) -> Vec<VmceRoleFacts> {
    seats.sort_by_key(|seat| (seat.raw_slot_index.unwrap_or(u8::MAX), seat.role));
    let mut rows = Vec::with_capacity(seats.len());
    for seat in seats {
        let assignment = assignments.and_then(|assignments| {
            assignments.participants.iter().find(|assignment| {
                Some(assignment.raw_slot_index) == seat.raw_slot_index
                    && assignment.participant_name == seat.name
            })
        });
        let role = assignment.map_or(seat.role, |assignment| i32::from(assignment.player_index));
        let info =
            roles.and_then(|roles| usize::try_from(role).ok().and_then(|role| roles.get(role)));
        let owner = assignment.map_or_else(
            || info.map_or("", |info| info.owner.as_str()),
            |assignment| assignment.owner_name.as_str(),
        );
        let owner_id = sim.interner().get(owner);
        let facts = owner_id
            .filter(|owner| {
                sim.houses()
                    .get(owner)
                    .is_some_and(|house| !house.multiplay_passive())
            })
            .and_then(|owner| sim.house_score_facts(owner));
        rows.push(VmceRoleFacts {
            player: seat.name.to_owned(),
            owner: owner.to_owned(),
            owner_id,
            rgb: info.and_then(|info| info.color),
            facts,
        });
    }
    rows
}

/// The live visit's projection: the admitted roster, its frozen assignments and
/// the authored role metadata of the loaded map.
pub(crate) fn role_rows(state: &AppState) -> Option<Vec<VmceRoleFacts>> {
    let seats = {
        let launch = state.match_state.session.network_setup()?;
        admitted_seats(launch.humans.iter().map(|human| {
            (
                human.is_observer(),
                human.name.as_str(),
                human.role.native_value(),
                human.raw_slot_index,
            )
        }))
    };
    let assignments = state.match_state.session.network_vmce_assignments();
    let roles = crate::app::shell_network_setup::view::vmce_result_roles(state);
    let runtime = state.match_state.sim_runtime.runtime()?;
    Some(role_rows_from(
        seats,
        assignments,
        roles.as_ref(),
        &runtime.simulation,
    ))
}

/// The admitted settlement seats: an observer never takes a result row.
fn admitted_seats<'a>(
    humans: impl IntoIterator<Item = (bool, &'a str, i32, Option<u8>)>,
) -> Vec<VmceSettlementSeat<'a>> {
    humans
        .into_iter()
        .filter(|(observer, ..)| !observer)
        .map(|(_, name, role, raw_slot_index)| VmceSettlementSeat {
            name,
            role,
            raw_slot_index,
        })
        .collect()
}

/// The distinct role House facts the settlement projects, in row order.
fn distinct_role_facts(rows: &[VmceRoleFacts]) -> Vec<&HouseScoreFacts> {
    let mut seen = std::collections::BTreeSet::new();
    rows.iter()
        .filter_map(|row| row.facts.as_ref())
        .filter(|facts| seen.insert(facts.owner))
        .collect()
}

/// The score model's kill total: VMCE's player-controlled Houses are its role
/// primaries, so a shared or repeated House contributes once.
pub(crate) fn enemy_kills_from(rows: &[VmceRoleFacts]) -> u32 {
    distinct_role_facts(rows)
        .into_iter()
        .fold(0u32, |sum, facts| sum.wrapping_add(facts.kills))
}

/// The round's player-row count: distinct role primaries the settlement shows.
/// This is the same row set the previous Cooperative map produced for VMCE,
/// now read from VMCE's assignment table instead of a Cooperative projection.
pub(crate) fn role_count_from(rows: &[VmceRoleFacts]) -> usize {
    distinct_role_facts(rows).len()
}

/// [`role_count_from`] for the live visit; diagnostics use this projection.
pub(crate) fn role_count(state: &AppState) -> Option<usize> {
    Some(role_count_from(&role_rows(state)?))
}

/// Project page content from already-gathered rows. Terminal rows win when the
/// ending latched one; otherwise the live facts (or zeros for a missing/passive
/// role House) match the old Cooperative-row fallback.
pub(crate) fn statistics_from(
    rows: &[VmceRoleFacts],
    model: &crate::ui::campaign_score_shell::CampaignScoreScreenModel,
    terminal: Option<&TerminalScoreSnapshot>,
) -> VmceStatistics {
    let mut content = Vec::with_capacity(rows.len());
    for row in rows {
        let values = terminal
            .and_then(|snapshot| {
                let owner_id = row.owner_id?;
                snapshot
                    .rows
                    .iter()
                    .find(|score| score.owner == owner_id)
                    .map(|score| {
                        (
                            score.kills,
                            score.losses,
                            score.built,
                            score.score,
                            score.credits_earned,
                        )
                    })
            })
            .or_else(|| {
                row.facts.map(|facts| {
                    (
                        facts.kills,
                        facts.losses,
                        facts.built,
                        facts.score,
                        facts.credits_earned,
                    )
                })
            })
            .unwrap_or((0, 0, 0, 0, 0));
        content.push(VmceStatisticsRow {
            player: row.player.clone(),
            owner: row.owner.clone(),
            rgb: row.rgb,
            kills: values.0,
            losses: values.1,
            built: values.2,
            score: values.3,
            income: values.4,
        });
    }
    VmceStatistics {
        rows: content,
        total_time: bare_duration(&model.your_time),
        par_time: bare_duration(&model.par_time),
    }
}

/// The page content for one already-projected visit. The rows are computed once
/// per page, so the role metadata and assignment table are not re-read between
/// the score model, the row count and the page values.
pub(crate) fn statistics(
    state: &AppState,
    rows: &[VmceRoleFacts],
    model: &crate::ui::campaign_score_shell::CampaignScoreScreenModel,
    source: VmceScoreSource,
) -> VmceStatistics {
    let terminal = match source {
        VmceScoreSource::Terminal => state
            .match_state
            .sim_runtime
            .runtime()
            .map(|runtime| &runtime.simulation)
            .and_then(|sim| sim.terminal_score_snapshot()),
        VmceScoreSource::Live => None,
    };
    statistics_from(rows, model, terminal)
}

/// The native score model's time fields keep their CSF label (`時間:...`).
/// The shared statistics header draws its own label, so the value is only the
/// trailing `HH:MM:SS`.
fn bare_duration(value: &str) -> String {
    let value = value.trim();
    let candidate = value
        .char_indices()
        .rev()
        .nth(7)
        .map_or(value, |(index, _)| &value[index..]);
    let bytes = candidate.as_bytes();
    if candidate.len() == 8
        && bytes[2] == b':'
        && bytes[5] == b':'
        && bytes[..2]
            .iter()
            .chain(&bytes[3..5])
            .all(u8::is_ascii_digit)
        && bytes[6..].iter().all(u8::is_ascii_digit)
    {
        candidate.to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::house_state::{HouseState, MatchStatistics};
    use crate::ui::campaign_score_shell::CampaignScoreScreenModel;

    fn insert_house(
        sim: &mut Simulation,
        name: &str,
        score: i32,
        kills: u32,
        losses: u32,
    ) -> InternedId {
        let owner = sim.interner_mut_for_test().intern(name);
        let mut house = HouseState::new(owner, 0, None, true, 0, 10);
        house.set_stats_for_test(MatchStatistics::from_totals_for_test(
            kills, 0, losses, 0, 0,
        ));
        house.economy_mut_for_test().set_score_for_test(score);
        sim.register_house(house);
        owner
    }

    fn role_info(owner: &str, color: [u8; 3]) -> VmceResultRoleInfo {
        VmceResultRoleInfo {
            owner: owner.to_owned(),
            country: String::new(),
            flag: None,
            color: Some(color),
        }
    }

    fn assignments(entries: &[(&str, u8, u8, &str)]) -> VmceHouseAssignments {
        VmceHouseAssignments {
            participants: entries
                .iter()
                .map(
                    |(name, slot, role, owner)| crate::skirmish_launch::VmceHouseAssignment {
                        participant_name: (*name).to_owned(),
                        raw_slot_index: *slot,
                        player_index: *role,
                        owner_name: (*owner).to_owned(),
                    },
                )
                .collect(),
            difficulty: 1,
        }
    }

    fn seat<'a>(name: &'a str, role: i32, slot: u8) -> VmceSettlementSeat<'a> {
        VmceSettlementSeat {
            name,
            role,
            raw_slot_index: Some(slot),
        }
    }

    #[test]
    fn role_rows_attribute_each_player_to_its_primary_house() {
        let mut sim = Simulation::with_seed(0x5E77_1E00);
        insert_house(&mut sim, "Host", 10, 2, 1);
        insert_house(&mut sim, "Guest", 20, 4, 0);
        // A third House no assignment names (the shared House a grant adds) is
        // not a player row and its kills are not attributed twice.
        insert_house(&mut sim, "Shared", 30, 7, 2);
        let roles = [
            role_info("Host", [32, 160, 64]),
            role_info("Guest", [200, 40, 40]),
            role_info("", [0, 0, 0]),
            role_info("", [0, 0, 0]),
        ];
        let assignment = assignments(&[("Host", 0, 0, "Host"), ("Guest", 1, 1, "Guest")]);
        // Deliberately out of seat order: the projection sorts by seat.
        let rows = role_rows_from(
            vec![seat("Guest", 1, 1), seat("Host", 0, 0)],
            Some(&assignment),
            Some(&roles),
            &sim,
        );
        assert_eq!(
            rows.iter()
                .map(|row| (row.player.as_str(), row.owner.as_str()))
                .collect::<Vec<_>>(),
            vec![("Host", "Host"), ("Guest", "Guest")]
        );
        assert_eq!(rows[0].rgb, Some([32, 160, 64]));
        assert_eq!(rows[1].rgb, Some([200, 40, 40]));
        let host = rows[0].facts.expect("Host facts");
        assert_eq!((host.kills, host.losses, host.built), (2, 1, 1));
        let guest = rows[1].facts.expect("Guest facts");
        assert_eq!((guest.kills, guest.losses, guest.built), (4, 0, 0));
        let page = statistics_from(&rows, &CampaignScoreScreenModel::default(), None);
        assert_eq!(
            page.rows
                .iter()
                .map(|row| (row.player.as_str(), row.kills, row.losses, row.built))
                .collect::<Vec<_>>(),
            vec![("Host", 2, 1, 1), ("Guest", 4, 0, 0)]
        );
        // Two distinct primaries: the shared House's 7 kills never enter.
        assert_eq!(enemy_kills_from(&rows), 6);
        assert_eq!(role_count_from(&rows), 2);
        assert!(!rows.iter().any(|row| row.owner == "Shared"));
    }

    #[test]
    fn a_repeated_house_is_counted_once_but_each_player_keeps_a_row() {
        let mut sim = Simulation::with_seed(0x5E77_1E01);
        insert_house(&mut sim, "Host", 10, 5, 1);
        insert_house(&mut sim, "Guest", 20, 3, 0);
        let roles = [
            role_info("Host", [32, 160, 64]),
            role_info("Guest", [200, 40, 40]),
            role_info("", [0, 0, 0]),
            role_info("", [0, 0, 0]),
        ];
        // A malformed/duplicated assignment puts both seats on the same
        // primary; the player rows stay two, the House total is counted once.
        let assignment = assignments(&[("Host", 0, 0, "Host"), ("Guest", 1, 0, "Host")]);
        let rows = role_rows_from(
            vec![seat("Host", 0, 0), seat("Guest", 1, 1)],
            Some(&assignment),
            Some(&roles),
            &sim,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].owner, "Host");
        let page = statistics_from(&rows, &CampaignScoreScreenModel::default(), None);
        assert_eq!(page.rows.len(), 2, "each admitted player keeps a row");
        assert_eq!(
            (page.rows[0].kills, page.rows[1].kills),
            (5, 5),
            "both players read the same House's facts"
        );
        assert_eq!(enemy_kills_from(&rows), 5, "the House is counted once");
        assert_eq!(role_count_from(&rows), 1);
    }

    #[test]
    fn observers_never_take_a_settlement_row() {
        let seats = admitted_seats(vec![
            (true, "Watcher", -1, Some(2)),
            (false, "Host", 0, Some(0)),
        ]);
        assert_eq!(seats.len(), 1, "an observer is not a player row");
        assert_eq!(
            (seats[0].name, seats[0].role, seats[0].raw_slot_index),
            ("Host", 0, Some(0))
        );
    }

    #[test]
    fn four_role_rows_keep_seat_order_and_their_own_house_values() {
        let mut sim = Simulation::with_seed(0x5E77_1E02);
        for (name, score, kills) in [("P0", 1, 1), ("P1", 2, 2), ("P2", 3, 3), ("P3", 4, 4)] {
            insert_house(&mut sim, name, score, kills, 0);
        }
        let roles = std::array::from_fn(|index| {
            let name = ["P0", "P1", "P2", "P3"][index];
            role_info(name, [index as u8, 0, 0])
        });
        let assignment = assignments(&[
            ("P0", 0, 0, "P0"),
            ("P1", 1, 1, "P1"),
            ("P2", 2, 2, "P2"),
            ("P3", 3, 3, "P3"),
        ]);
        let rows = role_rows_from(
            vec![
                seat("P3", 3, 3),
                seat("P1", 1, 1),
                seat("P0", 0, 0),
                seat("P2", 2, 2),
            ],
            Some(&assignment),
            Some(&roles),
            &sim,
        );
        let page = statistics_from(&rows, &CampaignScoreScreenModel::default(), None);
        assert_eq!(
            page.rows
                .iter()
                .map(|row| (row.player.as_str(), row.kills, row.score))
                .collect::<Vec<_>>(),
            vec![("P0", 1, 1), ("P1", 2, 2), ("P2", 3, 3), ("P3", 4, 4),]
        );
    }

    #[test]
    fn a_passive_role_house_shows_its_player_row_with_zeros() {
        let mut sim = Simulation::with_seed(0x5E77_1E03);
        let owner = insert_house(&mut sim, "Passive", 50, 9, 2);
        sim.make_house_multiplay_passive(owner);
        let roles = [
            role_info("Passive", [1, 2, 3]),
            role_info("", [0, 0, 0]),
            role_info("", [0, 0, 0]),
            role_info("", [0, 0, 0]),
        ];
        let assignment = assignments(&[("Host", 0, 0, "Passive")]);
        let rows = role_rows_from(
            vec![seat("Host", 0, 0)],
            Some(&assignment),
            Some(&roles),
            &sim,
        );
        assert_eq!(rows.len(), 1);
        assert!(rows[0].facts.is_none(), "a passive House is no fact row");
        let page = statistics_from(&rows, &CampaignScoreScreenModel::default(), None);
        assert_eq!(
            (page.rows[0].kills, page.rows[0].losses, page.rows[0].built),
            (0, 0, 0),
            "the player row stays with zeros, as the old Cooperative map did"
        );
        assert_eq!(enemy_kills_from(&rows), 0);
    }

    #[test]
    fn terminal_rows_win_over_live_facts() {
        let mut sim = Simulation::with_seed(0x5E77_1E04);
        let host = insert_house(&mut sim, "Host", 10, 2, 1);
        insert_house(&mut sim, "Guest", 20, 4, 0);
        let roles = [
            role_info("Host", [1, 1, 1]),
            role_info("Guest", [2, 2, 2]),
            role_info("", [0, 0, 0]),
            role_info("", [0, 0, 0]),
        ];
        let assignment = assignments(&[("Host", 0, 0, "Host"), ("Guest", 1, 1, "Guest")]);
        let rows = role_rows_from(
            vec![seat("Host", 0, 0), seat("Guest", 1, 1)],
            Some(&assignment),
            Some(&roles),
            &sim,
        );
        let terminal = TerminalScoreSnapshot {
            rows: vec![crate::sim::score::TerminalScoreRowSnapshot {
                owner: host,
                country: None,
                survived: true,
                kills: 11,
                losses: 12,
                built: 13,
                raw_score: 14,
                score: 15,
                credits_earned: 16,
            }],
        };
        let page = statistics_from(
            &rows,
            &CampaignScoreScreenModel {
                your_time: String::new(),
                par_time: String::new(),
                ..Default::default()
            },
            Some(&terminal),
        );
        assert_eq!(
            (page.rows[0].kills, page.rows[0].losses, page.rows[0].built),
            (11, 12, 13)
        );
        assert_eq!((page.rows[0].score, page.rows[0].income), (15, 16));
        // Guest has no terminal row and falls back to its live facts.
        assert_eq!((page.rows[1].kills, page.rows[1].score), (4, 20));
    }

    #[test]
    fn the_time_header_drops_the_native_label() {
        assert_eq!(bare_duration("時間:00:00:13"), "00:00:13");
        assert_eq!(bare_duration("00:00:13"), "00:00:13");
        assert_eq!(bare_duration(""), "");
    }
}
