//! Paint the LAN co-op result dialog `0xBC6`. `UI__RegisterWindow`
//! (`0x00622820`) gives it BC's D5/D6 chrome (SDTP frame 1 and SDMPBTN)
//! and no RadarBackground (D7); `0x0060CF00` gives it `SCRBK` for its
//! background. The children are the setup page's native widgets.
//! RESIDUAL: the statics paint at once, without their kind-1 reveal, and
//! the page has no entry or exit slide.

use super::chrome::*;
use super::controls::{
    ControlPaint, NewEditPaint, house_color_tint, list_controls, paint_control,
    push_new_edit_instances,
};
use super::in_game_shell::{
    self, InGameShellFrame, ShellFrameArt, ShellFrameCursor, ShellFrameOverlay,
};
use super::preview::{build_preview_surface_instance, ensure_map_preview_texture};
use super::text::{button_text_rect, localized_label, rect_to_text_rect};
use super::{
    SHELL_CONTROL_DEPTH, SHELL_CONTROL_TEXT_DEPTH, SHELL_EDIT_CARET_DEPTH, SHELL_EDIT_FRAME_DEPTH,
    SHELL_LABEL_TEXT_RGB, SHELL_PARENT_BACKGROUND_DEPTH,
};
use crate::app::AppState;
use crate::app::frontend::score_overlay;
use crate::render::shell_text::{self, ShellAlign};
use crate::ui::coop_score_shell::*;
use crate::ui::shell::geom::{self, RectPx};
use crate::util::time::Instant;

/// The native owner-draw caption colour of a lit button (`SDBTNANM` frame 4,
/// packed `0x00000C05`): a lit frame's label is dark, even while disabled.
const LIT_BUTTON_CAPTION_RGB: [f32; 3] = [5.0 / 255.0, 12.0 / 255.0, 0.0];

