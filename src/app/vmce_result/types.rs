//! VMCE result-page content types: the settlement board, the statistics
//! projection and the right-panel button geometry. This module owns the
//! VERA-only layout inside the stock `0x103` / `0xBC6` dialogs; the shared
//! `ui` models carry none of it.
//!
//! Native anchors: the board sits in `0x103`'s empty lower band, the right
//! panel reuses the lobby action slots, and the statistics page overlays the
//! reused `SCRBK` backdrop. Every rect is authored at 800x600 and mapped by the
//! open page's [`ShellScale`].

use crate::ui::shell::geom::{RectPx, ShellScale};

/// One VMCE settlement participant row.
#[derive(Debug, Clone, Default)]
pub(crate) struct VmceResultRow {
    /// Lobby player name for the separate Player column (LiveHost, ...).
    pub player: String,
    /// Owner cell: the authored role/House name only, no "P<n>" prefix.
    pub role: String,
    /// Localized country name, if the role has one.
    pub country: String,
    /// Native country emblem PCX.
    pub flag: Option<String>,
    /// This row's mission verdict (won when every viewpoint survived).
    pub won: bool,
    /// Continue-readiness; `None` on a defeat page, which has no ready gate.
    pub ready: Option<bool>,
    /// This row is the LAN host (raw slot 0).
    pub host: bool,
    /// The authored House colour the row's text uses, if the role has one.
    pub rgb: Option<[u8; 3]>,
}

/// The VMCE settlement board shown over the `0x103` layout: rows plus the
/// truthful continuation state for the current ending.
#[derive(Debug, Clone, Default)]
pub(crate) struct VmceResultBoard {
    pub rows: Vec<VmceResultRow>,
    pub victory: bool,
    /// This peer is the LAN host: no guest Ready button, Continue + Leave.
    pub local_host: bool,
    /// This guest already pressed Ready: the button keeps its lit/pressed
    /// state and is not clickable again.
    pub local_ready: bool,
    /// The host's all-ready gate at projection time. The live page reads the
    /// round again every frame; captures have no round and use this snapshot.
    pub can_continue: bool,
}

/// One VMCE settlement board caption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmceResultHeader {
    pub key: &'static str,
    pub rect: RectPx,
    pub centred: bool,
}

/// One row's cells. Text is inset four pixels inside the flat panel fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmceResultRowCells {
    pub player: RectPx,
    pub role: RectPx,
    pub country_icon: RectPx,
    pub country: RectPx,
    pub verdict: RectPx,
    /// One glyph cell: host H-in-star, ready guest star, or blank.
    pub ready: RectPx,
}

/// The VMCE settlement page's flat board and status strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmceResultLayout {
    pub frame: RectPx,
    pub fill: RectPx,
    pub header_rule: RectPx,
    pub headers: [VmceResultHeader; 5],
    pub rows: [VmceResultRowCells; 4],
}

/// The settlement board sits in the `0x103` dialog's empty lower band, inside
/// the native border: one flat fill, a caption rule and four rows. The page's
/// continuation caption is gone, so the board ends right below the last row and
/// clears the native stats, eagle, time panels and the right-panel buttons.
fn authored_result_layout() -> VmceResultLayout {
    // The board ends at the final row (including the 19 px Ready cell): no
    // empty fill strip below it.
    let frame = RectPx::new(124, 432, 396, 103);
    let fill = RectPx::new(126, 434, 392, 99);
    let x = 130;
    let text_w = 384;
    let header_y = 436;
    // Player / Owner / Country / Result / Ready, rebalanced inside the same
    // 384 px content width so player and House names keep whole at 800.
    let headers = [
        VmceResultHeader {
            key: "VERA:VmcePlayers",
            rect: RectPx::new(x, header_y, 92, 15),
            centred: false,
        },
        VmceResultHeader {
            key: "VERA:VmceOwner",
            rect: RectPx::new(x + 92, header_y, 86, 15),
            centred: false,
        },
        VmceResultHeader {
            key: "VERA:VmceCountryColumn",
            rect: RectPx::new(x + 182, header_y, 112, 15),
            centred: false,
        },
        VmceResultHeader {
            key: "VERA:VmceResultColumn",
            rect: RectPx::new(x + 290, header_y, 52, 15),
            centred: true,
        },
        VmceResultHeader {
            key: "VERA:VmceReady",
            rect: RectPx::new(x + 342, header_y, 42, 15),
            centred: true,
        },
    ];
    let rows = std::array::from_fn(|row| {
        let y = 455 + row as i32 * 20;
        VmceResultRowCells {
            player: RectPx::new(x, y, 92, 17),
            role: RectPx::new(x + 92, y, 86, 17),
            country_icon: RectPx::new(x + 182, y - 1, 50, 19),
            country: RectPx::new(x + 234, y, 56, 17),
            verdict: RectPx::new(x + 290, y, 52, 17),
            ready: RectPx::new(x + 342, y - 1, 42, 19),
        }
    });
    VmceResultLayout {
        frame,
        fill,
        header_rule: RectPx::new(x, 452, text_w, 1),
        headers,
        rows,
    }
}

