//! The shared `0x103` campaign score model projection.
//!
//! The offline campaign exit and VERA's VMCE outcome page both draw the native
//! campaign score dialog. This owner builds the dialog's values from the
//! ranking row, the mission's elapsed ticks and the player-controlled Houses'
//! kill total; each caller supplies its own difficulty, side and row selection.
//! It holds no authoritative state: the House totals it reads stay with the
//! simulation, and the caller's result owner decides which Houses are player
//! rows.

use crate::app::AppState;

/// Build the campaign score model from one caller's resolved inputs.
pub(crate) fn build(
    state: &AppState,
    elapsed_score_ticks: i32,
    difficulty: i32,
    ranking: &crate::map::ranking::RankingSection,
    houses: impl IntoIterator<Item = (bool, u32)>,
    side: i32,
) -> crate::ui::campaign_score_shell::CampaignScoreScreenModel {
    let mut model = build_from_sources(
        elapsed_score_ticks,
        difficulty,
        ranking,
        houses,
        side,
        |key| resolve_label(state, key),
    );
    model.theme = state
        .process_assets
        .side_score_theme(
            side,
            crate::rules::ruleset::ScoreTheme::Campaign {
                over_par: model.over_par,
            },
        )
        .map(str::to_owned);
    model
}

fn build_from_sources(
    elapsed_score_ticks: i32,
    difficulty: i32,
    ranking: &crate::map::ranking::RankingSection,
    houses: impl IntoIterator<Item = (bool, u32)>,
    side: i32,
    mut resolve_label: impl FnMut(&str) -> String,
) -> crate::ui::campaign_score_shell::CampaignScoreScreenModel {
    let mut model = crate::ui::campaign_score_shell::CampaignScoreScreenModel {
        side,
        ..Default::default()
    };
    let elapsed_score_seconds = elapsed_score_ticks / 60;
    model.your_time = format_campaign_score_time(elapsed_score_seconds);
    let par_score_ticks = ranking.par_time_for_difficulty(difficulty);
    let par_score_seconds = par_score_ticks / 60;
    model.par_time = format_campaign_score_time(par_score_seconds);
    // Native `0x6C9231..33` divides both Scenario+0x61C and ParTime by 60,
    // then selects the over-par strings only when elapsed is strictly greater.
    model.over_par = elapsed_score_seconds > par_score_seconds;
    let (rank_key, commendation_key) = if elapsed_score_seconds <= par_score_seconds {
        (&ranking.under_par_title, &ranking.under_par_message)
    } else {
        (&ranking.over_par_title, &ranking.over_par_message)
    };
    model.rank = resolve_label(rank_key);
    model.commendation = resolve_label(commendation_key);
    // Native 0x6C92C1 sums all four Killed*Types arrays for every HouseClass
    // with PlayerControl set. MatchStatistics is the existing per-house total
    // owner; `player_control` is the matching +0x1ED bit.
    let enemies_destroyed = houses
        .into_iter()
        .fold(0u32, |sum, (player_control, kills)| {
            if player_control {
                sum.wrapping_add(kills)
            } else {
                sum
            }
        });
    model.enemies_destroyed = enemies_destroyed.to_string();
    model
}

fn resolve_label(state: &AppState, native_key: &str) -> String {
    crate::app::frontend::shell_pass::resolve_csf(state, native_key).into_owned()
}

fn format_campaign_score_time(total_seconds: i32) -> String {
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::build_from_sources;
    use crate::assets::csf_file::test_csf;
    use crate::map::ranking::parse_ranking_section;
    use crate::rules::ini_parser::IniFile;
    use crate::sim::house_state::MatchStatistics;

    #[test]
    fn campaign_score_model_uses_ranking_difficulty_native_buckets_csf_and_player_control_kills() {
        let ranking = parse_ranking_section(&IniFile::from_str(
            "[Ranking]\nParTimeEasy=00:00:01\nParTimeMedium=00:00:02\nParTimeHard=00:00:15\nUnderParTitle=GUI:FastRank\nUnderParMessage=GUI:FastMessage\nOverParTitle=GUI:SlowRank\nOverParMessage=GUI:SlowMessage\n",
        ));
        let csf = test_csf(&[
            ("GUI:FastRank", "Gold"),
            ("GUI:FastMessage", "Under par"),
            ("GUI:SlowRank", "Silver"),
            ("GUI:SlowMessage", "Over par"),
        ]);
        // Production supplies the existing per-house MatchStatistics::kills()
        // projection; these represent two PlayerControl houses plus an AI.
        let player_a = MatchStatistics::from_totals_for_test(3, 2, 0, 0, 0);
        let player_b = MatchStatistics::from_totals_for_test(4, 1, 0, 0, 0);
        let computer = MatchStatistics::from_totals_for_test(90, 10, 0, 0, 0);
        let houses = [
            (true, player_a.kills()),
            (true, player_b.kills()),
            (false, computer.kills()),
        ];
        let resolve = |key: &str| csf.text(key).into_owned();

        // Scenario+0x60C raw 2 selects ParTimeHard. Elapsed=959 and Par=900
        // both divide to the same native 60-tick bucket, so this is UnderPar.
        let equal_bucket = build_from_sources(959, 2, &ranking, houses, 0, resolve);
        assert_eq!(equal_bucket.par_time, "00:00:15");
        assert_eq!(equal_bucket.your_time, "00:00:15");
        assert_eq!(equal_bucket.rank, "Gold");
        assert_eq!(equal_bucket.commendation, "Under par");
        assert_eq!(equal_bucket.enemies_destroyed, "10");

        let over_bucket = build_from_sources(960, 2, &ranking, houses, 0, |key| {
            csf.text(key).into_owned()
        });
        assert_eq!(over_bucket.your_time, "00:00:16");
        assert_eq!(over_bucket.rank, "Silver");
        assert_eq!(over_bucket.commendation, "Over par");
        assert_eq!(over_bucket.enemies_destroyed, "10");
    }
}
