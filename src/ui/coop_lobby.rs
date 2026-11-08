//! VMCE controls on the existing network setup page geometry.
//!
//! The original page owns the chrome anchors. Only the player columns are
//! rearranged: a map role already determines its House and country. A
//! display-only role board (role / country / ready+host / color) sits right of
//! the participant columns; it reuses the native roster pitch, bit font,
//! country flags and color swatches and owns no interaction.

use crate::ui::multiplayer_setup::{
    DIFFICULTY, MultiplayerSetupLayout, SPEED, SetupDialog, VmceLobbySidebar, VmceLobbySidebarRow,
    VmceLobbyStatusBar,
};
use crate::ui::shell::geom::{RectPx, ShellScale};

/// Authored 800x600 x of the player-name combo. The VMCE role combo takes the
/// rest of the participant column group.
const PLAYER_COMBO_X: i32 = 74;
const PLAYER_COMBO_W: i32 = 96;
const ROLE_COMBO_X: i32 = 174;
const ROLE_COMBO_W: i32 = 80;
/// Authored 800x600 metrics of the role board. The base columns are the
/// approved 92/92/66/32; the spare width between the board and the right
/// panel's frame (x 540..628) is distributed proportionally so no blank band
/// is left between the group and the native panel.
const SIDEBAR_X: i32 = 258;
const SIDEBAR_BASE_W: i32 = 282;
const SIDEBAR_RIGHT: i32 = 628;
const SIDEBAR_BASE_COLUMNS: [i32; 4] = [92, 92, 66, 32];
/// The native roster row pitch and the status bar's two-line height (the
/// native GAME.FNT cell advance is 17 px).
const ROW_PITCH: i32 = 26;
const ROW_Y: i32 = 36;
const ROW_H: i32 = 24;
const STATUS_H: i32 = 38;
const STATUS_GUTTER: i32 = 6;

pub(crate) fn apply(
    layout: &mut MultiplayerSetupLayout,
    dialog: SetupDialog,
    row_count: usize,
    screen_width: i32,
    screen_height: i32,
    status_present: bool,
) {
    // Use the same responsive mapping and resource anchors as the original
    // network setup, including its title, preview, labels and sidebar buttons.
    let native = MultiplayerSetupLayout::new(
        SetupDialog {
            modern: false,
            ..dialog
        },
        screen_width,
        screen_height,
        true,
    );
    let scale = native.page.scale;
    layout.page.scale = scale;
    layout.page.title = native.page.title;
    layout.page.status_help = native.page.status_help;
    layout.map_preview = native.map_preview;
    layout.no_preview = native.no_preview;
    layout.mode_label = native.mode_label;
    layout.map_label = native.map_label;
    layout.chat = native.chat;
    layout.chat_log = native.chat_log;
    layout.emote = native.emote;
    layout.acceptance = native.acceptance;
    layout.ping_icons = native.ping_icons;
    // Modern guest rooms normally replace Accept with Game Options and rely
    // on automatic acceptance. Coop uses the original manual-ready buttons.
    layout.page.buttons.clone_from(&native.page.buttons);

    let rect = |x, y, w, h| scale.scale_rect(RectPx::new(x, y, w, h));
    layout.labels = vec![
        ("GUI:Players", rect(PLAYER_COMBO_X, 11, PLAYER_COMBO_W, 17)),
        (
            "VERA:VmceCoopRole",
            rect(ROLE_COMBO_X, 11, ROLE_COMBO_W, 17),
        ),
    ];
    for (row, columns) in layout.combos.iter_mut().enumerate() {
        let y = ROW_Y + row as i32 * ROW_PITCH;
        columns[0] = rect(PLAYER_COMBO_X, y, PLAYER_COMBO_W, ROW_H);
        columns[4] = rect(ROLE_COMBO_X, y, ROLE_COMBO_W, ROW_H);
        // The board owns the flags now; keep the retained rect on its country
        // cell so no stale native flag column paints left of it.
        layout.flags[row] = rect(SIDEBAR_X + 4, y + 3, 18, 17);
    }
    layout.vmce_sidebar = Some(sidebar(scale));

    // The status bar sits directly under the four-row board with a small
    // gutter when the room has something to report; the mission briefing
    // follows it. Without a message the briefing takes the space back so the
    // page has no empty strip between the roster and the briefing.
    let board_bottom = ROW_Y + 4 * ROW_PITCH;
    let briefing_y = if status_present {
        let status_y = board_bottom + STATUS_GUTTER;
        layout.vmce_status_bar = Some(VmceLobbyStatusBar {
            frame: rect(
                PLAYER_COMBO_X,
                status_y,
                SIDEBAR_RIGHT - PLAYER_COMBO_X,
                STATUS_H,
            ),
        });
        status_y + STATUS_H + STATUS_GUTTER
    } else {
        layout.vmce_status_bar = None;
        board_bottom + STATUS_GUTTER
    };
    layout.cooperative_description = rect(
        PLAYER_COMBO_X,
        briefing_y,
        SIDEBAR_RIGHT - PLAYER_COMBO_X,
        if status_present { 96 } else { 140 },
    );
    for (id, bounds) in &mut layout.trackbars {
        if *id == DIFFICULTY {
            *bounds = rect(180, 290, 266, 22);
        } else if *id == SPEED {
            *bounds = rect(300, 337, 129, 21);
        }
    }
    // A VMCE room's capacity is at most four authored roles, but keep the
    // ordinary paging placement if a larger roster is ever admitted.
    let bottom = ROW_Y + row_count.min(4) as i32 * ROW_PITCH + 2;
    layout.roster_navigation = [
        rect(PLAYER_COMBO_X, bottom, 94, 18),
        rect(446, bottom, 94, 18),
    ];
    layout.roster_page_label = rect(176, bottom, 262, 18);
}

