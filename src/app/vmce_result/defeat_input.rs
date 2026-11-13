//! Input for VMCE's own defeat/retry page. Hit-tests the two right-panel
//! controls and hands their intents to the VMCE round.

use crate::app::AppState;
use crate::app::vmce_result::defeat::{self, VmceDefeatAction};
use crate::app::vmce_result::defeat_layout::{LEAVE, RETRY, VmceDefeatLayout};
use winit::event::MouseButton;

pub(crate) fn active(state: &AppState) -> bool {
    defeat::page(state).is_some()
}

fn layout(state: &AppState) -> VmceDefeatLayout {
    let (width, height) = crate::app::frontend::shell_layout::shell_screen_size(state);
    VmceDefeatLayout::new(
        width as i32,
        height as i32,
        crate::app::frontend::shell_layout::fills_window(state),
    )
}

pub(crate) fn cursor_moved(_state: &mut AppState) {}

pub(crate) fn mouse(state: &mut AppState, button: MouseButton, pressed: bool) {
    if button != MouseButton::Left || !active(state) {
        return;
    }
    let cursor = state.match_state.input.cursor_px();
    let layout = layout(state);
    let retry = layout.button_rect(RETRY);
    let leave = layout.button_rect(LEAVE);
    let inside = |rect: Option<crate::ui::shell::geom::RectPx>| {
        rect.is_some_and(|rect| {
            cursor.0 >= rect.x
                && cursor.1 >= rect.y
                && cursor.0 < rect.x + rect.w
                && cursor.1 < rect.y + rect.h
        })
    };
    let retry_state = defeat::retry_state(state);
    let hit = if inside(retry) && retry_state.2 {
        Some((RETRY, VmceDefeatAction::Retry))
    } else if inside(leave) && defeat::leave_enabled(state) {
        Some((LEAVE, VmceDefeatAction::Leave))
    } else {
        None
    };
    if pressed {
        if let Some(page) = match state.frontend.vmce_result_page.as_mut() {
            Some(crate::app::vmce_result::VmceResultPage::Defeat(page)) => Some(page),
            _ => None,
        } {
            page.pressed = hit.map(|(id, _)| id);
        }
        state.platform.window.request_redraw();
        return;
    }
    let pressed_id = match state.frontend.vmce_result_page.as_mut() {
        Some(crate::app::vmce_result::VmceResultPage::Defeat(page)) => page.pressed.take(),
        _ => None,
    };
    if let Some(action) = hit
        .filter(|(id, _)| pressed_id == Some(*id))
        .map(|(_, action)| action)
    {
        defeat::apply_action(state, action);
    } else {
        state.platform.window.request_redraw();
    }
}

pub(crate) fn key(state: &mut AppState, event: &winit::event::KeyEvent) {
    if !event.state.is_pressed() || !active(state) {
        return;
    }
    if matches!(
        event.physical_key,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape)
    ) {
        defeat::apply_action(state, VmceDefeatAction::Leave);
    }
}

pub(crate) fn poll(
    _state: &mut AppState,
) -> Option<crate::ui::main_menu_dialogs::options::LauncherCue> {
    None
}
