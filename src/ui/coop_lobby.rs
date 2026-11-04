//! Geometry for VERA's own cooperative room.
//!
//! This is presentation only: controls retain their multiplayer setup IDs,
//! hit-testing emits the shared setup actions, and every displayed value is
//! projected from `SetupView`.

use crate::ui::multiplayer_setup::MultiplayerSetupLayout;
use crate::ui::shell::geom::{RectPx, SHELL_BASE_H, SHELL_BASE_W, ShellScale};

pub(crate) fn apply(
    layout: &mut MultiplayerSetupLayout,
    row_count: usize,
    screen_width: i32,
    screen_height: i32,
) {
    // Native 640x480 keeps an identity ShellScale for retail dialogs; Coop uses
    // one fit scale for its geometry and the renderer's text/child controls.
    let fit = ShellScale::from_screen(screen_width, screen_height);
    layout.page.scale = fit;
    let factor = fit.factor();
    let logical_width = (screen_width as f32 / factor).round() as i32;
    let logical_height = (screen_height as f32 / factor).round() as i32;
    layout.page.status_help = fit.scale_rect(crate::ui::shell::layout::status_line_rect(
        RectPx::new(2, 355, 410, 12),
        SHELL_BASE_W,
        SHELL_BASE_H,
    ));
    let rect = |x, y, w, h| {
        let edge = |value: i32| (value as f32 * factor).round() as i32;
        let x0 = edge(x);
        let y0 = edge(y);
        let x1 = edge(x + w);
        let y1 = edge(y + h);
        RectPx::new(x0, y0, x1 - x0, y1 - y0)
    };

    let margin = (logical_width as f32 * 0.03).round() as i32;
    let left = margin + 18;
    let rail_width = (logical_width as f32 * 0.18).round() as i32;
    let rail_x = logical_width - margin - rail_width;
    let table_right = rail_x - 80;
    let gap = 8;
    // The map defines each role's House and country; only the participant and
    // role are selectable here. Keep the other column rectangles inert.
    let available_width = (table_right - left - gap).max(200);
    let player_width = available_width * 45 / 100;
    let role_x = left + player_width + gap;
    let role_width = available_width - player_width;
    layout.page.title = rect(margin, 12, rail_x - margin, 20);
    layout.labels = vec![
        ("GUI:Players", rect(left, 91, player_width, 17)),
        ("GUI:Team", rect(role_x, 91, role_width, 17)),
    ];

    let shown = row_count.min(layout.combos.len()).max(1);
    let chat_y = logical_height - 101;
    let chat_log_height = (logical_height as f32 * 0.152).round() as i32;
    let chat_log_y = chat_y - 4 - chat_log_height;
    let row_top = 111;
    let row_step = ((chat_log_y - 12 - row_top) / shown as i32).clamp(22, 36);
    for (row, columns) in layout.combos.iter_mut().enumerate() {
        let y = row_top + row as i32 * row_step;
        columns[0] = rect(left, y, player_width, 23);
        columns[4] = rect(role_x, y, role_width, 23);
        layout.flags[row] = rect(left - 23, y, 19, 21);
        layout.acceptance[row] = rect(rail_x - 40, y, 18, 21);
        if let Some(pings) = &mut layout.ping_icons {
            pings[row] = rect(rail_x - 60, y, 16, 21);
        }
    }

    let row_bottom = row_top + shown as i32 * row_step;
    layout.roster_navigation = [
        rect(left, row_bottom, 94, 20),
        rect(table_right - 94, row_bottom, 94, 20),
    ];
    layout.roster_page_label = rect(
        left + 102,
        row_bottom,
        (table_right - left - 204).max(80),
        20,
    );
    layout.cooperative_description = rect(left, 65, table_right - left, 22);
    layout.chat_log = rect(
        margin / 2,
        chat_log_y,
        rail_x - margin / 2 - 12,
        chat_log_height,
    );
    let emote_width = if layout.emote.is_some() { 32 } else { 0 };
    layout.chat = rect(
        margin / 2,
        chat_y,
        rail_x - margin / 2 - 12 - emote_width,
        22,
    );
    layout.emote = layout.emote.map(|_| rect(rail_x - 39, chat_y, 27, 24));

    let preview_width = (rail_width - 20).max(80);
    let preview_height = preview_width * 69 / 96;
    layout.map_preview = rect(
        rail_x + (rail_width - preview_width) / 2,
        27,
        preview_width,
        preview_height,
    );
    layout.no_preview = layout.no_preview.map(|_| layout.map_preview);
    // The artwork has a separate panel below the preview frame for labels.
    let label_y = (27 + preview_height + 16).max(160);
    layout.mode_label = rect(rail_x, label_y, rail_width, 18);
    layout.map_label = rect(rail_x, label_y + 20, rail_width, 34);

    // Keep room actions together beneath the map, clear of the chat panel.
    let button_width = (rail_width - 8).max(80);
    let button_count = layout.page.buttons.len() as i32;
    for (index, button) in layout.page.buttons.iter_mut().enumerate() {
        button.rect = rect(
            rail_x + (rail_width - button_width) / 2,
            logical_height - 68 - (button_count - 1 - index as i32) * 38,
            button_width,
            30,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::multiplayer_setup::{SetupDialog, SetupFlavor};

    #[test]
    fn vera_coop_layout_groups_roster_chat_and_actions() {
        let mut layout = MultiplayerSetupLayout::new(
            SetupDialog {
                host: true,
                flavor: SetupFlavor::Lan,
                modern: true,
            },
            800,
            600,
            false,
        );

        apply(&mut layout, 2, 800, 600);

        assert_eq!(layout.labels.len(), 2);
        assert!(layout.combos[0][0].x + layout.combos[0][0].w < layout.combos[0][4].x);
        assert!(layout.chat_log.x + layout.chat_log.w < layout.map_preview.x);
        assert!(layout.chat.x + layout.chat.w < layout.map_preview.x);
    }

    #[test]
    fn vera_coop_native_640x480_layout_keeps_text_inputs_and_buttons_visible() {
        let mut layout = MultiplayerSetupLayout::new(
            SetupDialog {
                host: true,
                flavor: SetupFlavor::Lan,
                modern: true,
            },
            640,
            480,
            false,
        );
        let screen = RectPx::new(0, 0, 640, 480);

        apply(&mut layout, 2, 640, 480);
        assert!((layout.page.scale.factor() - 0.8).abs() < 0.001);

        for rect in [
            layout.page.title,
            layout.labels[0].1,
            layout.labels[1].1,
            layout.combos[0][0],
            layout.combos[1][0],
            layout.chat_log,
            layout.chat,
            layout
                .page
                .button_rect(crate::ui::multiplayer_setup::START)
                .unwrap(),
            layout
                .page
                .button_rect(crate::ui::multiplayer_setup::BACK)
                .unwrap(),
        ] {
            assert!(
                rect.x >= screen.x
                    && rect.y >= screen.y
                    && rect.x + rect.w <= screen.x + screen.w
                    && rect.y + rect.h <= screen.y + screen.h,
                "lobby control outside native 640x480 screen: {rect:?}"
            );
        }
        assert!(layout.chat.w >= 400, "chat entry must remain usable");
        assert!(
            layout
                .page
                .button_rect(crate::ui::multiplayer_setup::START)
                .unwrap()
                .w
                >= 100
        );
    }

    #[test]
    fn vera_coop_wide_layout_uses_extra_width_and_anchors_the_preview_rail() {
        let mut layout = MultiplayerSetupLayout::new(
            SetupDialog {
                host: true,
                flavor: SetupFlavor::Wol,
                modern: true,
            },
            1920,
            1080,
            true,
        );
        let base_player_width = layout.combos[0][0].w;

        apply(&mut layout, 2, 1920, 1080);

        assert!(layout.combos[0][0].w > base_player_width);
        assert!(layout.map_preview.x > layout.combos[0][4].x);
        assert!(layout.map_preview.x + layout.map_preview.w <= 1920);
        assert!(layout.chat.x + layout.chat.w <= 1920);
        let ping = layout.ping_icons.unwrap()[0];
        assert!(layout.combos[0][4].x + layout.combos[0][4].w <= ping.x);
        let start = layout
            .page
            .button_rect(crate::ui::multiplayer_setup::START)
            .unwrap();
        assert!(start.x >= layout.chat.x + layout.chat.w);
        assert!(start.y + start.h <= 1080);
    }

    #[test]
    fn vera_coop_small_responsive_window_keeps_actions_and_chat_in_bounds() {
        let mut layout = MultiplayerSetupLayout::new(
            SetupDialog {
                host: true,
                flavor: SetupFlavor::Lan,
                modern: true,
            },
            480,
            360,
            true,
        );
        let screen = RectPx::new(0, 0, 480, 360);

        apply(&mut layout, 2, 480, 360);

        let start = layout
            .page
            .button_rect(crate::ui::multiplayer_setup::START)
            .unwrap();
        for rect in [layout.chat_log, layout.chat, start, layout.map_preview] {
            assert!(rect.x >= 0 && rect.y >= 0);
            assert!(rect.x + rect.w <= screen.w && rect.y + rect.h <= screen.h);
        }
        assert!(layout.chat.w >= 300);
        assert!(start.w >= 80);
    }
}
