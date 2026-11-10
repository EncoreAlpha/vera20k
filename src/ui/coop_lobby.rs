//! VMCE controls on the existing network setup page geometry.
//!
//! The original page owns the chrome anchors. Only the player columns are
//! rearranged: the owner combo on each row selects the campaign role, and a
//! map role already determines its House and country. A display-only owner
//! board (country / ready / color) sits right of the participant columns on a
//! native panel/list background; it reuses the native roster pitch, bit font,
//! country flags, ready glyph and color swatches and owns no interaction.

use crate::ui::multiplayer_setup::{
    DIFFICULTY, MultiplayerSetupLayout, SPEED, SetupDialog, VmceLobbyHeader, VmceLobbySidebar,
    VmceLobbySidebarRow, VmceLobbyStatusBar,
};
use crate::ui::shell::geom::{RectPx, ShellScale};

/// Authored 800x600 x of the player-name combo. The VMCE owner combo takes the
/// rest of the participant column group.
const PLAYER_COMBO_X: i32 = 74;
const PLAYER_COMBO_W: i32 = 96;
const OWNER_COMBO_X: i32 = 174;
/// Wide enough for the authored House names: the 20 px arrow reserve leaves a
/// 100 px text face, where `Americans House` and `YuriCountry House` fit.
const OWNER_COMBO_W: i32 = 120;
/// Authored 800x600 metrics of the owner board. The board frame starts after
/// the 4 px gutter behind the owner combo and ends flush against the native
/// right panel's bevel at x 632; the content keeps a 4 px inner padding. The
/// separate role column is gone: country starts at the content edge, the
/// country text owns the wide middle, one narrow Ready glyph cell follows, and
/// the color swatch keeps its native 586..628 slot.
const SIDEBAR_X: i32 = 298;
const SIDEBAR_RIGHT: i32 = 632;
const SIDEBAR_TOP: i32 = 10;
const INNER_PAD: i32 = 4;
/// Content starts at x 302; the country emblem keeps the native 50 px cell.
const CONTENT_X: i32 = SIDEBAR_X + INNER_PAD;
const COUNTRY_ICON_W: i32 = 50;
const COUNTRY_TEXT_X: i32 = CONTENT_X + 54;
const COUNTRY_TEXT_W: i32 = 150;
/// One Ready cell for the host H-in-star / guest accepted star.
const READY_X: i32 = 514;
const READY_W: i32 = 48;
/// The color column keeps its approved 586..628 region.
const COLOR_X: i32 = 586;
const COLOR_W: i32 = SIDEBAR_RIGHT - INNER_PAD - COLOR_X;
/// The native roster row pitch, its combo height, and the board's 1 px padding
/// above and below each row's content (2 px total).
const ROW_PITCH: i32 = 26;
const ROW_Y: i32 = 36;
const ROW_H: i32 = 24;
const ROW_PAD: i32 = 1;
/// The captions and the left labels share this y so both rows read on one
/// baseline.
const HEADER_Y: i32 = SIDEBAR_TOP + INNER_PAD;
/// The status strip's native two-line text height plus its 4 px padding.
const STATUS_H: i32 = 48;
/// The flat fill's inset inside the native list frame.
const FILL_BORDER: i32 = 2;
const STATUS_GUTTER: i32 = 4;
/// Extra air between the status strip and the reflowed briefing.
const STATUS_BRIEFING_GAP: i32 = 2;
const HEADER_H: i32 = 17;

