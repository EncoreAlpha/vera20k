//! VMCE's dedicated `0xBC6` defeat-page painter.
//!
//! Owns every VMCE-specific draw on the native BC6 result dialog: the
//! four-row six-column statistics table with its per-row verdict, the hidden
//! native prompts, the Retry/Leave Room captions and their lit/enabled states,
//! and the suppression of chat, player setup, slider and preview. It emits the
//! neutral [`CoopScoreOverlay`] the shared renderer consumes.
//!
//! The native statics are hidden; the statistics table replaces the native
//! score table and every prompt.

use crate::app::AppState;
use crate::app::cooperative_round::CoopScoreView;
use crate::app::frontend::score_overlay::{CoopOverlayButton, CoopOverlayText, CoopScoreOverlay};
use crate::render::shell_text::ShellAlign;
use crate::ui::coop_score_shell::{CONTINUE, COOP_SCORE_CONTROLS, CoopScoreLayout, QUIT};
use crate::ui::shell::geom::{RectPx, ShellScale};

use super::defeat::DefeatResultPage;
use super::types::{self, VmceStatistics, VmceStatisticsLayout};

/// The native shell label colour (`SHELL_LABEL_TEXT_RGB`): VMCE's own page
/// content keeps the same yellow as the shared statics.
const LABEL_RGB: [f32; 3] = [1.0, 1.0, 0.0];

/// The open page's defeat content, if the installed extension is the defeat
/// page.
fn page(state: &AppState) -> Option<&DefeatResultPage> {
    super::defeat::page(state)
}

/// The open page's right-panel button projection.
pub(crate) fn buttons(state: &AppState, view: &CoopScoreView) -> Option<Vec<CoopOverlayButton>> {
    page(state)?;
    Some(retry_buttons(
        state.match_state.session.cooperative_round_retry_state(),
        view.quit_enabled,
    ))
}

/// Retry/Leave Room as the defeat page shows them. `retry` is the session's
/// `(host, lit, enabled)`: a guest that already voted keeps the lit frame 4
/// but takes no press; the host lights Retry once every online guest voted.
fn retry_buttons(retry: Option<(bool, bool, bool)>, quit_enabled: bool) -> Vec<CoopOverlayButton> {
    let (_, lit, enabled) = retry.unwrap_or((false, false, false));
    vec![
        CoopOverlayButton {
            id: CONTINUE,
            label_key: "VERA:VmceRetry",
            lit,
            enabled,
        },
        CoopOverlayButton {
            id: QUIT,
            label_key: "VERA:VmceLeaveRoom",
            lit: false,
            enabled: quit_enabled,
        },
    ]
}

/// The open page's overlay, or `None` for a native page.
pub(crate) fn overlay(
    state: &AppState,
    layout: &CoopScoreLayout,
    view: &CoopScoreView,
) -> Option<CoopScoreOverlay> {
    let defeat = page(state)?;
    let scale = layout.page.scale;
    let table = types::statistics_layout(scale);
    let label = |key: &str, fallback: &str| {
        crate::i18n::label(state.process_assets.csf.as_ref(), key, fallback)
    };
    let ellipsize = |text: &str, max_w: i32| {
        crate::app::frontend::skirmish_shell_render::vmce_panel::ellipsize_native(
            &state.renderer.bit_font,
            text,
            max_w,
        )
    };
    let texts = statistics_texts(&defeat.statistics, &table, &label, &ellipsize, scale);
    Some(CoopScoreOverlay {
        suppress_stock_text: true,
        suppress_body_controls: true,
        hidden_controls: hidden_controls(),
        texts,
        buttons: buttons(state, view).unwrap_or_default(),
    })
}

/// The native BC6 statics VERA's statistics-only page hides: no next mission,
/// description, difficulty prompt or previous-mission/next caption. The
/// statistics table replaces every other native static too.
fn hidden_controls() -> Vec<u16> {
    COOP_SCORE_CONTROLS
        .iter()
        .map(|spec| spec.id)
        .filter(|id| types::defeat_hides_static(*id))
        .collect()
}