fn sidebar(scale: ShellScale) -> VmceLobbySidebar {
    let rect = |x, y, w, h| scale.scale_rect(RectPx::new(x, y, w, h));
    // Widen the approved 282 px group into the spare strip left of the native
    // right panel, keeping the approved proportions. Each edge is rounded
    // from the cumulative proportion so the columns meet exactly.
    let widened = SIDEBAR_RIGHT - SIDEBAR_X;
    let edge = |index: usize| {
        let cumulative: i32 = SIDEBAR_BASE_COLUMNS[..=index].iter().sum();
        SIDEBAR_X + ((widened * cumulative + SIDEBAR_BASE_W / 2) / SIDEBAR_BASE_W)
    };
    let lefts = [SIDEBAR_X, edge(0), edge(1), edge(2), SIDEBAR_RIGHT];
    let columns: [(i32, i32); 4] =
        std::array::from_fn(|column| (lefts[column], lefts[column + 1] - lefts[column]));
    let headers = [
        (
            "VERA:VmceRoleColumn",
            rect(columns[0].0, 11, columns[0].1, 17),
        ),
        (
            "VERA:VmceCountryColumn",
            rect(columns[1].0, 11, columns[1].1, 17),
        ),
        (
            "VERA:VmceReadyColumn",
            rect(columns[2].0, 11, columns[2].1, 17),
        ),
        (
            "VERA:VmceColorColumn",
            rect(columns[3].0, 11, columns[3].1, 17),
        ),
    ];
    let rows = std::array::from_fn(|row| {
        let y = ROW_Y + row as i32 * ROW_PITCH;
        VmceLobbySidebarRow {
            role: rect(columns[0].0, y, columns[0].1, ROW_H),
            country: rect(columns[1].0, y, columns[1].1, ROW_H),
            status: rect(columns[2].0, y, columns[2].1, ROW_H),
            color: rect(columns[3].0, y, columns[3].1, ROW_H),
        }
    });
    VmceLobbySidebar { headers, rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::multiplayer_setup::{ACCEPT, BACK, CHOOSE_MAP, START, SetupFlavor};

    #[test]
    fn coop_reuses_original_sidebar_anchors_at_multiple_sizes() {
        let dialog = SetupDialog {
            host: true,
            flavor: SetupFlavor::Lan,
            modern: true,
        };
        for (width, height) in [(640, 480), (800, 600), (1920, 1080)] {
            let mut coop = MultiplayerSetupLayout::new(dialog, width, height, true);
            let native = MultiplayerSetupLayout::new(
                SetupDialog {
                    modern: false,
                    ..dialog
                },
                width,
                height,
                true,
            );
            apply(&mut coop, dialog, 4, width, height, true);
            assert_eq!(coop.page.title, native.page.title);
            assert_eq!(coop.map_preview, native.map_preview);
            assert_eq!(coop.mode_label, native.mode_label);
            assert_eq!(coop.map_label, native.map_label);
            assert_eq!(coop.chat, native.chat);
            assert_eq!(coop.chat_log, native.chat_log);
            for id in [START, CHOOSE_MAP, BACK] {
                assert_eq!(coop.page.button_rect(id), native.page.button_rect(id));
            }
            // Participant columns, then a gutter, then the display board.
            // Compare in authored 800x600 pixels: the layout's mapping may
            // scale the page even at the retail window sizes (640 uses the
            // 640-wide shell art), so device-space gaps are not integers.
            let scale = coop.page.scale;
            let base = |rect| scale.base_rect(rect);
            let player = base(coop.combos[0][0]);
            let role = base(coop.combos[0][4]);
            assert_eq!(player.x + player.w + 4, role.x);
            let board = coop.vmce_sidebar.expect("VMCE role board");
            assert_eq!(base(board.rows[0].role).x, role.x + role.w + 4);
            for row in &board.rows {
                let cells = [
                    base(row.role),
                    base(row.country),
                    base(row.status),
                    base(row.color),
                ];
                assert!(role.x + role.w < cells[0].x);
                // The board's columns meet exactly, with no overlap.
                for pair in cells.windows(2) {
                    assert_eq!(pair[0].x + pair[0].w, pair[1].x);
                }
                assert_eq!(cells[3].x + cells[3].w, 628);
            }
            // The status bar follows the board directly and the briefing
            // follows the bar; the native speed row stays below the briefing.
            let status = base(coop.vmce_status_bar.expect("status").frame);
            assert!(base(board.rows[3].role).y + base(board.rows[3].role).h < status.y);
            let briefing = base(coop.cooperative_description);
            assert!(status.y + status.h < briefing.y);
            let speed = base(coop.control_rect(SPEED).unwrap());
            assert!(briefing.y + briefing.h < speed.y);
            assert!(speed.y + speed.h < base(coop.chat_log).y);

            // Without a message the reserved status strip disappears and the
            // briefing starts directly under the board.
            apply(&mut coop, dialog, 4, width, height, false);
            assert!(coop.vmce_status_bar.is_none());
            let briefing = base(coop.cooperative_description);
            assert!(base(board.rows[3].role).y + base(board.rows[3].role).h < briefing.y);
            assert_eq!(briefing.y, 146);
            let speed = base(coop.control_rect(SPEED).unwrap());
            assert!(briefing.y + briefing.h < speed.y);
        }
    }

    #[test]
    fn board_columns_meet_exactly_and_keep_the_approved_proportions() {
        let dialog = SetupDialog {
            host: true,
            flavor: SetupFlavor::Lan,
            modern: true,
        };
        let mut layout = MultiplayerSetupLayout::new(dialog, 800, 600, true);
        apply(&mut layout, dialog, 4, 800, 600, true);
        let board = layout.vmce_sidebar.expect("VMCE role board");
        let first = board.rows[0];
        assert_eq!(first.role.x, SIDEBAR_X);
        assert_eq!(first.role.w, 121);
        assert_eq!(first.country.w, 120);
        assert_eq!(first.status.w, 87);
        assert_eq!(first.color.w, 42);
        assert_eq!(first.color.x + first.color.w, SIDEBAR_RIGHT);
        for (row, expected_y) in board.rows.iter().zip([36, 62, 88, 114].into_iter()) {
            for cell in [row.role, row.country, row.status, row.color] {
                assert_eq!(cell.y, expected_y);
                assert_eq!(cell.h, 24);
            }
        }
        // Headers keep the native label row and share their column's edges.
        let row = board.rows[0];
        for (header, cell) in
            board
                .headers
                .iter()
                .zip([row.role, row.country, row.status, row.color])
        {
            assert_eq!(header.1.y, 11);
            assert_eq!(header.1.h, 17);
            assert_eq!(header.1.x, cell.x);
        }
    }

    #[test]
    fn role_combo_keeps_its_own_rect_and_hit_target() {
        let dialog = SetupDialog {
            host: true,
            flavor: SetupFlavor::Lan,
            modern: true,
        };
        let mut coop = MultiplayerSetupLayout::new(dialog, 800, 600, true);
        apply(&mut coop, dialog, 4, 800, 600, false);
        let player = coop.control_rect(0x411).unwrap();
        let role = coop.control_rect(0x76D).unwrap();
        assert_eq!(player, RectPx::new(74, 36, 96, 24));
        assert_eq!(role, RectPx::new(174, 36, 80, 24));
        assert_eq!(player.x + player.w + 4, role.x);
        // The board starts after a further 4 px gutter.
        assert_eq!(
            coop.vmce_sidebar.expect("board").rows[0].role.x,
            role.x + role.w + 4
        );
    }

    #[test]
    fn coop_guest_has_the_original_manual_ready_button() {
        for flavor in [SetupFlavor::Lan, SetupFlavor::Wol] {
            let dialog = SetupDialog {
                host: false,
                flavor,
                modern: true,
            };
            let mut coop = MultiplayerSetupLayout::new(dialog, 800, 600, true);
            let original = MultiplayerSetupLayout::new(
                SetupDialog {
                    modern: false,
                    ..dialog
                },
                800,
                600,
                true,
            );
            apply(&mut coop, dialog, 2, 800, 600, false);
            assert_eq!(coop.control_rect(ACCEPT), original.control_rect(ACCEPT));
            assert!(coop.control_rect(ACCEPT).is_some());
            assert_eq!(coop.control_rect(BACK), original.control_rect(BACK));
            assert!(coop.vmce_sidebar.is_some());
        }
    }
}
