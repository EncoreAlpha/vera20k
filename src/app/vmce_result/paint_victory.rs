//! VMCE's dedicated `0x103` result-page painter.
//!
//! Owns every VMCE-specific draw on the native campaign score dialog: the
//! settlement board (flat fill, list frame, rule, captions, four role rows with
//! flags, verdicts and ready/host glyphs), the full-screen statistics subpage,
//! and the right-panel button captions/lit states. It emits the neutral
//! [`CampaignScoreOverlay`] the shared renderer consumes, so the shared
//! renderer never names a VMCE type or control.
//!
//! The painter projects the open [`VictoryResultPage`] only; the transient
//! controller press is folded in by the shared renderer, which keeps this
//! module's projection pure page state (paint and hit-testing therefore share
//! one button projection).

use crate::app::AppState;
use crate::app::frontend::score_overlay::{CampaignScoreOverlay, OverlayButton};
use crate::app::frontend::shell_pass::resolve_csf;
use crate::app::frontend::skirmish_shell_render::vmce_panel;
use crate::app::vmce_result::types::{
    self, VmceResultBoard, VmceResultButton, VmceStatistics, VmceStatisticsRow,
};
use crate::app::vmce_result::victory::VictoryResultPage;
use crate::render::batch::SpriteInstance;
use crate::render::bit_font::BitFont;
use crate::render::shell_paint::{self, PaintLabel, SHELL_TEXT_RGB_ENABLED};
use crate::render::shell_text::ShellAlign;
use crate::render::skirmish_shell_chrome::SkirmishShellChromeAtlas;
use crate::ui::campaign_score_shell::CampaignScoreLayout;
use crate::ui::shell::geom::ShellScale;

/// The open page's right-panel button projection. Paint and hit-testing both
/// call this, so the drawn and clickable rects cannot drift apart.
pub(crate) fn buttons(
    state: &AppState,
    layout: &CampaignScoreLayout,
) -> Option<Vec<OverlayButton>> {
    let page = victory_page(state)?;
    let continue_id = state
        .frontend
        .campaign_score_page
        .as_ref()
        .map_or(0, |page| page.dialog.continue_button());
    let can_continue = page
        .board
        .as_ref()
        .is_some_and(|board| can_continue(state, board));
    page_buttons(page, layout, continue_id, can_continue)
}

/// The open page's overlay, or `None` for a native page.
pub(crate) fn overlay(
    state: &AppState,
    layout: &CampaignScoreLayout,
) -> Option<CampaignScoreOverlay> {
    let page = victory_page(state)?;
    let continue_id = state
        .frontend
        .campaign_score_page
        .as_ref()
        .map_or(0, |page| page.dialog.continue_button());
    let can_continue = page
        .board
        .as_ref()
        .is_some_and(|board| can_continue(state, board));
    page_overlay(
        page,
        layout,
        continue_id,
        can_continue,
        state.frontend.skirmish_shell_chrome.as_ref(),
        Some(&state.renderer.bit_font),
        |key| resolve_csf(state, key).into_owned(),
    )
}

fn victory_page(state: &AppState) -> Option<&VictoryResultPage> {
    state.frontend.vmce_result_page.as_ref()?.victory()
}

/// The host's all-ready gate: the live round while it exists, else the board's
/// snapshot (captures have no round).
fn can_continue(state: &AppState, board: &VmceResultBoard) -> bool {
    if state.match_state.session.cooperative_round().is_some() {
        state
            .match_state
            .session
            .cooperative_round_continue_enabled()
    } else {
        board.can_continue
    }
}

/// The single button projection of one page. Both paint and hit-testing route
/// through this function (through [`buttons`]).
fn page_buttons(
    page: &VictoryResultPage,
    layout: &CampaignScoreLayout,
    continue_id: u16,
    can_continue: bool,
) -> Option<Vec<OverlayButton>> {
    // Both the board and the statistics subpage are only reachable with a
    // projected board; a board-less VMCE page stays the stock dialog.
    let board = page.board.as_ref()?;
    let rects = types::button_rects(layout.scale);
    let buttons = if board.victory && page.statistics_open() {
        types::statistics_page_buttons(rects)
    } else {
        types::result_buttons(
            board,
            rects,
            layout.continue_button,
            continue_id,
            can_continue,
        )
    };
    Some(buttons.iter().map(overlay_button).collect())
}