/// The statistics table's text lines in the order the shared renderer appends
/// them: the two time headers, the six column headings and one line per role
/// row (player, verdict, kills, losses, built, score, income). Every row keeps
/// its authored House colour.
fn statistics_texts(
    statistics: &VmceStatistics,
    table: &VmceStatisticsLayout,
    label: &dyn Fn(&str, &str) -> String,
    ellipsize: &dyn Fn(&str, i32) -> String,
    scale: ShellScale,
) -> Vec<CoopOverlayText> {
    let mut texts = Vec::new();
    texts.push(CoopOverlayText {
        text: format!(
            "{} {}",
            label("GUI:YourTime", "Your Time"),
            statistics.total_time
        ),
        rect: table.total_time,
        align: ShellAlign::NONE,
        rgb: LABEL_RGB,
    });
    texts.push(CoopOverlayText {
        text: format!(
            "{} {}",
            label("GUI:ParTime", "Par Time"),
            statistics.par_time
        ),
        rect: table.par_time,
        align: ShellAlign::NONE,
        rgb: LABEL_RGB,
    });
    for header in &table.headers {
        texts.push(CoopOverlayText {
            text: label(header.key, header.key),
            rect: header.rect,
            align: if header.centred {
                ShellAlign::H_CENTER
            } else {
                ShellAlign::NONE
            },
            rgb: LABEL_RGB,
        });
    }
    for (row, cells) in statistics.rows.iter().zip(table.rows.iter()) {
        // This page is reached only when every player lost; retain a separate
        // visible verdict for each participant.
        let player_cell = cells.cells[0];
        let name_width = player_cell.w * 2 / 3;
        let name_rect = RectPx::new(player_cell.x, player_cell.y, name_width, player_cell.h);
        let result_rect = RectPx::new(
            player_cell.x + name_width,
            player_cell.y,
            player_cell.w - name_width,
            player_cell.h,
        );
        let rgb = row_rgb(row.rgb);
        texts.push(CoopOverlayText {
            text: ellipsize(&row.player, scale.base_rect(name_rect).w),
            rect: name_rect,
            align: ShellAlign::NONE,
            rgb,
        });
        texts.push(CoopOverlayText {
            text: label("VERA:VmceResultLost", "Lost"),
            rect: result_rect,
            align: ShellAlign::H_CENTER,
            rgb,
        });
        let values = [
            row.kills.to_string(),
            row.losses.to_string(),
            row.built.to_string(),
            row.score.to_string(),
            row.income.to_string(),
        ];
        for (column, value) in values.iter().enumerate() {
            texts.push(CoopOverlayText {
                text: value.clone(),
                rect: cells.cells[column + 1],
                align: ShellAlign::H_CENTER,
                rgb,
            });
        }
    }
    texts
}

