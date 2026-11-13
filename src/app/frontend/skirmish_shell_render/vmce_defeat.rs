//! VMCE's own `0xBC6` defeat/retry page painter.
//!
//! The page owns its background, title, statistics table and Retry/Leave
//! controls. It reads only VMCE result state; the stock Cooperative page model
//! and renderer are not involved.

use super::chrome::*;
use super::in_game_shell::{
    self, InGameShellFrame, ShellFrameArt, ShellFrameCursor, ShellFrameOverlay,
};
use super::text::{button_text_rect, localized_label, rect_to_text_rect};
use super::{
    SHELL_CONTROL_DEPTH, SHELL_CONTROL_TEXT_DEPTH, SHELL_LABEL_TEXT_RGB,
    SHELL_PARENT_BACKGROUND_DEPTH,
};
use crate::app::AppState;
use crate::app::vmce_result::defeat_layout::{LEAVE, RETRY, VmceDefeatLayout};
use crate::app::vmce_result::{defeat, types};
use crate::render::shell_text::{self, ShellAlign};
use crate::ui::shell::button::ButtonVisual;
use crate::ui::shell::geom::{self, RectPx};

const LIT_BUTTON_CAPTION_RGB: [f32; 3] = [5.0 / 255.0, 12.0 / 255.0, 0.0];