fn overlay_button(button: &VmceResultButton) -> OverlayButton {
    OverlayButton {
        id: button.id,
        rect: button.rect,
        pressed: button.pressed,
        enabled: button.enabled,
        caption_key: button.key,
        caption_rect: types::button_caption_rect(button.rect),
        caption_rgb: caption_rgb(button),
    }
}

/// The caption colour the projection asks for; the renderer overrides it while
/// the control is held down (the transient press).
fn caption_rgb(button: &VmceResultButton) -> [f32; 3] {
    if button.pressed {
        types::result_button_text_rgb(true)
    } else if !button.enabled {
        shell_paint::SHELL_TEXT_RGB_DISABLED
    } else {
        types::result_button_text_rgb(false)
    }
}

fn page_overlay(
    page: &VictoryResultPage,
    layout: &CampaignScoreLayout,
    continue_id: u16,
    can_continue: bool,
    chrome: Option<&SkirmishShellChromeAtlas>,
    font: Option<&BitFont>,
    resolve: impl Fn(&str) -> String,
) -> Option<CampaignScoreOverlay> {
    let buttons = page_buttons(page, layout, continue_id, can_continue)?;
    let board = page.board.as_ref()?;
    let mut overlay = CampaignScoreOverlay {
        // The enemies report is hidden only on the board page; a non-victory
        // board (never on the native `0x103`) keeps the stock report.
        hidden_statics: if board.victory {
            hidden_statics()
        } else {
            Vec::new()
        },
        buttons,
        ..Default::default()
    };
    if board.victory && page.statistics_open() {
        // The statistics subpage takes over the body: no native statics, no
        // SCRT/SCRA art, only the bottom Back button.
        let statistics = page.statistics.as_ref()?;
        overlay.suppress_stock_text = true;
        overlay.plain_backdrop = true;
        overlay.labels = statistics_labels(statistics, layout.scale, resolve);
        return Some(overlay);
    }
    // The board's flags/host/ready glyphs come from the skirmish chrome the
    // lobby already loaded. Without it the stock page stays (as before): the
    // board's buttons and hidden statics still apply.
    if let (Some(chrome), Some(font)) = (chrome, font) {
        overlay.labels = board_labels(board, layout.scale, font);
        overlay.board_sprites = board_sprites(chrome, board, layout.scale);
    }
    Some(overlay)
}

/// The native statics VERA's `0x103` pages hide: the top report's
/// destroyed-enemies label and value. Their kind-1 reveals still advance.
fn hidden_statics() -> Vec<u16> {
    [
        crate::ui::campaign_score_shell::ENEMIES_DESTROYED_LABEL_STATIC,
        crate::ui::campaign_score_shell::ENEMIES_DESTROYED_VALUE_STATIC,
    ]
    .into_iter()
    .filter(|id| types::hides_enemy_kills(*id))
    .collect()
}

/// The settlement board's flat fill, native list frame, caption rule and the
/// per-row flag/ready glyphs. Depth order is the old painter's exactly: fill,
/// frame, rule, then each row's flag and ready glyph.
fn board_sprites(
    chrome: &SkirmishShellChromeAtlas,
    board: &VmceResultBoard,
    scale: ShellScale,
) -> Vec<SpriteInstance> {
    let layout = types::result_layout(scale);
    let depth = shell_paint::DIALOG_BACKGROUND_DEPTH;
    let mut sprites = Vec::new();
    vmce_panel::push_fill(&mut sprites, chrome, layout.fill, depth);
    vmce_panel::push_list_frame(&mut sprites, chrome, layout.frame, scale, depth);
    vmce_panel::push_rule(&mut sprites, chrome, layout.header_rule, depth);
    for (row, cells) in board.rows.iter().zip(layout.rows.iter()) {
        if let Some(flag) = row
            .flag
            .as_deref()
            .and_then(|flag| chrome.lan_player_side(flag))
        {
            vmce_panel::push_native_centred(&mut sprites, flag, cells.country_icon, scale);
        }
        let icon = if row.host {
            chrome.lan_player_status(true, false)
        } else if row.ready == Some(true) {
            chrome.lan_player_status(false, true)
        } else {
            None
        };
        if let Some(icon) = icon {
            vmce_panel::push_native_centred(&mut sprites, icon, cells.ready, scale);
        }
    }
    sprites
}