fn scale_layout(scale: ShellScale, layout: &mut VmceResultLayout) {
    layout.frame = scale.scale_rect(layout.frame);
    layout.fill = scale.scale_rect(layout.fill);
    layout.header_rule = scale.scale_rect(layout.header_rule);
    for header in &mut layout.headers {
        header.rect = scale.scale_rect(header.rect);
    }
    for row in &mut layout.rows {
        row.player = scale.scale_rect(row.player);
        row.role = scale.scale_rect(row.role);
        row.country_icon = scale.scale_rect(row.country_icon);
        row.country = scale.scale_rect(row.country);
        row.verdict = scale.scale_rect(row.verdict);
        row.ready = scale.scale_rect(row.ready);
    }
}

/// The settlement board at the open page's scale. The retail mapping is the
/// identity; only the fitted shell scales the authored 800x600 rects.
pub(crate) fn result_layout(scale: ShellScale) -> VmceResultLayout {
    let mut layout = authored_result_layout();
    scale_layout(scale, &mut layout);
    layout
}

/// The `0x103` statics VERA's victory/campaign-end pages hide: the top
/// report's destroyed-enemies label and value; the native panel keeps its
/// height.
pub(crate) fn hides_enemy_kills(id: u16) -> bool {
    matches!(
        id,
        crate::ui::campaign_score_shell::ENEMIES_DESTROYED_LABEL_STATIC
            | crate::ui::campaign_score_shell::ENEMIES_DESTROYED_VALUE_STATIC
    )
}

/// VERA-only control ids outside the native `0x103`/`0xBC7` id ranges.
pub(crate) const VMCE_READY_CONTROL: u16 = 0x7001;
pub(crate) const VMCE_LEAVE_CONTROL: u16 = 0x7002;
pub(crate) const VMCE_STATS_CONTROL: u16 = 0x7003;
pub(crate) const VMCE_STATS_BACK_CONTROL: u16 = 0x7004;
/// The two additional BC6 score rows' cell ids, outside the native range.
pub(crate) const VMCE_ROW_CELLS: [[u16; 5]; 2] = [
    [0x7B81, 0x7B83, 0x7B85, 0x7B87, 0x7B89],
    [0x7B82, 0x7B84, 0x7B86, 0x7B88, 0x7B8A],
];
/// BC6's row count on VERA's defeat page: the stock page keeps two.
pub(crate) const DEFEAT_SCORE_ROWS: usize = 4;

/// Authored 800x600 right-panel slots the VMCE result buttons reuse: the
/// lobby's topmost action slot (Continue/Ready), the slot below it
/// (`统计数据`) and the bottom native slot (Leave Room / stats Back).
const READY_RECT: RectPx = RectPx::new(644, 199, 156, 42);
const STATS_RECT: RectPx = RectPx::new(644, 241, 156, 42);
const LEAVE_RECT: RectPx = RectPx::new(644, 535, 156, 42);

/// The three right-panel slots the VMCE pages use at one scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmceButtonRects {
    pub ready: RectPx,
    pub statistics: RectPx,
    pub leave: RectPx,
}

pub(crate) fn button_rects(scale: ShellScale) -> VmceButtonRects {
    VmceButtonRects {
        ready: scale.scale_rect(READY_RECT),
        statistics: scale.scale_rect(STATS_RECT),
        leave: scale.scale_rect(LEAVE_RECT),
    }
}