/// The row text colour: the row's authored House colour, or the native yellow
/// when the role authored none.
fn row_rgb(rgb: Option<[u8; 3]>) -> [f32; 3] {
    rgb.unwrap_or([255, 255, 0])
        .map(|channel| f32::from(channel) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::vmce_result::types::VmceStatisticsRow;

    fn statistics() -> VmceStatistics {
        let row = |player: &str, kills: u32, rgb: Option<[u8; 3]>| VmceStatisticsRow {
            player: player.to_owned(),
            owner: String::new(),
            rgb,
            kills,
            losses: 3,
            built: 40,
            score: 900,
            income: 5_000,
        };
        VmceStatistics {
            rows: vec![
                row("LiveHost", 12, Some([32, 160, 64])),
                row("LiveGuest1", 7, None),
            ],
            total_time: "00:12:00".into(),
            par_time: "00:15:00".into(),
        }
    }

    fn label(key: &str, _fallback: &str) -> String {
        key.to_owned()
    }

    fn ellipsize(text: &str, _max_w: i32) -> String {
        text.to_owned()
    }

    fn texts() -> Vec<CoopOverlayText> {
        let table = types::statistics_layout(ShellScale::RETAIL);
        statistics_texts(
            &statistics(),
            &table,
            &label,
            &ellipsize,
            ShellScale::RETAIL,
        )
    }

    #[test]
    fn the_statistics_texts_keep_the_authored_order_and_verdicts() {
        let texts = texts();
        // No previous-mission caption: the page starts at the time headers,
        // exactly like the B2 baseline's statistics-only body.
        assert_eq!(texts[0].text, "GUI:YourTime 00:12:00");
        assert_eq!(texts[1].text, "GUI:ParTime 00:15:00");
        let headings = &texts[2..8];
        assert_eq!(
            headings
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            [
                "GUI:Player",
                "GUI:Kills",
                "GUI:Losses",
                "GUI:Built",
                "GUI:Score",
                "VERA:VmceIncome",
            ]
        );
        // First role row: name, verdict, then the five statistic cells.
        let row = &texts[8..15];
        assert_eq!(
            row.iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            [
                "LiveHost",
                "VERA:VmceResultLost",
                "12",
                "3",
                "40",
                "900",
                "5000"
            ]
        );
        // The verdict sits in the remaining third of the Player cell.
        let player_cell = types::statistics_layout(ShellScale::RETAIL).rows[0].cells[0];
        assert_eq!(row[0].rect.w + row[1].rect.w, player_cell.w);
        assert_eq!(row[1].rect.x, player_cell.x + row[0].rect.w);
        assert_eq!(row[1].align, ShellAlign::H_CENTER);
    }

    #[test]
    fn the_role_rows_use_their_authored_house_colour() {
        let texts = texts();
        let authored = [32.0 / 255.0, 160.0 / 255.0, 64.0 / 255.0];
        assert_eq!(texts[8].rgb, authored);
        assert_eq!(texts[9].rgb, authored);
        // A role without an authored colour keeps the native yellow.
        assert_eq!(texts[15].rgb, [1.0, 1.0, 0.0]);
    }

    #[test]
    fn the_retry_gate_lights_but_does_not_arm_a_cast_vote() {
        let host_disabled = retry_buttons(Some((true, false, false)), true);
        assert_eq!(host_disabled[0].id, CONTINUE);
        assert_eq!(host_disabled[0].label_key, "VERA:VmceRetry");
        assert!(!host_disabled[0].lit && !host_disabled[0].enabled);
        let voted = retry_buttons(Some((false, true, false)), true);
        assert!(
            voted[0].lit && !voted[0].enabled,
            "a guest's cast vote keeps the lit frame but takes no press"
        );
        let host_ready = retry_buttons(Some((true, true, true)), true);
        assert!(host_ready[0].lit && host_ready[0].enabled);
        // Quit is Leave Room and follows the round's own quit gate.
        assert_eq!(host_ready[1].id, QUIT);
        assert_eq!(host_ready[1].label_key, "VERA:VmceLeaveRoom");
        assert!(host_ready[1].enabled);
        assert!(!retry_buttons(Some((true, true, true)), false)[1].enabled);
    }

    #[test]
    fn the_defeat_page_hides_its_native_prompts() {
        let hidden = hidden_controls();
        for id in [
            crate::ui::coop_score_shell::NEXT_MISSION,
            crate::ui::coop_score_shell::DESCRIPTION,
            crate::ui::coop_score_shell::DIFFICULTY,
            0x0798,
            0x0799,
            0x079A,
            0x079B,
            crate::ui::coop_score_shell::PREVIOUS_MISSION,
        ] {
            assert!(hidden.contains(&id), "{id:#x} belongs to the stock page");
        }
        for id in [
            crate::ui::coop_score_shell::TIME,
            crate::ui::coop_score_shell::ROW_CELLS[0][0],
            CONTINUE,
            QUIT,
        ] {
            assert!(!hidden.contains(&id), "{id:#x} stays on the defeat page");
        }
    }
}