/// The settlement board's captions and row text: five headers, then per row the
/// player, role, country (when it has one) and verdict. Row text uses the
/// authored House colour; captions keep the native yellow.
fn board_labels(
    board: &VmceResultBoard,
    scale: ShellScale,
    font: &BitFont,
) -> Vec<PaintLabel<'static>> {
    let layout = types::result_layout(scale);
    let mut labels = Vec::new();
    for header in layout.headers {
        labels.push(PaintLabel {
            text: crate::i18n::tr(header.key).into(),
            rect: header.rect,
            align: if header.centred {
                ShellAlign::H_CENTER
            } else {
                ShellAlign::NONE
            },
            rgb: SHELL_TEXT_RGB_ENABLED,
            path_a_reveal: None,
        });
    }
    for (row, cells) in board.rows.iter().zip(layout.rows.iter()) {
        let rgb = types::result_text_rgb(row).map(|channel| f32::from(channel) / 255.0);
        labels.push(PaintLabel {
            text: vmce_panel::ellipsize_native(font, &row.player, scale.base_rect(cells.player).w)
                .into(),
            rect: cells.player,
            align: ShellAlign::NONE,
            rgb,
            path_a_reveal: None,
        });
        labels.push(PaintLabel {
            text: vmce_panel::ellipsize_native(font, &row.role, scale.base_rect(cells.role).w)
                .into(),
            rect: cells.role,
            align: ShellAlign::NONE,
            rgb,
            path_a_reveal: None,
        });
        if !row.country.is_empty() {
            labels.push(PaintLabel {
                text: vmce_panel::ellipsize_native(
                    font,
                    &row.country,
                    scale.base_rect(cells.country).w,
                )
                .into(),
                rect: cells.country,
                align: ShellAlign::NONE,
                rgb,
                path_a_reveal: None,
            });
        }
        labels.push(PaintLabel {
            text: crate::i18n::tr(if row.won {
                "VERA:VmceResultWon"
            } else {
                "VERA:VmceResultLost"
            })
            .into(),
            rect: cells.verdict,
            align: ShellAlign::H_CENTER,
            rgb,
            path_a_reveal: None,
        });
    }
    labels
}

/// The full-screen statistics subpage's text: title, your/par time, the six
/// native headings and four role rows in House colours. `resolve` answers the
/// game's CSF keys; `VERA:` keys come from the language catalog. The Back
/// caption belongs to the button projection, not here.
fn statistics_labels(
    statistics: &VmceStatistics,
    scale: ShellScale,
    resolve: impl Fn(&str) -> String,
) -> Vec<PaintLabel<'static>> {
    let layout = types::statistics_layout(scale);
    let mut labels = Vec::new();
    labels.push(PaintLabel {
        text: crate::i18n::tr("VERA:VmceStatistics").into(),
        rect: layout.title,
        align: ShellAlign::H_CENTER,
        rgb: SHELL_TEXT_RGB_ENABLED,
        path_a_reveal: None,
    });
    labels.push(PaintLabel {
        text: format!("{} {}", resolve("GUI:YourTime"), statistics.total_time).into(),
        rect: layout.total_time,
        align: ShellAlign::NONE,
        rgb: SHELL_TEXT_RGB_ENABLED,
        path_a_reveal: None,
    });
    labels.push(PaintLabel {
        text: format!("{} {}", resolve("GUI:ParTime"), statistics.par_time).into(),
        rect: layout.par_time,
        align: ShellAlign::NONE,
        rgb: SHELL_TEXT_RGB_ENABLED,
        path_a_reveal: None,
    });
    for header in layout.headers {
        let text = if header.key.starts_with("VERA:") {
            crate::i18n::tr(header.key)
        } else {
            resolve(header.key)
        };
        labels.push(PaintLabel {
            text: text.into(),
            rect: header.rect,
            align: if header.centred {
                ShellAlign::H_CENTER
            } else {
                ShellAlign::NONE
            },
            rgb: SHELL_TEXT_RGB_ENABLED,
            path_a_reveal: None,
        });
    }
    for (row, cells) in statistics.rows.iter().zip(layout.rows.iter()) {
        let rgb = row_rgb(row);
        let values = [
            row.player.clone(),
            row.kills.to_string(),
            row.losses.to_string(),
            row.built.to_string(),
            row.score.to_string(),
            row.income.to_string(),
        ];
        for (column, value) in values.into_iter().enumerate() {
            labels.push(PaintLabel {
                text: value.into(),
                rect: cells.cells[column],
                align: if column == 0 {
                    ShellAlign::NONE
                } else {
                    ShellAlign::H_CENTER
                },
                rgb,
                path_a_reveal: None,
            });
        }
    }
    labels
}