pub(crate) fn render_vmce_defeat(
    state: &mut AppState,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Texture,
) -> anyhow::Result<bool> {
    if state.frontend.skirmish_shell_chrome.is_none() {
        return Ok(false);
    }
    let Some(page) = defeat::page(state) else {
        return Ok(false);
    };
    let (width, height) = (state.render_width(), state.render_height());
    // Input hit-tests the same logical shell layout; using render dimensions
    // here would offset every control when the upscale pass is active.
    let (shell_width, shell_height) = crate::app::frontend::shell_layout::shell_screen_size(state);
    let fills = crate::app::frontend::shell_layout::fills_window(state);
    let layout = VmceDefeatLayout::new(shell_width as i32, shell_height as i32, fills);
    let scale = layout.scale();
    let chrome_layout = crate::ui::skirmish_shell::compute_layout_on(scale, width, height);
    let atlas = state
        .frontend
        .skirmish_shell_chrome
        .as_ref()
        .expect("checked above");
    let font = &state.renderer.bit_font;
    let rgb = |rgb: [u8; 3]| rgb.map(|channel| f32::from(channel) / 255.0);
    let text = |value: &str, rect: RectPx, align: ShellAlign, rgb: [f32; 3]| {
        shell_text::draw_in_rect_scaled(
            font,
            value,
            rect_to_text_rect(rect),
            rgb,
            align,
            [0.0; 2],
            SHELL_CONTROL_TEXT_DEPTH,
            scale.factor(),
        )
    };

    // Dialog background: right panel, lower strip and the native frame.
    let mut art = Vec::new();
    push_right_panel_base_instances(&mut art, atlas, &chrome_layout, false);
    push_lower_strip_instance(&mut art, atlas, &chrome_layout);
    push_steady_optional_chrome_instances(
        &mut art,
        atlas,
        &chrome_layout,
        super::draw_order::ShellDialogChromeProfile::SkirmishSetup0x102,
    );
    let score_backdrop = state.frontend.campaign_score_art.as_ref().map(|scores| {
        let (aw, ah) = if fills {
            (800, 600)
        } else {
            (width as i32, height as i32)
        };
        let (x, y) = geom::dialog_origin(aw, ah);
        let small = chrome_layout.authored_width() == 640;
        let rect = scale.scale_rect(RectPx::new(
            x,
            y,
            if small { 472 } else { 632 },
            if small { 448 } else { 568 },
        ));
        let mut sprites = Vec::with_capacity(1);
        crate::render::shell_paint::push_entry_sized(
            &mut sprites,
            scores.background,
            rect.x as f32,
            rect.y as f32,
            [rect.w as f32, rect.h as f32],
            SHELL_PARENT_BACKGROUND_DEPTH,
        );
        sprites[0]
    });

    let mut texts = Vec::new();
    texts.push(text(
        &localized_label(state, "GUI:CampaignResults", "Campaign Results"),
        layout.page.title,
        ShellAlign::PANEL_TITLE,
        SHELL_LABEL_TEXT_RGB,
    ));

    // Statistics table: headers, then up to four role rows.
    let table = types::statistics_layout(scale);
    for header in &table.headers {
        texts.push(text(
            &localized_label(state, header.key, header.key),
            header.rect,
            if header.centred {
                ShellAlign::H_CENTER
            } else {
                ShellAlign::NONE
            },
            SHELL_LABEL_TEXT_RGB,
        ));
    }
    let ellipsize = |value: &str, max_w: i32| {
        crate::app::frontend::skirmish_shell_render::vmce_panel::ellipsize_native(
            &state.renderer.bit_font,
            value,
            max_w,
        )
    };
    for (row_index, row) in page.statistics.rows.iter().take(4).enumerate() {
        let cells = table.rows[row_index].cells;
        let player_cell = cells[0];
        let name_width = player_cell.w * 2 / 3;
        let name_rect = RectPx::new(player_cell.x, player_cell.y, name_width, player_cell.h);
        let result_rect = RectPx::new(
            player_cell.x + name_width,
            player_cell.y,
            player_cell.w - name_width,
            player_cell.h,
        );
        let row_rgb = rgb(row.rgb.unwrap_or([255, 255, 0]));
        texts.push(text(
            &ellipsize(&row.player, scale.base_rect(name_rect).w),
            name_rect,
            ShellAlign::NONE,
            row_rgb,
        ));
        texts.push(text(
            &localized_label(state, "VERA:VmceResultLost", "Lost"),
            result_rect,
            ShellAlign::H_CENTER,
            row_rgb,
        ));
        let values: [String; 5] = [
            row.kills.to_string(),
            row.losses.to_string(),
            row.built.to_string(),
            row.score.to_string(),
            row.income.to_string(),
        ];
        for (column, value) in values.into_iter().enumerate() {
            texts.push(text(
                &value,
                cells[column + 1],
                ShellAlign::H_CENTER,
                row_rgb,
            ));
        }
    }
    texts.push(text(
        &format!(
            "{} {}",
            localized_label(state, "GUI:YourTime", "Your Time"),
            page.statistics.total_time
        ),
        table.total_time,
        ShellAlign::NONE,
        SHELL_LABEL_TEXT_RGB,
    ));
    texts.push(text(
        &format!(
            "{} {}",
            localized_label(state, "GUI:ParTime", "Par Time"),
            page.statistics.par_time
        ),
        table.par_time,
        ShellAlign::NONE,
        SHELL_LABEL_TEXT_RGB,
    ));

    // Retry / Leave Room occupy the native right-panel slots.
    let (_, retry_lit, retry_enabled) = defeat::retry_state(state);
    let retry_enabled = retry_enabled && defeat::page(state).is_some();
    let leave_enabled = defeat::leave_enabled(state);
    for (id, label_key, lit, enabled) in [
        (RETRY, "VERA:VmceRetry", retry_lit, retry_enabled),
        (LEAVE, "VERA:VmceLeaveRoom", false, leave_enabled),
    ] {
        let Some(rect) = layout.button_rect(id) else {
            continue;
        };
        push_right_panel_button_shp(
            &mut art,
            atlas,
            rect,
            ButtonVisual {
                pressed: lit || page.pressed == Some(id),
                highlighted: false,
            },
            !enabled && !lit,
            SHELL_CONTROL_DEPTH,
        );
        let caption_rgb = if lit {
            LIT_BUTTON_CAPTION_RGB
        } else {
            super::text::button_label_color_for_disabled(!enabled)
        };
        texts.push(shell_text::draw_in_rect_scaled(
            font,
            &localized_label(state, label_key, label_key),
            button_text_rect(rect, false, scale),
            caption_rgb,
            ShellAlign::BUTTON_CAPTION,
            [0.0; 2],
            SHELL_CONTROL_TEXT_DEPTH,
            scale.factor(),
        ));
    }

    in_game_shell::render_shell_frame(
        state,
        encoder,
        destination,
        InGameShellFrame {
            art,
            controls: Vec::new(),
            texts,
            label: "VMCE Defeat Result 0xBC6",
        },
        ShellFrameArt::Launcher,
        ShellFrameOverlay {
            score_backdrop,
            ..Default::default()
        },
        ShellFrameCursor::Visible,
    )?;
    state.platform.window.request_redraw();
    Ok(true)
}