/// One right-panel button on a VMCE victory/campaign-end page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VmceResultButton {
    pub kind: VmceResultButtonKind,
    /// The controller id the input feed uses: a native Continue id or one of
    /// the VERA-only ids.
    pub id: u16,
    pub rect: RectPx,
    /// Native SDBTNANM frame 4: the guest's Ready keeps this after the click.
    pub pressed: bool,
    pub enabled: bool,
    pub key: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmceResultButtonKind {
    Ready,
    Continue,
    Statistics,
    Leave,
    /// The statistics page's only right-panel control (row one, normal state).
    Back,
}

/// The VMCE page's right-panel buttons. The first slot is Continue for both
/// roles, the second is `统计数据`, and the bottom slot is Leave Room.
/// The guest's Continue records readiness. `continue_id` is the dialog's native
/// result control, so the host's Continue keeps the native input action. The
/// host's Continue uses frame 4 only while the all-ready gate is open; before
/// that it is the native disabled frame 2.
pub(crate) fn result_buttons(
    board: &VmceResultBoard,
    rects: VmceButtonRects,
    native_continue: RectPx,
    continue_id: u16,
    can_continue: bool,
) -> Vec<VmceResultButton> {
    if !board.victory {
        return vec![VmceResultButton {
            kind: VmceResultButtonKind::Continue,
            id: continue_id,
            rect: native_continue,
            pressed: false,
            enabled: true,
            key: "GUI:Continue",
        }];
    }
    let leave = VmceResultButton {
        kind: VmceResultButtonKind::Leave,
        id: VMCE_LEAVE_CONTROL,
        rect: rects.leave,
        pressed: false,
        enabled: true,
        key: "VERA:VmceLeaveRoom",
    };
    let statistics = statistics_button(rects);
    if board.local_host {
        return vec![
            VmceResultButton {
                kind: VmceResultButtonKind::Continue,
                id: continue_id,
                rect: rects.ready,
                pressed: can_continue,
                enabled: can_continue,
                key: "GUI:Continue",
            },
            statistics,
            leave,
        ];
    }
    vec![
        VmceResultButton {
            kind: VmceResultButtonKind::Ready,
            id: VMCE_READY_CONTROL,
            rect: rects.ready,
            pressed: board.local_ready,
            enabled: !board.local_ready,
            key: "GUI:Continue",
        },
        statistics,
        leave,
    ]
}

/// The victory page's `统计数据` row-two button.
pub(crate) fn statistics_button(rects: VmceButtonRects) -> VmceResultButton {
    VmceResultButton {
        kind: VmceResultButtonKind::Statistics,
        id: VMCE_STATS_CONTROL,
        rect: rects.statistics,
        pressed: false,
        enabled: true,
        key: "VERA:VmceStatistics",
    }
}

/// The statistics page's only right-panel button: a normal (never lit) Back
/// in the bottom native slot, where Leave Room usually sits.
pub(crate) fn statistics_back_button(rects: VmceButtonRects) -> VmceResultButton {
    VmceResultButton {
        kind: VmceResultButtonKind::Back,
        id: VMCE_STATS_BACK_CONTROL,
        rect: rects.leave,
        pressed: false,
        enabled: true,
        key: "VERA:VmceBack",
    }
}

/// The statistics page's complete right-panel set: only the bottom-slot Back.
pub(crate) fn statistics_page_buttons(rects: VmceButtonRects) -> Vec<VmceResultButton> {
    vec![statistics_back_button(rects)]
}

/// The centred caption rectangle of a VMCE right-panel button. The native
/// pressed owner-draw offset moved the lit caption off-centre, so every VMCE
/// state uses this horizontal frame with one pixel of top inset.
pub(crate) const fn button_caption_rect(rect: RectPx) -> RectPx {
    RectPx::new(rect.x, rect.y + 1, rect.w, rect.h - 1)
}