/// A statistics row's text colour: the authored House colour, else native
/// yellow.
fn row_rgb(row: &VmceStatisticsRow) -> [f32; 3] {
    row.rgb
        .unwrap_or([255, 255, 0])
        .map(|channel| f32::from(channel) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::vmce_result::types::{
        VMCE_LEAVE_CONTROL, VMCE_READY_CONTROL, VMCE_STATS_BACK_CONTROL, VMCE_STATS_CONTROL,
        VmceResultRow,
    };
    use crate::app::vmce_result::victory_button_intent;
    use crate::render::bit_font::tests::make_test_font;
    use crate::ui::shell::geom::RectPx;

    const CONTINUE_ID: u16 = 0x423;

    fn test_font() -> BitFont {
        make_test_font(&[(b'a' as u16, 4)], 4)
    }

    fn row(player: &str, role: &str, rgb: Option<[u8; 3]>, won: bool) -> VmceResultRow {
        VmceResultRow {
            player: player.into(),
            role: role.into(),
            country: "America".into(),
            flag: None,
            won,
            ready: Some(true),
            host: false,
            rgb,
        }
    }

    fn board(host: bool, ready: bool, can_continue: bool) -> VmceResultBoard {
        VmceResultBoard {
            rows: vec![
                row("LiveHost", "US House", Some([32, 160, 64]), true),
                row("LiveGuest", "Soviet House", None, false),
            ],
            victory: true,
            local_host: host,
            local_ready: ready,
            can_continue,
        }
    }

    fn statistics() -> VmceStatistics {
        VmceStatistics {
            rows: vec![
                VmceStatisticsRow {
                    player: "LiveHost".into(),
                    owner: "Americans House".into(),
                    rgb: Some([32, 160, 64]),
                    kills: 12,
                    losses: 3,
                    built: 40,
                    score: 900,
                    income: 5_000,
                },
                VmceStatisticsRow {
                    player: "LiveGuest".into(),
                    owner: "Soviet House".into(),
                    rgb: None,
                    kills: 7,
                    losses: 5,
                    built: 31,
                    score: 640,
                    income: 3_200,
                },
            ],
            total_time: "00:09:47".into(),
            par_time: "00:12:00".into(),
        }
    }

    /// The old `ui::campaign_score_shell` board layout assertions: five columns
    /// inside the authored frame, four rows, right-panel slots reused.
    #[test]
    fn the_board_keeps_the_authored_five_column_layout() {
        let layout = CampaignScoreLayout::compute_layout(800, 600);
        let board_layout = types::result_layout(layout.scale);
        assert_eq!(board_layout.rows.len(), 4);
        assert_eq!(board_layout.headers.len(), 5);
        assert_eq!(board_layout.frame, RectPx::new(124, 432, 396, 103));
        assert_eq!(board_layout.fill, RectPx::new(126, 434, 392, 99));
        assert_eq!(board_layout.header_rule, RectPx::new(130, 452, 384, 1));
        assert_eq!(
            board_layout.headers.map(|header| header.key),
            [
                "VERA:VmcePlayers",
                "VERA:VmceOwner",
                "VERA:VmceCountryColumn",
                "VERA:VmceResultColumn",
                "VERA:VmceReady"
            ]
        );
        for (index, row) in board_layout.rows.iter().enumerate() {
            let y = 455 + index as i32 * 20;
            assert_eq!(row.player, RectPx::new(130, y, 92, 17));
            assert_eq!(row.role, RectPx::new(222, y, 86, 17));
            assert_eq!(row.country_icon, RectPx::new(312, y - 1, 50, 19));
            assert_eq!(row.country, RectPx::new(364, y, 56, 17));
            assert_eq!(row.verdict, RectPx::new(420, y, 52, 17));
            assert_eq!(row.ready, RectPx::new(472, y - 1, 42, 19));
            assert_eq!(row.player.x + row.player.w, row.role.x);
            assert!(row.role.x + row.role.w <= row.country_icon.x);
            assert!(row.country.x + row.country.w <= row.verdict.x);
            assert_eq!(row.verdict.x + row.verdict.w, row.ready.x);
            assert_eq!(
                row.ready.x + row.ready.w,
                board_layout.header_rule.x + board_layout.header_rule.w
            );
        }
        // The board clears the native time panels and the status line, and
        // ends at its final row (no empty fill strip).
        assert!(board_layout.frame.y > 417);
        assert!(board_layout.frame.y + board_layout.frame.h <= layout.status_help.y);
        assert_eq!(
            board_layout.fill.y + board_layout.fill.h,
            board_layout.frame.y + board_layout.frame.h - 2
        );
        // Right-panel slots reuse the lobby's action rows.
        let rects = types::button_rects(layout.scale);
        assert_eq!(rects.ready, RectPx::new(644, 199, 156, 42));
        assert_eq!(rects.statistics, RectPx::new(644, 241, 156, 42));
        assert_eq!(rects.leave, RectPx::new(644, 535, 156, 42));
        // The fitted shell scales the authored rects once.
        let wide = CampaignScoreLayout::compute_responsive_layout(1920, 1080);
        let scaled = types::result_layout(wide.scale);
        assert!(scaled.fill.w > board_layout.fill.w);
        assert!(types::button_rects(wide.scale).ready.w > rects.ready.w);
    }

    /// The migrated six-column statistics table assertions.
    #[test]
    fn the_statistics_table_keeps_the_authored_six_column_layout() {
        let layout = CampaignScoreLayout::compute_layout(800, 600);
        let stats = types::statistics_layout(layout.scale);
        assert_eq!(stats.headers.len(), 6);
        assert_eq!(stats.rows.len(), 4);
        assert_eq!(
            stats.headers.map(|header| header.key),
            [
                "GUI:Player",
                "GUI:Kills",
                "GUI:Losses",
                "GUI:Built",
                "GUI:Score",
                "VERA:VmceIncome"
            ]
        );
        assert_eq!(stats.total_time, RectPx::new(40, 56, 260, 17));
        assert_eq!(stats.par_time, RectPx::new(340, 56, 260, 17));
        assert_eq!(stats.headers[0].rect, RectPx::new(40, 96, 150, 17));
        assert_eq!(stats.headers[5].rect, RectPx::new(490, 96, 90, 17));
        for (index, row) in stats.rows.iter().enumerate() {
            let y = 122 + index as i32 * 34;
            assert_eq!(row.cells[0], RectPx::new(40, y, 150, 24));
            assert_eq!(row.cells[5], RectPx::new(490, y, 90, 24));
            for pair in row.cells.windows(2) {
                assert!(pair[0].x + pair[0].w <= pair[1].x, "columns overlap");
            }
        }
        // The table stays left of the right-panel buttons; the subpage's only
        // button is the bottom Back slot.
        let rects = types::button_rects(layout.scale);
        for row in stats.rows {
            assert!(row.cells[5].x + row.cells[5].w < rects.ready.x);
        }
        let back = types::statistics_page_buttons(rects);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].id, VMCE_STATS_BACK_CONTROL);
        assert_eq!(back[0].rect, rects.leave);
    }

    #[test]
    fn the_board_projection_paints_every_authored_cell() {
        let layout = CampaignScoreLayout::compute_layout(800, 600);
        let settlement = board(true, false, true);
        let labels = board_labels(&settlement, layout.scale, &test_font());
        let board_layout = types::result_layout(layout.scale);
        // Five headers, then per row player/role/country/verdict.
        assert_eq!(labels.len(), 5 + settlement.rows.len() * 4);
        for (label, header) in labels.iter().zip(board_layout.headers.iter()) {
            assert_eq!(label.text, crate::i18n::tr(header.key));
            assert_eq!(label.rect, header.rect);
            assert_eq!(label.rgb, SHELL_TEXT_RGB_ENABLED);
        }
        let first = &labels[5];
        assert_eq!(first.text, "LiveHost");
        assert_eq!(first.rect, board_layout.rows[0].player);
        assert_eq!(first.rgb, [32.0 / 255.0, 160.0 / 255.0, 64.0 / 255.0]);
        assert_eq!(labels[6].text, "US House");
        assert_eq!(labels[7].text, "America");
        assert_eq!(
            labels[8].text,
            crate::i18n::tr("VERA:VmceResultWon"),
            "row verdict"
        );
        // A role without an authored colour keeps the native yellow.
        assert_eq!(labels[12].rgb, [1.0, 1.0, 0.0]);
        assert_eq!(labels[12].text, crate::i18n::tr("VERA:VmceResultLost"));
        // A role with no country drops its country cell.
        let mut no_country = board(true, false, true);
        no_country.rows[1].country.clear();
        assert_eq!(
            board_labels(&no_country, layout.scale, &test_font()).len(),
            5 + settlement.rows.len() * 4 - 1
        );
        // Long names are cut to the authored cell width.
        let mut long = board(true, false, true);
        long.rows[0].player = "a".repeat(64);
        let cut = board_labels(&long, layout.scale, &test_font());
        assert!(cut[5].text.ends_with("..."), "{}", cut[5].text);
        assert!(
            test_font().text_width(&cut[5].text) <= layout.scale.base_rect(cut[5].rect).w as u32
        );
    }

    #[test]
    fn the_statistics_projection_takes_over_the_body() {
        let layout = CampaignScoreLayout::compute_layout(800, 600);
        let statistics = statistics();
        let labels = statistics_labels(&statistics, layout.scale, |key| key.to_owned());
        let stats = types::statistics_layout(layout.scale);
        // Title, your/par time, six headings and two six-cell rows.
        assert_eq!(labels.len(), 1 + 2 + 6 + statistics.rows.len() * 6);
        assert_eq!(labels[0].text, crate::i18n::tr("VERA:VmceStatistics"));
        assert_eq!(labels[0].rect, stats.title);
        assert_eq!(labels[0].align, ShellAlign::H_CENTER);
        assert_eq!(labels[1].text, "GUI:YourTime 00:09:47");
        assert_eq!(labels[1].rect, stats.total_time);
        assert_eq!(labels[2].text, "GUI:ParTime 00:12:00");
        assert_eq!(labels[3].text, "GUI:Player");
        assert_eq!(labels[3].align, ShellAlign::NONE);
        assert_eq!(labels[8].text, crate::i18n::tr("VERA:VmceIncome"));
        assert_eq!(labels[8].align, ShellAlign::H_CENTER);
        // Row cells: authored House colour, or native yellow; first column
        // left-aligned, the rest centred.
        let first_row = &labels[9..15];
        assert_eq!(first_row[0].text, "LiveHost");
        assert_eq!(first_row[0].align, ShellAlign::NONE);
        assert_eq!(
            first_row[0].rgb,
            [32.0 / 255.0, 160.0 / 255.0, 64.0 / 255.0]
        );
        assert_eq!(first_row[1].text, "12");
        assert_eq!(first_row[1].align, ShellAlign::H_CENTER);
        for (column, label) in first_row.iter().enumerate() {
            assert_eq!(label.rect, stats.rows[0].cells[column]);
        }
        assert_eq!(labels[15].rgb, [1.0, 1.0, 0.0], "no authored colour");
    }

    /// Gap (a): a disabled host Continue and an already-voted guest Ready are
    /// painted in their native state but take no press (the input feed keeps
    /// only enabled buttons).
    #[test]
    fn disabled_and_voted_buttons_are_painted_but_not_pressable() {
        let layout = CampaignScoreLayout::compute_layout(800, 600);
        // Host before the all-ready gate: native disabled frame, no press.
        let host = board(true, false, false);
        let host_buttons = page_buttons(
            &VictoryResultPage::new(Some(host.clone()), Some(statistics())),
            &layout,
            CONTINUE_ID,
            false,
        )
        .expect("board projection");
        assert_eq!(host_buttons[0].id, CONTINUE_ID);
        assert!(!host_buttons[0].pressed && !host_buttons[0].enabled);
        assert_eq!(
            host_buttons[0].caption_rgb,
            shell_paint::SHELL_TEXT_RGB_DISABLED
        );
        let host_feed: Vec<u16> = host_buttons
            .iter()
            .filter(|button| button.enabled)
            .map(|button| button.id)
            .collect();
        assert_eq!(host_feed, vec![VMCE_STATS_CONTROL, VMCE_LEAVE_CONTROL]);
        // The gate opens the same row-one button: lit and pressable.
        let open_buttons = page_buttons(
            &VictoryResultPage::new(Some(host), Some(statistics())),
            &layout,
            CONTINUE_ID,
            true,
        )
        .expect("board projection");
        assert!(open_buttons[0].pressed && open_buttons[0].enabled);
        assert_eq!(
            open_buttons[0].caption_rgb,
            types::result_button_text_rgb(true)
        );
        // Guest after Ready: lit frame stays, but the button leaves the feed
        // and the route.
        let voted_page =
            VictoryResultPage::new(Some(board(false, true, false)), Some(statistics()));
        let voted_buttons =
            page_buttons(&voted_page, &layout, CONTINUE_ID, false).expect("board projection");
        assert_eq!(voted_buttons[0].id, VMCE_READY_CONTROL);
        assert!(voted_buttons[0].pressed, "cast vote keeps the lit frame");
        assert!(!voted_buttons[0].enabled, "cast vote takes no press");
        assert_eq!(
            voted_buttons[0].caption_rgb,
            types::result_button_text_rgb(true)
        );
        let voted_feed: Vec<u16> = voted_buttons
            .iter()
            .filter(|button| button.enabled)
            .map(|button| button.id)
            .collect();
        assert!(!voted_feed.contains(&VMCE_READY_CONTROL));
        assert_eq!(
            victory_button_intent(&voted_page, VMCE_READY_CONTROL, CONTINUE_ID),
            None,
            "the released voted Ready is not an intent"
        );
        // An unready guest may press Ready.
        let unready = page_buttons(
            &VictoryResultPage::new(Some(board(false, false, false)), Some(statistics())),
            &layout,
            CONTINUE_ID,
            false,
        )
        .expect("board projection");
        assert!(unready[0].enabled && !unready[0].pressed);
        assert_eq!(unready[0].caption_rgb, types::result_button_text_rgb(false));
    }

    /// Gap (b): opening the statistics subpage and pressing Back returns to the
    /// same board projection.
    #[test]
    fn opening_statistics_and_back_restores_the_board_projection() {
        let layout = CampaignScoreLayout::compute_layout(800, 600);
        let mut page = VictoryResultPage::new(Some(board(true, false, true)), Some(statistics()));
        let board_buttons = page_buttons(&page, &layout, CONTINUE_ID, true).expect("board");
        let board_overlay = page_overlay(&page, &layout, CONTINUE_ID, true, None, None, |key| {
            key.to_owned()
        })
        .expect("board overlay");
        assert!(!board_overlay.suppress_stock_text && !board_overlay.plain_backdrop);
        assert_eq!(
            board_overlay.hidden_statics,
            vec![
                crate::ui::campaign_score_shell::ENEMIES_DESTROYED_LABEL_STATIC,
                crate::ui::campaign_score_shell::ENEMIES_DESTROYED_VALUE_STATIC
            ]
        );
        // The tab opens the subpage: body takeover, only the bottom Back.
        page.toggle_statistics();
        assert!(page.statistics_open());
        let stats_buttons = page_buttons(&page, &layout, CONTINUE_ID, true).expect("stats");
        assert_eq!(stats_buttons.len(), 1);
        assert_eq!(stats_buttons[0].id, VMCE_STATS_BACK_CONTROL);
        assert_eq!(
            stats_buttons[0].rect,
            types::button_rects(layout.scale).leave,
            "the subpage Back takes the bottom native slot"
        );
        let stats_overlay = page_overlay(&page, &layout, CONTINUE_ID, true, None, None, |key| {
            key.to_owned()
        })
        .expect("stats overlay");
        assert!(stats_overlay.suppress_stock_text);
        assert!(stats_overlay.plain_backdrop);
        assert!(stats_overlay.board_sprites.is_empty());
        assert!(!stats_overlay.labels.is_empty(), "statistics labels");
        assert_eq!(
            stats_overlay.labels[0].text,
            crate::i18n::tr("VERA:VmceStatistics")
        );
        assert_eq!(
            victory_button_intent(&page, VMCE_STATS_BACK_CONTROL, CONTINUE_ID),
            Some(crate::app::vmce_result::ButtonIntent::CloseStatistics)
        );
        // Back returns to the board state.
        page.close_statistics();
        let restored = page_buttons(&page, &layout, CONTINUE_ID, true).expect("board");
        assert_eq!(restored, board_buttons);
        let restored_overlay = page_overlay(&page, &layout, CONTINUE_ID, true, None, None, |key| {
            key.to_owned()
        })
        .expect("board overlay");
        assert!(!restored_overlay.suppress_stock_text && !restored_overlay.plain_backdrop);
        assert!(restored_overlay.labels.is_empty());
        assert_eq!(
            restored_overlay.hidden_statics,
            board_overlay.hidden_statics
        );
    }

    /// Gap (c): a reopened page starts clean — no stale subpage state or board.
    #[test]
    fn a_reopened_page_projects_a_clean_board() {
        let layout = CampaignScoreLayout::compute_layout(800, 600);
        // The previous page left the statistics subpage open.
        let mut previous =
            VictoryResultPage::new(Some(board(true, false, true)), Some(statistics()));
        previous.toggle_statistics();
        assert!(previous.statistics_open());
        // The production install creates a fresh page for the new settlement.
        let reopened = VictoryResultPage::new(Some(board(true, false, true)), Some(statistics()));
        assert!(!reopened.statistics_open());
        let overlay = page_overlay(&reopened, &layout, CONTINUE_ID, true, None, None, |key| {
            key.to_owned()
        })
        .expect("reopened overlay");
        assert!(!overlay.suppress_stock_text && !overlay.plain_backdrop);
        assert_eq!(
            overlay.buttons.len(),
            3,
            "board buttons, not the subpage Back"
        );
        assert!(
            overlay
                .buttons
                .iter()
                .all(|button| button.id != VMCE_STATS_BACK_CONTROL)
        );
        assert_eq!(
            overlay.hidden_statics,
            vec![
                crate::ui::campaign_score_shell::ENEMIES_DESTROYED_LABEL_STATIC,
                crate::ui::campaign_score_shell::ENEMIES_DESTROYED_VALUE_STATIC
            ]
        );
        // A page opened with no statistics cannot show the subpage.
        let mut bare = VictoryResultPage::new(Some(board(true, false, true)), None);
        bare.toggle_statistics();
        assert!(!bare.statistics_open());
        let bare_buttons = page_buttons(&bare, &layout, CONTINUE_ID, true).expect("board");
        assert_eq!(bare_buttons.len(), 3);
    }

    /// Caption geometry migrated from the old ui test: every VMCE button's
    /// caption shares the button's horizontal centre with one pixel of top
    /// inset.
    #[test]
    fn button_captions_are_centred_for_lit_and_disabled_states() {
        let layout = CampaignScoreLayout::compute_layout(800, 600);
        let rects = types::button_rects(layout.scale);
        for rect in [rects.ready, rects.statistics, rects.leave] {
            let caption = types::button_caption_rect(rect);
            assert_eq!(caption.x + caption.w / 2, rect.x + rect.w / 2);
            assert!(caption.y > rect.y && caption.y + caption.h <= rect.y + rect.h);
        }
        assert_eq!(
            types::result_button_text_rgb(true),
            [5.0 / 255.0, 12.0 / 255.0, 0.0]
        );
        assert_eq!(types::result_button_text_rgb(false), [1.0, 1.0, 0.0]);
    }
}