pub(crate) fn apply(
    layout: &mut MultiplayerSetupLayout,
    dialog: SetupDialog,
    row_count: usize,
    role_count: usize,
    screen_width: i32,
    screen_height: i32,
    status_present: bool,
) {
    let role_count = role_count.clamp(1, 4);
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
        (
            "GUI:Players",
            rect(PLAYER_COMBO_X, HEADER_Y, PLAYER_COMBO_W, 17),
        ),
        (
            "VERA:VmceOwner",
            rect(OWNER_COMBO_X, HEADER_Y, OWNER_COMBO_W, 17),
        ),
    ];
    for (row, columns) in layout.combos.iter_mut().enumerate() {
        let y = ROW_Y + row as i32 * ROW_PITCH;
        columns[0] = rect(PLAYER_COMBO_X, y, PLAYER_COMBO_W, ROW_H);
        columns[4] = rect(OWNER_COMBO_X, y, OWNER_COMBO_W, ROW_H);
    }
    layout.vmce_sidebar = Some(sidebar(scale, role_count));

    // The board frame ends at its last authored-role row plus the 4 px inner
    // padding.
    let board_bottom =
        SIDEBAR_TOP + INNER_PAD + HEADER_H + 1 + role_count as i32 * ROW_PITCH + INNER_PAD;
    let briefing = if status_present {
        let status_y = board_bottom + STATUS_GUTTER;
        let status_w = SIDEBAR_RIGHT - PLAYER_COMBO_X;
        layout.vmce_status_bar = Some(VmceLobbyStatusBar {
            frame: rect(PLAYER_COMBO_X, status_y, status_w, STATUS_H),
            fill: rect(
                PLAYER_COMBO_X + FILL_BORDER,
                status_y + FILL_BORDER,
                status_w - 2 * FILL_BORDER,
                STATUS_H - 2 * FILL_BORDER,
            ),
            text: rect(
                PLAYER_COMBO_X + FILL_BORDER + INNER_PAD,
                status_y + FILL_BORDER + INNER_PAD,
                status_w - 2 * (FILL_BORDER + INNER_PAD),
                STATUS_H - 2 * (FILL_BORDER + INNER_PAD),
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
    // Centre the three captions over the slider's thirds on one baseline four
    // pixels above the track; the last caption still ends inside the track.
    layout.difficulty_labels = Some(std::array::from_fn(|index| {
        let x0 = 180 + 266 * index as i32 / 3;
        let x1 = 180 + 266 * (index as i32 + 1) / 3;
        rect(x0, 271, x1 - x0, 17)
    }));
    // A VMCE room's capacity follows the selected map's authored roles.
    let bottom = ROW_Y + row_count.min(role_count) as i32 * ROW_PITCH + 2;
    layout.roster_navigation = [
        rect(PLAYER_COMBO_X, bottom, 94, 18),
        rect(446, bottom, 94, 18),
    ];
    layout.roster_page_label = rect(176, bottom, 262, 18);
}

fn sidebar(scale: ShellScale, role_count: usize) -> VmceLobbySidebar {
    let role_count = role_count.clamp(1, 4);
    let rect = |x, y, w, h| scale.scale_rect(RectPx::new(x, y, w, h));
    let header_y = HEADER_Y;
    let headers = [
        VmceLobbyHeader {
            key: "VERA:VmceCountryColumn",
            // 4 px inside the flat panel fill, over the country icon and text.
            rect: rect(
                CONTENT_X + 2,
                header_y,
                COUNTRY_TEXT_X + COUNTRY_TEXT_W - (CONTENT_X + 2),
                HEADER_H,
            ),
            centred: false,
        },
        VmceLobbyHeader {
            key: "VERA:VmceReady",
            rect: rect(READY_X, header_y, READY_W, HEADER_H),
            centred: true,
        },
        VmceLobbyHeader {
            key: "VERA:VmceColorColumn",
            rect: rect(COLOR_X, header_y, COLOR_W, HEADER_H),
            centred: true,
        },
    ];
    // The one caption row's dark rule, then one row per authored primary role
    // on the native pitch.
    let header_rule = rect(
        CONTENT_X,
        header_y + HEADER_H + 1,
        SIDEBAR_RIGHT - INNER_PAD - CONTENT_X,
        1,
    );
    let rows = (0..role_count)
        .map(|row| {
            let y = ROW_Y + row as i32 * ROW_PITCH;
            let content = y + ROW_PAD;
            let content_h = ROW_H - 2 * ROW_PAD;
            VmceLobbySidebarRow {
                // The native flag PCX carries its country emblem beside the
                // flag (the roster draws it in a 49x21 cell); give it the full
                // native frame plus air, and centre it without clipping.
                country_icon: rect(CONTENT_X, y, COUNTRY_ICON_W, ROW_H),
                country: rect(COUNTRY_TEXT_X, content, COUNTRY_TEXT_W, content_h),
                ready: rect(READY_X, content, READY_W, content_h),
                color: rect(COLOR_X + 2, content, COLOR_W - 4, content_h),
            }
        })
        .collect::<Vec<_>>();
    let board_h =
        (header_y + HEADER_H + 1 + role_count as i32 * ROW_PITCH + INNER_PAD) - SIDEBAR_TOP;
    let frame = rect(SIDEBAR_X, SIDEBAR_TOP, SIDEBAR_RIGHT - SIDEBAR_X, board_h);
    VmceLobbySidebar {
        frame,
        fill: scale.scale_rect(RectPx::new(
            SIDEBAR_X + FILL_BORDER,
            SIDEBAR_TOP + FILL_BORDER,
            SIDEBAR_RIGHT - SIDEBAR_X - 2 * FILL_BORDER,
            board_h - 2 * FILL_BORDER,
        )),
        header_rule,
        headers,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::multiplayer_setup::{ACCEPT, BACK, CHOOSE_MAP, START, SetupFlavor};
    #[cfg(feature = "vmce")]
    use crate::ui::multiplayer_setup::{VmceReadyGlyph, vmce_ready_glyph};

    fn coop_layout(
        host: bool,
        flavor: SetupFlavor,
        width: i32,
        height: i32,
        status: bool,
    ) -> MultiplayerSetupLayout {
        coop_layout_with_roles(host, flavor, width, height, status, 4)
    }

    fn coop_layout_with_roles(
        host: bool,
        flavor: SetupFlavor,
        width: i32,
        height: i32,
        status: bool,
        role_count: usize,
    ) -> MultiplayerSetupLayout {
        let dialog = SetupDialog {
            host,
            flavor,
            modern: true,
        };
        let mut layout = MultiplayerSetupLayout::new(dialog, width, height, true);
        apply(
            &mut layout,
            dialog,
            role_count,
            role_count,
            width,
            height,
            status,
        );
        layout
    }

    #[test]
    fn the_board_and_roster_follow_the_maps_authored_role_count() {
        let coop = coop_layout_with_roles(true, SetupFlavor::Lan, 800, 600, false, 2);
        let board = coop.vmce_sidebar.as_ref().expect("board");
        assert_eq!(board.rows.len(), 2);
        let frame = coop.page.scale.base_rect(board.frame);
        assert_eq!(
            frame.h,
            (HEADER_Y + HEADER_H + 1 + 2 * ROW_PITCH + INNER_PAD) - SIDEBAR_TOP
        );
        // One row shorter than the four-role board: the briefing starts higher.
        let briefing = coop.page.scale.base_rect(coop.cooperative_description);
        assert_eq!(
            briefing.y,
            SIDEBAR_TOP + INNER_PAD + HEADER_H + 1 + 2 * ROW_PITCH + INNER_PAD + STATUS_GUTTER
        );
        // The roster navigation follows the same two slots.
        assert_eq!(coop.control_rect(0x411).unwrap().h, ROW_H);
        assert_eq!(
            coop.page.scale.base_rect(coop.roster_navigation[0]).y,
            ROW_Y + 2 * ROW_PITCH + 2
        );
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
            let owner = base(coop.combos[0][4]);
            assert_eq!(player.x + player.w + 4, owner.x);
            let board = coop.vmce_sidebar.as_ref().expect("VMCE owner board");
            // 4 px gutter behind the owner combo, then the native panel frame.
            assert_eq!(base(board.frame).x, owner.x + owner.w + 4);
            assert_eq!(base(board.frame).x, SIDEBAR_X);
            assert!((base(board.frame).x + base(board.frame).w - SIDEBAR_RIGHT).abs() <= 1);
            // Country / Ready / Color, no separate role or host column.
            assert_eq!(board.headers.len(), 3);
            assert_eq!(board.headers[0].key, "VERA:VmceCountryColumn");
            assert_eq!(board.headers[1].key, "VERA:VmceReady");
            assert_eq!(board.headers[2].key, "VERA:VmceColorColumn");
            for (index, header) in board.headers.iter().enumerate() {
                assert!((base(header.rect).y - 14).abs() <= 1);
                assert_eq!(base(header.rect).h, HEADER_H);
                let cell = match index {
                    0 => base(board.rows[0].country_icon),
                    1 => base(board.rows[0].ready),
                    _ => base(board.rows[0].color),
                };
                assert!((base(header.rect).x - cell.x).abs() <= 2, "header {index}");
            }
            for row in &board.rows {
                let icon = base(row.country_icon);
                let country = base(row.country);
                let ready = base(row.ready);
                let color = base(row.color);
                assert!((icon.y - country.y).abs() <= 2);
                assert!((country.y - ready.y).abs() <= 2);
                assert!((ready.y - color.y).abs() <= 2);
                assert!(icon.x + icon.w <= country.x);
                assert!(country.x + country.w <= ready.x);
                assert!(ready.x + ready.w <= color.x);
                assert!((color.x + color.w - (SIDEBAR_RIGHT - INNER_PAD - 2)).abs() <= 1);
                assert!(color.x + color.w <= SIDEBAR_RIGHT - INNER_PAD);
                // Rows keep the native pitch and 2 px vertical padding.
                assert!((country.h - (ROW_H - 2 * ROW_PAD)).abs() <= 1);
            }
            assert!(
                (base(board.rows[1].country_icon).y
                    - (base(board.rows[0].country_icon).y + ROW_PITCH))
                    .abs()
                    <= 1
            );
            let status = coop.vmce_status_bar.expect("status").frame;
            assert!(base(board.frame).y + base(board.frame).h <= base(status).y);
            assert!((base(status).h - STATUS_H).abs() <= 1);
            let difficulty = base(coop.control_rect(DIFFICULTY).expect("difficulty track"));
            assert!(
                (difficulty.x - 180).abs() <= 1
                    && (difficulty.y - 290).abs() <= 1
                    && (difficulty.w - 266).abs() <= 1
                    && (difficulty.h - 22).abs() <= 1,
                "difficulty track moved: {difficulty:?}"
            );
            let labels = coop.difficulty_labels.expect("difficulty captions");
            let base_track = difficulty;
            for (index, label) in labels.iter().enumerate() {
                let label = base(*label);
                let centre = label.x + label.w / 2;
                let expected = base_track.x + base_track.w * (2 * index as i32 + 1) / 6;
                assert!(
                    (centre - expected).abs() <= 3,
                    "caption {index} centre: {centre} vs {expected}"
                );
                assert!(
                    (label.y + label.h - (base_track.y - 2)).abs() <= 2,
                    "caption {index} baseline"
                );
                assert!((label.y - base(labels[0]).y).abs() <= 2);
                assert!((label.h - base(labels[0]).h).abs() <= 1);
            }
            let last = base(labels[2]);
            assert!(last.x + last.w <= base_track.x + base_track.w);
            let text = coop.vmce_status_bar.expect("status").text;
            assert!((base(text).x - (base(status).x + FILL_BORDER + INNER_PAD)).abs() <= 1);
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
            let board = coop.vmce_sidebar.as_ref().expect("board");
            assert!(coop.vmce_status_bar.is_none());
            let briefing = base(coop.cooperative_description);
            let last_row = base(board.rows[3].color);
            assert!(last_row.y + last_row.h <= briefing.y);
            let speed = base(coop.control_rect(SPEED).unwrap());
            assert!(briefing.y + briefing.h <= speed.y);
        }
    }

    #[test]
    fn owner_board_columns_cover_the_content_width_without_overlap() {
        let coop = coop_layout(true, SetupFlavor::Lan, 800, 600, false);
        let board = coop.vmce_sidebar.as_ref().expect("VMCE owner board");
        let base = |rect| coop.page.scale.base_rect(rect);
        let row = board.rows[0];
        assert_eq!(base(row.country_icon), RectPx::new(302, 36, 50, 24));
        assert_eq!(base(row.country), RectPx::new(356, 37, 150, 22));
        assert_eq!(base(row.ready), RectPx::new(514, 37, 48, 22));
        assert_eq!(base(row.color), RectPx::new(588, 37, 38, 22));
        assert_eq!(base(board.headers[0].rect), RectPx::new(304, 14, 202, 17));
        assert_eq!(base(board.headers[1].rect), RectPx::new(514, 14, 48, 17));
        assert_eq!(base(board.headers[2].rect), RectPx::new(586, 14, 42, 17));
        let rule = base(board.header_rule);
        assert_eq!((rule.x, rule.y, rule.w, rule.h), (302, 32, 326, 1));
        let frame = base(board.frame);
        assert_eq!((frame.x, frame.y, frame.w, frame.h), (298, 10, 334, 130));
        for (index, row) in board.rows.iter().enumerate() {
            let y = 36 + index as i32 * 26;
            assert_eq!(base(row.country_icon), RectPx::new(302, y, 50, 24));
            assert_eq!(base(row.country), RectPx::new(356, y + 1, 150, 22));
            assert_eq!(base(row.ready), RectPx::new(514, y + 1, 48, 22));
            assert_eq!(base(row.color), RectPx::new(588, y + 1, 38, 22));
        }
        for row in &board.rows {
            let icon = base(row.country_icon);
            let country = base(row.country);
            let ready = base(row.ready);
            let color = base(row.color);
            assert!(icon.x + icon.w <= country.x, "country icon clips the text");
            assert!(
                country.x + country.w <= ready.x,
                "country text reaches Ready"
            );
            assert!(
                ready.x + ready.w <= color.x,
                "Ready reaches the color column"
            );
            assert_eq!(color.x + color.w, 626);
            assert!(color.x + color.w <= SIDEBAR_RIGHT - INNER_PAD);
        }
    }

    #[test]
    fn owner_combo_keeps_its_own_rect_and_hit_target() {
        let coop = coop_layout(true, SetupFlavor::Lan, 800, 600, false);
        let player = coop.control_rect(0x411).unwrap();
        let owner = coop.control_rect(0x76D).unwrap();
        assert_eq!(player, RectPx::new(74, 36, 96, 24));
        assert_eq!(owner, RectPx::new(174, 36, 120, 24));
        assert_eq!(player.x + player.w + 4, owner.x);
        // The authored House names need a readable text face: the arrow reserve
        // is 20 px, so the cell must leave at least 100 px for the text.
        assert!(
            coop.page.scale.base_rect(owner).w - 20 >= 100,
            "owner text face: {owner:?}"
        );
        // The list labels and the board captions share one baseline.
        for (_, label) in &coop.labels {
            assert_eq!(coop.page.scale.base_rect(*label).y, HEADER_Y);
        }
        let headers = coop.vmce_sidebar.as_ref().expect("board").headers;
        for header in headers {
            assert_eq!(coop.page.scale.base_rect(header.rect).y, HEADER_Y);
        }
        // The interactive column is Owner; the board's separate role captions
        // are gone.
        assert!(coop.labels.iter().any(|(key, _)| *key == "VERA:VmceOwner"));
        assert!(
            !coop
                .labels
                .iter()
                .any(|(key, _)| *key == "VERA:VmceRoleSelect")
        );
        assert!(
            !coop
                .labels
                .iter()
                .any(|(key, _)| *key == "VERA:VmceCoopRole")
        );
        assert!(
            !coop
                .vmce_sidebar
                .as_ref()
                .expect("board")
                .headers
                .iter()
                .any(|header| header.key == "VERA:VmceRoleColumn")
        );
        // The board keeps a further 4 px gutter behind the owner combo.
        assert_eq!(
            coop.vmce_sidebar.as_ref().expect("board").frame.x,
            owner.x + owner.w + 4
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn ready_cell_gives_the_host_the_star_and_leaves_a_waiting_guest_blank() {
        assert_eq!(vmce_ready_glyph(true, false), VmceReadyGlyph::Host);
        assert_eq!(vmce_ready_glyph(true, true), VmceReadyGlyph::Host);
        assert_eq!(vmce_ready_glyph(false, false), VmceReadyGlyph::Blank);
        assert_eq!(vmce_ready_glyph(false, true), VmceReadyGlyph::Ready);
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