/// The caption colour of a VMCE result-page button. The native SDBTNANM
/// frame 4 is the lit/bright yellow state (`0x006192FC`'s pressed frame), so
/// its caption switches to the packed owner-draw button text colour
/// `0x00000C05`; releasing (or an ordinary enabled button) keeps the native
/// yellow.
pub(crate) fn result_button_text_rgb(pressed: bool) -> [f32; 3] {
    if pressed {
        [5.0 / 255.0, 12.0 / 255.0, 0.0]
    } else {
        [1.0, 1.0, 0.0]
    }
}

/// The row text colour: the row's authored House colour, or the native yellow
/// when the role authored none. Headers keep the native yellow separately.
pub(crate) fn result_text_rgb(row: &VmceResultRow) -> [u8; 3] {
    row.rgb.unwrap_or([255, 255, 0])
}

/// One role row of VERA's full-screen statistics page: the player/House
/// projection plus the terminal score cells and the income statistic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VmceStatisticsRow {
    pub player: String,
    pub owner: String,
    pub rgb: Option<[u8; 3]>,
    pub kills: u32,
    pub losses: u32,
    pub built: u32,
    pub score: i32,
    pub income: i64,
}

/// VERA's victory/campaign-end statistics page content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VmceStatistics {
    pub rows: Vec<VmceStatisticsRow>,
    pub total_time: String,
    pub par_time: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmceStatisticsRowCells {
    pub cells: [RectPx; 6],
}

/// VERA's full-screen statistics layout inside the reused `0x103` background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmceStatisticsLayout {
    pub title: RectPx,
    pub total_time: RectPx,
    pub par_time: RectPx,
    pub headers: [VmceResultHeader; 6],
    pub rows: [VmceStatisticsRowCells; 4],
}

/// Authored 800x600 statistics table shared by the victory statistics page,
/// the victory page's right-panel toggle and the BC6 defeat page: time header,
/// six native headings and four role rows.
fn authored_statistics_layout() -> VmceStatisticsLayout {
    let columns = [
        RectPx::new(40, 0, 150, 17),
        RectPx::new(200, 0, 70, 17),
        RectPx::new(270, 0, 70, 17),
        RectPx::new(340, 0, 70, 17),
        RectPx::new(410, 0, 80, 17),
        RectPx::new(490, 0, 90, 17),
    ];
    let headers = [
        VmceResultHeader {
            key: "GUI:Player",
            rect: RectPx::new(40, 96, 150, 17),
            centred: false,
        },
        VmceResultHeader {
            key: "GUI:Kills",
            rect: RectPx::new(200, 96, 70, 17),
            centred: true,
        },
        VmceResultHeader {
            key: "GUI:Losses",
            rect: RectPx::new(270, 96, 70, 17),
            centred: true,
        },
        VmceResultHeader {
            key: "GUI:Built",
            rect: RectPx::new(340, 96, 70, 17),
            centred: true,
        },
        VmceResultHeader {
            key: "GUI:Score",
            rect: RectPx::new(410, 96, 80, 17),
            centred: true,
        },
        VmceResultHeader {
            key: "VERA:VmceIncome",
            rect: RectPx::new(490, 96, 90, 17),
            centred: true,
        },
    ];
    let rows = std::array::from_fn(|row| VmceStatisticsRowCells {
        cells: std::array::from_fn(|column| {
            let template = columns[column];
            RectPx::new(template.x, 122 + row as i32 * 34, template.w, 24)
        }),
    });
    VmceStatisticsLayout {
        title: RectPx::new(8, 26, 616, 18),
        total_time: RectPx::new(40, 56, 260, 17),
        par_time: RectPx::new(340, 56, 260, 17),
        headers,
        rows,
    }
}

/// The statistics table at the open page's scale.
pub(crate) fn statistics_layout(scale: ShellScale) -> VmceStatisticsLayout {
    let mut layout = authored_statistics_layout();
    let scalar = |rect: RectPx| scale.scale_rect(rect);
    layout.title = scalar(layout.title);
    layout.total_time = scalar(layout.total_time);
    layout.par_time = scalar(layout.par_time);
    for header in &mut layout.headers {
        header.rect = scalar(header.rect);
    }
    for row in &mut layout.rows {
        for cell in &mut row.cells {
            *cell = scalar(*cell);
        }
    }
    layout
}

