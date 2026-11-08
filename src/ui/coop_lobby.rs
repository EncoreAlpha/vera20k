//! VMCE controls on the existing network setup page geometry.
//!
//! The original page owns the chrome anchors. Only the player columns are
//! rearranged: a map role already determines its House and country. A
//! display-only role board (role / country / ready / host / color) sits right
//! of the participant columns on a native panel/list background; it reuses the
//! native roster pitch, bit font, country flags and color swatches and owns no
//! interaction.

use crate::ui::multiplayer_setup::{
    DIFFICULTY, MultiplayerSetupLayout, SPEED, SetupDialog, VmceLobbyHeader, VmceLobbySidebar,
    VmceLobbySidebarRow, VmceLobbyStatusBar,
};
use crate::ui::shell::geom::{RectPx, ShellScale};

/// Authored 800x600 x of the player-name combo. The VMCE role combo takes the
/// rest of the participant column group.
const PLAYER_COMBO_X: i32 = 74;
const PLAYER_COMBO_W: i32 = 96;
const ROLE_COMBO_X: i32 = 174;
const ROLE_COMBO_W: i32 = 80;
/// Authored 800x600 metrics of the role board. The board frame starts after
/// the 4 px gutter behind the role combo and ends flush against the native
/// right panel's bevel at x 632; the content keeps a 4 px inner padding. The
/// base columns are the approved 92/92/33/33/32 split (Ready and Host share
/// the old Ready+Host width) distributed over the available content width.
const SIDEBAR_X: i32 = 258;
const SIDEBAR_RIGHT: i32 = 632;
const SIDEBAR_TOP: i32 = 10;
const INNER_PAD: i32 = 4;
const SIDEBAR_BASE_COLUMNS: [i32; 5] = [92, 92, 33, 33, 32];
const SIDEBAR_BASE_W: i32 = 282;
/// The native roster row pitch, its combo height, and the board's 2 px
/// vertical padding inside each row.
const ROW_PITCH: i32 = 26;
const ROW_Y: i32 = 36;
const ROW_H: i32 = 24;
const ROW_PAD: i32 = 1;
/// The status strip's native two-line text height plus its 4 px padding.
const STATUS_H: i32 = 42;
const STATUS_GUTTER: i32 = 4;
/// Extra air between the status strip and the reflowed briefing.
const STATUS_BRIEFING_GAP: i32 = 2;
const HEADER_H: i32 = 17;

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
            "VERA:VmceRoleSelect",
            rect(ROLE_COMBO_X, 11, ROLE_COMBO_W, 17),
        ),
    ];
    for (row, columns) in layout.combos.iter_mut().enumerate() {
        let y = ROW_Y + row as i32 * ROW_PITCH;
        columns[0] = rect(PLAYER_COMBO_X, y, PLAYER_COMBO_W, ROW_H);
        columns[4] = rect(ROLE_COMBO_X, y, ROLE_COMBO_W, ROW_H);
        // The board owns the emblems now; keep the retained rect on its
        // country slot so no stale native flag column paints left of it.
        layout.flags[row] = rect(SIDEBAR_X + INNER_PAD + 2, y + ROW_PAD, 18, 20);
    }
    layout.vmce_sidebar = Some(sidebar(scale));

    // The board frame ends at its fourth row plus the 4 px inner padding.
    let board_bottom = SIDEBAR_TOP + INNER_PAD + HEADER_H + 1 + 4 * ROW_PITCH + INNER_PAD;
    let briefing = if status_present {
        let status_y = board_bottom + STATUS_GUTTER;
        layout.vmce_status_bar = Some(VmceLobbyStatusBar {
            frame: rect(SIDEBAR_X, status_y, SIDEBAR_RIGHT - SIDEBAR_X, STATUS_H),
            text: rect(
                SIDEBAR_X + INNER_PAD,
                status_y + INNER_PAD,
                SIDEBAR_RIGHT - SIDEBAR_X - 2 * INNER_PAD,
                STATUS_H - 2 * INNER_PAD,
            ),
        });
        status_y + STATUS_H + STATUS_GUTTER + STATUS_BRIEFING_GAP
    } else {
        layout.vmce_status_bar = None;
        board_bottom + STATUS_GUTTER
    };
    // The briefing ends before the native difficulty caption at y 267.
    layout.cooperative_description = rect(
        PLAYER_COMBO_X,
        briefing,
        SIDEBAR_RIGHT - PLAYER_COMBO_X,
        (262 - briefing).max(0),
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
    // Distribute the approved proportions over the inner width so the
    // columns meet exactly; the last edge is the frame's inner right edge.
    let inner_w = SIDEBAR_RIGHT - SIDEBAR_X - 2 * INNER_PAD;
    let edge = |index: usize| {
        let cumulative: i32 = SIDEBAR_BASE_COLUMNS[..=index].iter().sum();
        SIDEBAR_X + INNER_PAD + ((inner_w * cumulative + SIDEBAR_BASE_W / 2) / SIDEBAR_BASE_W)
    };
    let lefts = [
        SIDEBAR_X + INNER_PAD,
        edge(0),
        edge(1),
        edge(2),
        edge(3),
        SIDEBAR_RIGHT - INNER_PAD,
    ];
    let columns: [(i32, i32); 5] =
        std::array::from_fn(|column| (lefts[column], lefts[column + 1] - lefts[column]));
    let header_y = SIDEBAR_TOP + INNER_PAD;
    let headers = [
        VmceLobbyHeader {
            key: "VERA:VmceRoleColumn",
            // 2 px more than the row content: the caption clears the native
            // list frame's left bevel instead of touching it.
            rect: rect(columns[0].0 + 4, header_y, columns[0].1 - 6, HEADER_H),
            centred: false,
        },
        VmceLobbyHeader {
            key: "VERA:VmceCountryColumn",
            rect: rect(columns[1].0 + 2, header_y, columns[1].1 - 4, HEADER_H),
            centred: false,
        },
        VmceLobbyHeader {
            key: "VERA:VmceReadyColumn",
            rect: rect(
                columns[2].0,
                header_y,
                columns[3].0 + columns[3].1 - columns[2].0,
                HEADER_H,
            ),
            centred: true,
        },
        VmceLobbyHeader {
            key: "VERA:VmceColorColumn",
            rect: rect(columns[4].0, header_y, columns[4].1, HEADER_H),
            centred: true,
        },
    ];
    // The one caption row's dark rule, then four rows on the native pitch.
    let header_rule = rect(SIDEBAR_X + INNER_PAD, header_y + HEADER_H + 1, inner_w, 1);
    let rows = std::array::from_fn(|row| {
        let y = ROW_Y + row as i32 * ROW_PITCH;
        let content = y + ROW_PAD;
        let content_h = ROW_H - 2 * ROW_PAD;
        VmceLobbySidebarRow {
            role: rect(columns[0].0 + 2, content, columns[0].1 - 4, content_h),
            // The native flag PCX carries its country emblem beside the flag
            // (the roster draws it in a 49x21 cell); give it the full native
            // frame plus air, and centre it without clipping.
            country_icon: rect(columns[1].0 + 1, y, 50, ROW_H),
            country: rect(columns[1].0 + 54, content, columns[1].1 - 56, content_h),
            ready: rect(columns[2].0 + 1, content, columns[2].1 - 2, content_h),
            host: rect(columns[3].0 + 1, content, columns[3].1 - 2, content_h),
            color: rect(columns[4].0 + 2, content, columns[4].1 - 4, content_h),
        }
    });
    VmceLobbySidebar {
        frame: rect(
            SIDEBAR_X,
            SIDEBAR_TOP,
            SIDEBAR_RIGHT - SIDEBAR_X,
            (header_y + HEADER_H + 1 + 4 * ROW_PITCH + INNER_PAD) - SIDEBAR_TOP,
        ),
        header_rule,
        headers,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::multiplayer_setup::{ACCEPT, BACK, CHOOSE_MAP, START, SetupFlavor};

    fn coop_layout(
        host: bool,
        flavor: SetupFlavor,
        width: i32,
        height: i32,
        status: bool,
    ) -> MultiplayerSetupLayout {
        let dialog = SetupDialog {
            host,
            flavor,
            modern: true,
        };
        let mut layout = MultiplayerSetupLayout::new(dialog, width, height, true);
        apply(&mut layout, dialog, 4, width, height, status);
        layout
    }

    #[test]
    fn coop_reuses_original_sidebar_anchors_at_multiple_sizes() {
        for (width, height) in [(640, 480), (800, 600), (1920, 1080)] {
            let coop = coop_layout(true, SetupFlavor::Lan, width, height, true);
            let native = MultiplayerSetupLayout::new(
                SetupDialog {
                    host: true,
                    flavor: SetupFlavor::Lan,
                    modern: false,
                },
                width,
                height,
                true,
            );
            assert_eq!(coop.page.title, native.page.title);
            assert_eq!(coop.map_preview, native.map_preview);
            assert_eq!(coop.mode_label, native.mode_label);
            assert_eq!(coop.map_label, native.map_label);
            assert_eq!(coop.chat, native.chat);
            assert_eq!(coop.chat_log, native.chat_log);
            for id in [START, CHOOSE_MAP, BACK] {
                assert_eq!(coop.page.button_rect(id), native.page.button_rect(id));
            }
            // Authored-space comparisons: the mapping may scale the page.
            let scale = coop.page.scale;
            let base = |rect| scale.base_rect(rect);
            let player = base(coop.combos[0][0]);
            let role = base(coop.combos[0][4]);
            assert_eq!(player.x + player.w + 4, role.x);
            let board = coop.vmce_sidebar.expect("VMCE role board");
            // 4 px gutter behind the role combo, then the native panel frame.
            assert_eq!(base(board.frame).x, role.x + role.w + 4);
            assert_eq!(base(board.frame).x, SIDEBAR_X);
            assert!((base(board.frame).x + base(board.frame).w - SIDEBAR_RIGHT).abs() <= 1);
            for (index, header) in board.headers.iter().enumerate() {
                assert!((base(header.rect).y - 14).abs() <= 1);
                assert_eq!(base(header.rect).h, HEADER_H);
                let cell = match index {
                    0 => base(board.rows[0].role),
                    1 => base(board.rows[0].country_icon),
                    2 => base(board.rows[0].ready),
                    _ => base(board.rows[0].color),
                };
                assert!((base(header.rect).x - cell.x).abs() <= 2, "header {index}");
            }
            for row in &board.rows {
                let role_text = base(row.role);
                let country_icon = base(row.country_icon);
                let country_text = base(row.country);
                let ready = base(row.ready);
                let host = base(row.host);
                let color = base(row.color);
                assert!((role_text.y - country_icon.y).abs() <= 2);
                assert!((country_icon.y - ready.y).abs() <= 2);
                assert!((ready.y - host.y).abs() <= 2);
                assert!((host.y - color.y).abs() <= 2);
                assert!(role_text.x + role_text.w <= country_icon.x);
                assert!(country_icon.x + country_icon.w <= country_text.x);
                assert!(country_text.x + country_text.w <= ready.x);
                assert!(ready.x + ready.w <= host.x);
                assert!(host.x + host.w <= color.x);
                assert!((color.x + color.w - (SIDEBAR_RIGHT - INNER_PAD - 2)).abs() <= 1);
                // Rows keep the native pitch and 2 px vertical padding.
                assert!((base(row.role).h - (ROW_H - 2 * ROW_PAD)).abs() <= 1);
            }
            assert!(
                (base(board.rows[1].role).y - (base(board.rows[0].role).y + ROW_PITCH)).abs() <= 1
            );
            let status = coop.vmce_status_bar.expect("status").frame;
            assert!(base(board.frame).y + base(board.frame).h <= base(status).y);
            assert!((base(status).h - STATUS_H).abs() <= 1);
            let text = coop.vmce_status_bar.expect("status").text;
            assert!((base(text).x - (base(status).x + INNER_PAD)).abs() <= 1);
            let briefing = base(coop.cooperative_description);
            assert!(
                (briefing.y - (base(status).y + STATUS_H + STATUS_GUTTER + STATUS_BRIEFING_GAP))
                    .abs()
                    <= 1
            );
            let speed = base(coop.control_rect(SPEED).unwrap());
            assert!(briefing.y + briefing.h <= speed.y);
            assert!(speed.y + speed.h <= base(coop.chat_log).y);

            // Without a message the reserved strip disappears and the briefing
            // starts directly under the board.
            let coop = coop_layout(true, SetupFlavor::Lan, width, height, false);
            let board = coop.vmce_sidebar.expect("board");
            assert!(coop.vmce_status_bar.is_none());
            let briefing = base(coop.cooperative_description);
            assert!(base(board.rows[3].role).y + base(board.rows[3].role).h <= briefing.y);
            let speed = base(coop.control_rect(SPEED).unwrap());
            assert!(briefing.y + briefing.h <= speed.y);
        }
    }

    #[test]
    fn board_columns_meet_exactly_and_keep_the_approved_proportions() {
        let coop = coop_layout(true, SetupFlavor::Lan, 800, 600, false);
        let board = coop.vmce_sidebar.expect("VMCE role board");
        let base = |rect| coop.page.scale.base_rect(rect);
        let row = board.rows[0];
        assert_eq!(base(row.role).x, 264);
        assert_eq!(base(row.role).w, 115);
        assert_eq!(base(row.country_icon).w, 50);
        assert_eq!(base(row.country).w, 64);
        assert_eq!(base(row.ready).x, 502);
        assert_eq!(base(row.ready).w, 41);
        assert_eq!(base(row.host).x, 545);
        assert_eq!(base(row.host).w, 40);
        assert_eq!(base(row.color).x, 588);
        assert_eq!(base(row.color).w, 38);
        for row in &board.rows {
            assert_eq!(base(row.role).h, 22);
        }
        for (row, y) in board.rows.iter().zip([37, 63, 89, 115]) {
            assert_eq!(base(row.role).y, y);
        }
        assert_eq!(base(board.headers[0].rect).x, 266);
        let rule = base(board.header_rule);
        assert_eq!(rule.y, 32);
        assert_eq!(rule.w, 366);
        let frame = base(board.frame);
        assert_eq!((frame.x, frame.y, frame.w, frame.h), (258, 10, 374, 130));
        assert_eq!(
            (base(board.headers[2].rect).x, base(board.headers[2].rect).w),
            (501, 85)
        );
        assert_eq!(
            (base(board.headers[3].rect).x, base(board.headers[3].rect).w),
            (586, 42)
        );
    }

    #[test]
    fn role_combo_keeps_its_own_rect_and_hit_target() {
        let coop = coop_layout(true, SetupFlavor::Lan, 800, 600, false);
        let player = coop.control_rect(0x411).unwrap();
        let role = coop.control_rect(0x76D).unwrap();
        assert_eq!(player, RectPx::new(74, 36, 96, 24));
        assert_eq!(role, RectPx::new(174, 36, 80, 24));
        assert_eq!(player.x + player.w + 4, role.x);
        // The interactive column is Role Select; the display board header
        // stays the plain role word.
        assert!(
            coop.labels
                .iter()
                .any(|(key, _)| *key == "VERA:VmceRoleSelect")
        );
        assert!(
            !coop
                .labels
                .iter()
                .any(|(key, _)| *key == "VERA:VmceCoopRole")
        );
        // The board keeps a further 4 px gutter behind the role combo.
        assert_eq!(
            coop.vmce_sidebar.expect("board").frame.x,
            role.x + role.w + 4
        );
    }

    #[test]
    fn coop_guest_has_the_original_manual_ready_button() {
        for flavor in [SetupFlavor::Lan, SetupFlavor::Wol] {
            let coop = coop_layout(false, flavor, 800, 600, false);
            let original = MultiplayerSetupLayout::new(
                SetupDialog {
                    host: false,
                    flavor,
                    modern: false,
                },
                800,
                600,
                true,
            );
            assert_eq!(coop.control_rect(ACCEPT), original.control_rect(ACCEPT));
            assert!(coop.control_rect(ACCEPT).is_some());
            assert_eq!(coop.control_rect(BACK), original.control_rect(BACK));
            assert!(coop.vmce_sidebar.is_some());
        }
    }
}