pub(crate) fn render_coop_score(
    state: &mut AppState,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Texture,
) -> anyhow::Result<bool> {
    if state.frontend.skirmish_shell_chrome.is_none() {
        return Ok(false);
    }
    let Some(view) = crate::app::cooperative_round::view(state) else {
        return Ok(false);
    };
    crate::app::input::coop_score::update_geometry(state);
    let layout = crate::app::input::coop_score::layout(state);
    // One overlay projection per frame: paint and input consume the same
    // extension data, so the drawn and clickable buttons cannot drift.
    let overlay = score_overlay::coop_overlay(state, &layout, &view);
    let suppress_stock_text = overlay
        .as_ref()
        .is_some_and(|overlay| overlay.suppress_stock_text);
    let suppress_body_controls = overlay
        .as_ref()
        .is_some_and(|overlay| overlay.suppress_body_controls);
    let hidden_control = |id: u16| {
        overlay
            .as_ref()
            .is_some_and(|overlay| overlay.hidden_controls.contains(&id))
    };
    let scale = layout.page.scale;
    let (width, height) = (state.render_width(), state.render_height());
    let fills = crate::app::frontend::shell_layout::fills_window(state);
    let now = Instant::now();
    let (can_continue, map_index) = {
        let model = &state.frontend.coop_score_page.as_ref().expect("view").model;
        (model.can_continue, model.map_index)
    };
    match map_index.filter(|_| can_continue && !suppress_body_controls) {
        Some(index) => ensure_map_preview_texture(
            state,
            index,
            layout
                .control_rect(PREVIEW)
                .unwrap_or(crate::ui::skirmish_shell::RectPx::new(0, 0, 0, 0)),
        ),
        None => state.frontend.skirmish_preview_texture = None,
    }
    let shown = |id: u16| can_continue || !hidden_on_final_mission(id);
    let controller_hover = state
        .frontend
        .coop_score_page
        .as_ref()
        .and_then(|page| page.ui.buttons.hovered);
    let status = controller_hover
        .and_then(help_key)
        .map(|key| localized_label(state, key, ""))
        .unwrap_or_default();
    let title = localized_label(state, COOP_SCORE_PAGE.title_key, COOP_SCORE_PAGE.title_key);
    let page = state.frontend.coop_score_page.as_ref().expect("view");
    let model = &page.model;
    let ui = &page.ui;
    let atlas = state
        .frontend
        .skirmish_shell_chrome
        .as_ref()
        .expect("checked above");
    let font = &state.renderer.bit_font;
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
    let rgb = |rgb: [u8; 3]| rgb.map(|channel| f32::from(channel) / 255.0);
    let align = |align: CoopScoreAlign| match align {
        CoopScoreAlign::Left => ShellAlign::NONE,
        CoopScoreAlign::Center => ShellAlign::H_CENTER,
        CoopScoreAlign::Right => ShellAlign::H_RIGHT,
    };

    // Paint mode 1: RightPanel__Draw, the dialog background, then D5/D6.
    let chrome_layout = crate::ui::skirmish_shell::compute_layout_on(scale, width, height);
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

    let mut controls = Vec::new();
    let mut texts = Vec::new();
    texts.push(text(
        &title,
        layout.page.title,
        ShellAlign::PANEL_TITLE,
        SHELL_LABEL_TEXT_RGB,
    ));
    texts.push(text(
        &status,
        layout.page.status_help,
        ShellAlign::V_CENTER,
        SHELL_LABEL_TEXT_RGB,
    ));
    // The extension takes over the page body: no native statics, cells or
    // right-panel map texts.
    if !suppress_stock_text {
        for spec in COOP_SCORE_CONTROLS
            .iter()
            .filter(|spec| shown(spec.id) && !hidden_control(spec.id))
        {
            let Some(rect) = layout.control_rect(spec.id) else {
                continue;
            };
            let value = match spec.id {
                TIME => model.time.clone(),
                NEXT_MISSION => model.next_mission.clone(),
                DESCRIPTION => model.description.clone(),
                id => match spec.caption_key {
                    Some(key) => localized_label(state, key, key),
                    None => {
                        let _ = id;
                        continue;
                    }
                },
            };
            texts.push(text(&value, rect, align(spec.align), SHELL_LABEL_TEXT_RGB));
        }
        for (label, id) in [
            (&model.mode_label, MODE_LABEL),
            (&model.map_label, MAP_LABEL),
        ] {
            if shown(id)
                && let Some(rect) = layout.control_rect(id)
            {
                texts.push(text(
                    label,
                    rect,
                    ShellAlign::H_CENTER,
                    SHELL_LABEL_TEXT_RGB,
                ));
            }
        }
        // The tail's cells: name, then "%d" for each count, in the row colour.
        for (index, row) in model.rows.iter().enumerate() {
            let Some(cells) = layout.row_cells(index) else {
                continue;
            };
            let values = [
                row.name.clone(),
                row.kills.to_string(),
                row.losses.to_string(),
                row.built.to_string(),
                row.score.to_string(),
            ];
            for (column, (value, id)) in values.iter().zip(cells).enumerate() {
                let row_align = match column {
                    0 => CoopScoreAlign::Left,
                    _ => CoopScoreAlign::Right,
                };
                if let Some(rect) = layout.control_rect(id) {
                    texts.push(text(value, rect, align(row_align), rgb(row.rgb)));
                }
            }
        }
    } else if let Some(overlay) = overlay.as_ref() {
        // The extension's body text replaces the native table; its lines are
        // appended at this depth in the projection's authored order.
        for line in &overlay.texts {
            texts.push(text(&line.text, line.rect, line.align, line.rgb));
        }
    }
    let chrome = atlas.control_chrome();
    if !suppress_body_controls {
        for (row, slot) in view.slots.iter().enumerate() {
            let Some(slot) = slot else {
                continue;
            };
            // 0x0046FA20: wolhost.pcx on slot 0 and wolacpt.pcx on slot 1 while
            // ready, on human slots only.
            if let Some(icon) =
                atlas.lan_player_status(row == 0 && slot.ready, row == 1 && slot.ready)
                && let Some(rect) = layout.control_rect(READY_ICONS[row])
            {
                push_flag_entry_clipped_centered(
                    &mut controls,
                    icon,
                    rect,
                    scale,
                    SHELL_CONTROL_DEPTH,
                );
            }
            if !can_continue {
                continue;
            }
            if let Some(flag) = slot
                .flag
                .as_deref()
                .and_then(|name| atlas.lan_player_side(name))
                && let Some(rect) = layout.control_rect(FLAGS[row])
            {
                push_flag_entry_clipped_centered(
                    &mut controls,
                    flag,
                    rect,
                    scale,
                    SHELL_CONTROL_DEPTH,
                );
            }
            let name = crate::ui::multiplayer_setup::SetupChoice {
                value: 0,
                text: slot.name.clone(),
                swatch: None,
            };
            // 0x0046DCC0 disables the other human's combos; the name combo has
            // no handler and stays disabled for both.
            for (id, item, enabled) in [
                (NAMES[row], &name, false),
                (COUNTRIES[row], &slot.country, slot.enabled),
                (COLORS[row], &slot.color, slot.enabled),
                (STARTS[row], &slot.start, slot.enabled),
            ] {
                let Some(rect) = layout.control_rect(id) else {
                    continue;
                };
                paint_control(
                    &mut controls,
                    &chrome,
                    ControlPaint::Combo {
                        rect,
                        swatch: item.swatch.map(|index| house_color_tint(&[], index)),
                        open: false,
                        disabled: !enabled,
                    },
                    scale,
                );
                if item.swatch.is_none() {
                    texts.push(text(
                        &item.text,
                        crate::ui::skirmish_shell::combo_text_rect(rect, scale),
                        ShellAlign::V_CENTER,
                        super::text::combo_face_text_color(!enabled),
                    ));
                }
            }
        }
    }
    if can_continue
        && !suppress_body_controls
        && let Some(rect) = layout.control_rect(DIFFICULTY)
    {
        // 0x0046DE20's slider: range 0..2 at [0xA8EB68], no value plaque.
        let bar = crate::ui::multiplayer_setup::SetupTrackbar {
            id: crate::ui::multiplayer_setup::DIFFICULTY,
            value: view.difficulty,
            range: DIFFICULTY_RANGE,
            enabled: view.host,
            visible: true,
        };
        paint_control(
            &mut controls,
            &chrome,
            ControlPaint::Trackbar {
                rect,
                thumb_left: bar.thumb_left(rect, scale),
                plaque: false,
                // EnableWindow(0x732) at `0x0046DDF3..0x0046DE10`.
                disabled: !bar.enabled,
            },
            scale,
        );
    }
    if !suppress_body_controls {
        let (Some(chat_rect), Some(chat_log_rect)) =
            (layout.control_rect(CHAT), layout.control_rect(CHAT_LOG))
        else {
            return Ok(false);
        };
        // The NewEdit paints inside its one-pixel resource frame.
        let chat_rect = scale.derive_rect(chat_rect, |rect| {
            RectPx::new(rect.x + 1, rect.y + 1, rect.w - 2, rect.h - 2)
        });
        let edit = &ui.chat.edit;
        let field = scale.derive_rect(chat_rect, |rect| {
            RectPx::new(rect.x + 2, rect.y, rect.w - 1, rect.h + 1)
        });
        push_new_edit_instances(
            &mut controls,
            atlas,
            font,
            edit,
            NewEditPaint {
                frame: scale.derive_rect(chat_rect, |rect| {
                    RectPx::new(rect.x - 2, rect.y - 2, rect.w + 5, rect.h + 5)
                }),
                text: field,
                background: None,
                frame_depth: SHELL_EDIT_FRAME_DEPTH,
                caret_depth: SHELL_EDIT_CARET_DEPTH,
                caret_visible: edit.focused && (now.elapsed().as_millis() / 500) % 2 == 0,
            },
            scale,
        );
        texts.push(text(
            &String::from_utf16_lossy(&edit.units[edit.first_visible_unit.min(edit.units.len())..]),
            field,
            ShellAlign::V_CENTER,
            SHELL_LABEL_TEXT_RGB,
        ));
        let chat = ui
            .chat
            .list
            .geometry(chat_log_rect, ui.chat.lines.len(), scale);
        list_controls(&mut controls, atlas, chat, &ui.chat.list, false, scale);
        for (visible, line) in ui
            .chat
            .lines
            .iter()
            .skip(ui.chat.list.top_index)
            .take(chat.visible_rows)
            .enumerate()
        {
            let color = if line.color == u32::MAX {
                SHELL_LABEL_TEXT_RGB
            } else {
                rgb([
                    line.color as u8,
                    (line.color >> 8) as u8,
                    (line.color >> 16) as u8,
                ])
            };
            texts.push(text(
                &String::from_utf16_lossy(&line.text),
                scale.derive_rect(chat.row(visible), |rect| {
                    RectPx::new(rect.x + 2, rect.y, rect.w - 4, rect.h)
                }),
                ShellAlign::V_CENTER,
                color,
            ));
        }
    }
    let mut frame_overlay = ShellFrameOverlay {
        score_backdrop,
        ..Default::default()
    };
    if let (Some(preview), Some(rect)) = (
        state.frontend.skirmish_preview_texture.as_ref(),
        layout.control_rect(PREVIEW),
    ) {
        frame_overlay.preview = build_preview_surface_instance(rect, preview.width, preview.height);
    }
    for button in &layout.page.buttons {
        if !shown(button.id) || hidden_control(button.id) {
            continue;
        }
        // A page extension owns its button set: its projection supplies the
        // caption key and the lit/enabled state; native art and the disabled
        // style stay the shared renderer's.
        let (label_key, lit, enabled) = match overlay.as_ref().and_then(|overlay| {
            overlay
                .buttons
                .iter()
                .find(|candidate| candidate.id == button.id)
        }) {
            Some(projected) => (projected.label_key, projected.lit, projected.enabled),
            None if overlay.is_some() => continue,
            None => {
                let enabled = match button.id {
                    CONTINUE => view.continue_enabled,
                    _ => view.quit_enabled,
                };
                let key = COOP_SCORE_PAGE
                    .buttons()
                    .find(|spec| spec.id == button.id)
                    .map(|spec| spec.csf_key)
                    .unwrap_or("GUI:Continue");
                (key, false, enabled)
            }
        };
        let pressed = lit || ui.buttons.is_pressed(button.id);
        push_right_panel_button_shp(
            &mut art,
            atlas,
            button.rect,
            crate::ui::shell::button::ButtonVisual {
                pressed,
                highlighted: false,
            },
            // The lit frame must stay frame 4 even when the button is no
            // longer clickable (a guest's cast vote); input removes it.
            !enabled && !lit,
            SHELL_CONTROL_DEPTH,
        );
        let rgb = if lit {
            LIT_BUTTON_CAPTION_RGB
        } else {
            super::text::button_label_color_for_disabled(!enabled)
        };
        texts.push(shell_text::draw_in_rect_scaled(
            font,
            &localized_label(state, label_key, label_key),
            // Lit and disabled captions share the unpressed, centred frame.
            button_text_rect(button.rect, false, scale),
            rgb,
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
            controls,
            texts,
            label: "LAN Co-op Result BC6",
        },
        ShellFrameArt::Launcher,
        frame_overlay,
        ShellFrameCursor::Visible,
    )?;
    state.platform.window.request_redraw();
    Ok(true)
}
