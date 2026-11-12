//! BC6 (`0xBC6`) window input adapter. The dialog produces intents;
//! `app::cooperative_round` and `MatchSession` decide their effects.

use crate::app::cooperative_round::CoopScoreView;
use crate::app::frontend::{score_overlay, shell_layout, shell_transition};
use crate::app::{App, AppState};
use crate::ui::coop_score_shell::*;
use crate::ui::lan_shell::LanEditKey;
use crate::ui::main_menu_dialogs::options::LauncherCue;
use crate::util::time::Instant;
use winit::event::{KeyEvent, MouseButton};
use winit::keyboard::{KeyCode, PhysicalKey};

pub(crate) fn layout(state: &AppState) -> CoopScoreLayout {
    let (w, h) = shell_layout::shell_screen_size(state);
    // The open page's extension owns its row count and extra cell ids; the
    // native page keeps the two-row table. A stock BC6 page lists at most the
    // two native rows (`0x0046EE17`'s table has two), so a longer model
    // without an extension is a projection bug, not extra painted rows.
    let (rows, extended): (usize, &[[u16; 5]]) = match score_overlay::coop_row_extension(state) {
        Some((rows, cells)) => (rows, cells),
        None => {
            let model_rows = state
                .frontend
                .coop_score_page
                .as_ref()
                .map_or(DEFAULT_SCORE_ROWS, |page| page.model.rows.len());
            if model_rows > DEFAULT_SCORE_ROWS {
                log::warn!(
                    "Stock BC6 model lists {model_rows} rows; the native page lays out \
                     {DEFAULT_SCORE_ROWS} and takes extra rows only from a page extension"
                );
            }
            (DEFAULT_SCORE_ROWS, &[])
        }
    };
    CoopScoreLayout::new_rows(
        w as i32,
        h as i32,
        shell_layout::fills_window(state),
        rows,
        extended,
    )
}

/// BC6 owns the result screen while its page is open.
pub(crate) fn active(state: &AppState) -> bool {
    matches!(
        state.frontend.screen,
        crate::app::GameScreen::MissionResult { .. }
    ) && state.frontend.coop_score_page.is_some()
}

pub(crate) fn update_geometry(state: &mut AppState) {
    if score_overlay::coop_is_statistics_only(state) {
        return;
    }
    let layout = layout(state);
    let (Some(chat_log), Some(chat)) = (layout.control_rect(CHAT_LOG), layout.control_rect(CHAT))
    else {
        return;
    };
    let Some(page) = state.frontend.coop_score_page.as_mut() else {
        return;
    };
    let ui = &mut page.ui;
    ui.chat
        .list
        .reconcile_geometry(chat_log, ui.chat.lines.len(), layout.page.scale);
    ui.chat.edit.scroll_to_caret(
        (layout.page.scale.base_rect(chat).w - 3).max(0) as u32,
        |text| state.renderer.bit_font.text_width(text),
    );
}

/// Whether control `id` takes input now (`0x0046FB50`, `0x0046DE20`). The
/// open page's extension supplies the enablement of its own buttons; `None`
/// keeps the native rules.
fn enabled(state: &AppState, view: &CoopScoreView, can_continue: bool, id: u16) -> bool {
    if let Some(extended) = score_overlay::coop_control_enabled(state, view, id) {
        return extended;
    }
    if !can_continue && hidden_on_final_mission(id) {
        return false;
    }
    match id {
        CONTINUE => view.continue_enabled,
        QUIT => view.quit_enabled,
        CHAT | CHAT_LOG => true,
        _ => false,
    }
}

pub(crate) fn key(state: &mut AppState, event: &KeyEvent) {
    if !event.state.is_pressed() || shell_transition::blocks_shell_input(state) {
        return;
    }
    let code = if let PhysicalKey::Code(code) = event.physical_key {
        Some(code)
    } else {
        None
    };
    key_press(state, code, event.text.as_deref());
}