/// The BC6 statics VERA's statistics-only defeat page hides: no next mission,
/// description, difficulty prompt or previous-mission/next caption.
pub(crate) fn defeat_hides_static(id: u16) -> bool {
    use crate::ui::coop_score_shell::{DESCRIPTION, DIFFICULTY, NEXT_MISSION, PREVIOUS_MISSION};
    matches!(
        id,
        NEXT_MISSION
            | DESCRIPTION
            | DIFFICULTY
            | 0x0798
            | 0x0799
            | 0x079A
            | 0x079B
            | PREVIOUS_MISSION
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::coop_score_shell::{DESCRIPTION, DIFFICULTY, NEXT_MISSION};

    fn scaled_once(rect: RectPx, scale: ShellScale) -> RectPx {
        scale.scale_rect(rect)
    }

    #[test]
    fn the_board_keeps_the_authored_layer_at_800() {
        let layout = result_layout(ShellScale::RETAIL);
        assert_eq!(layout.frame, RectPx::new(124, 432, 396, 103));
        assert_eq!(layout.rows[3].ready.y, 455 + 60 - 1);
        // Every row sits inside the frame and clears the native stats above.
        for row in layout.rows {
            assert!(row.player.y >= layout.frame.y);
            assert!(
                row.player.y + row.player.h <= layout.frame.y + layout.frame.h,
                "row text stays inside the board frame"
            );
            assert!(row.player.w > 0 && row.player.h > 0);
        }
        // The last row's Ready cell closes the fill, with no strip below it.
        assert_eq!(
            layout.rows[3].ready.y + layout.rows[3].ready.h,
            layout.fill.y + layout.fill.h
        );
    }

    #[test]
    fn the_table_scales_from_its_authored_rects() {
        let base = statistics_layout(ShellScale::RETAIL);
        assert_eq!(base.rows.len(), 4);
        assert_eq!(base.headers[5].key, "VERA:VmceIncome");
        let scale = ShellScale::from_screen(1920, 1080);
        let wide = statistics_layout(scale);
        assert_eq!(
            wide.rows[0].cells[0],
            scaled_once(base.rows[0].cells[0], scale)
        );
        assert!(wide.rows[0].cells[0].w > base.rows[0].cells[0].w);
    }

    #[test]
    fn the_button_projection_follows_the_viewer_and_gate() {
        let rects = button_rects(ShellScale::RETAIL);
        let board = |host: bool, ready: bool, can_continue: bool| VmceResultBoard {
            rows: Vec::new(),
            victory: true,
            local_host: host,
            local_ready: ready,
            can_continue,
        };
        let host = result_buttons(
            &board(true, false, false),
            rects,
            RectPx::new(644, 535, 156, 42),
            0x423,
            false,
        );
        assert_eq!(host.len(), 3);
        assert_eq!(host[0].kind, VmceResultButtonKind::Continue);
        assert!(!host[0].enabled && !host[0].pressed);
        let host_ready = result_buttons(
            &board(true, false, true),
            rects,
            RectPx::new(644, 535, 156, 42),
            0x423,
            true,
        );
        assert!(host_ready[0].enabled && host_ready[0].pressed);
        let guest = result_buttons(
            &board(false, false, false),
            rects,
            RectPx::new(644, 535, 156, 42),
            0x423,
            false,
        );
        assert_eq!(guest[0].kind, VmceResultButtonKind::Ready);
        assert_eq!(guest[0].id, VMCE_READY_CONTROL);
        assert!(guest[0].enabled);
        let voted = result_buttons(
            &board(false, true, false),
            rects,
            RectPx::new(644, 535, 156, 42),
            0x423,
            false,
        );
        assert!(voted[0].pressed, "a cast vote keeps the lit frame");
        assert!(!voted[0].enabled, "a cast vote is not clickable");
    }

    #[test]
    fn the_defeat_page_hides_only_its_prompts() {
        for id in [NEXT_MISSION, DESCRIPTION, DIFFICULTY, 0x0798, 0x079B] {
            assert!(defeat_hides_static(id), "{id:#x}");
        }
        for id in [
            crate::ui::coop_score_shell::TIME,
            crate::ui::coop_score_shell::CONTINUE,
            crate::ui::coop_score_shell::ROW_CELLS[0][0],
        ] {
            assert!(!defeat_hides_static(id), "{id:#x}");
        }
    }
}