pub(crate) fn key_press(state: &mut AppState, code: Option<KeyCode>, text: Option<&str>) {
    if score_overlay::coop_is_statistics_only(state) {
        // The statistics page is the whole result view: no chat input.
        return;
    }
    let Some(page) = state.frontend.coop_score_page.as_mut() else {
        return;
    };
    // The chat edit has the focus and takes every key; Enter submits.
    let key = match code {
        Some(KeyCode::Backspace) => Some(LanEditKey::Backspace),
        Some(KeyCode::Delete) => Some(LanEditKey::Delete),
        Some(KeyCode::ArrowLeft) => Some(LanEditKey::Left),
        Some(KeyCode::ArrowRight) => Some(LanEditKey::Right),
        Some(KeyCode::Home) => Some(LanEditKey::Home),
        Some(KeyCode::End) => Some(LanEditKey::End),
        Some(KeyCode::Enter | KeyCode::NumpadEnter) => Some(LanEditKey::Enter),
        _ => None,
    };
    let action = if let Some(key) = key {
        page.ui.chat.key(key).map(CoopScoreAction::Chat)
    } else {
        if !matches!(code, Some(KeyCode::Escape | KeyCode::Tab))
            && let Some(text) = text
        {
            page.ui.chat.insert_text(text);
        }
        None
    };
    if let Some(action) = action {
        crate::app::cooperative_round::apply_action(state, action);
    }
    state.platform.window.request_redraw();
}

pub(crate) fn cursor_moved(state: &mut AppState) {
    let layout = layout(state);
    let (x, y) = state.match_state.input.cursor_px();
    let statistics_only = score_overlay::coop_is_statistics_only(state);
    let Some(page) = state.frontend.coop_score_page.as_mut() else {
        return;
    };
    page.ui.buttons.hovered = layout.control_at(x, y);
    if statistics_only {
        state.platform.window.request_redraw();
        return;
    }
    let Some(chat_log) = layout.control_rect(CHAT_LOG) else {
        return;
    };
    let ui = &mut page.ui;
    let geometry = ui
        .chat
        .list
        .geometry(chat_log, ui.chat.lines.len(), layout.page.scale);
    ui.chat
        .list
        .scroll
        .pointer_moved(geometry, &mut ui.chat.list.top_index, x, y);
    state.platform.window.request_redraw();
}

pub(crate) fn mouse(state: &mut AppState, button: MouseButton, pressed: bool) {
    if button != MouseButton::Left || shell_transition::blocks_shell_input(state) {
        return;
    }
    let Some(view) = crate::app::cooperative_round::view(state) else {
        return;
    };
    let layout = layout(state);
    let (x, y) = state.match_state.input.cursor_px();
    let can_continue = match state.frontend.coop_score_page.as_ref() {
        Some(page) => page.model.can_continue,
        None => return,
    };
    let over = layout
        .control_at(x, y)
        .filter(|id| enabled(state, &view, can_continue, *id));
    let Some(page) = state.frontend.coop_score_page.as_mut() else {
        return;
    };
    let ui = &mut page.ui;
    let mut action = None;
    let mut cue = None;
    if pressed {
        match over {
            Some(CHAT) => ui.chat.edit.focused = true,
            Some(CHAT_LOG) => {
                let rect = layout.control_rect(CHAT_LOG).expect("chat list rect");
                let geometry = ui
                    .chat
                    .list
                    .geometry(rect, ui.chat.lines.len(), layout.page.scale);
                if let Some(part) = geometry.scroll_part_at(x, y) {
                    ui.chat.list.scroll.press(
                        part,
                        geometry,
                        &mut ui.chat.list.top_index,
                        y,
                        Instant::now(),
                    );
                }
            }
            Some(control @ (CONTINUE | QUIT)) => {
                ui.buttons.press(Some(control));
                cue = Some(LauncherCue::MainButton);
            }
            _ => {}
        }
    } else {
        ui.chat.list.scroll.cancel();
        if let Some(control) = ui.buttons.release(over) {
            action = match control {
                CONTINUE => Some(CoopScoreAction::Continue),
                QUIT => Some(CoopScoreAction::Quit),
                _ => None,
            };
        }
    }
    if let Some(cue) = cue {
        App::play_launcher_options_cue(state, cue);
    }
    if let Some(action) = action {
        crate::app::cooperative_round::apply_action(state, action);
    }
    state.platform.window.request_redraw();
}

/// The chat list's scrollbar repeat timer.
pub(crate) fn poll(state: &mut AppState) -> Option<Instant> {
    if score_overlay::coop_is_statistics_only(state) {
        return None;
    }
    let layout = layout(state);
    let (x, y) = state.match_state.input.cursor_px();
    let rect = layout.control_rect(CHAT_LOG)?;
    let page = state.frontend.coop_score_page.as_mut()?;
    let ui = &mut page.ui;
    let geometry = ui
        .chat
        .list
        .geometry(rect, ui.chat.lines.len(), layout.page.scale);
    if ui
        .chat
        .list
        .scroll
        .poll(geometry, &mut ui.chat.list.top_index, x, y, Instant::now())
    {
        state.platform.window.request_redraw();
    }
    ui.chat.list.scroll.repeat_at()
}
