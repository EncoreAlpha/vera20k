//! Render glue for campaign victory dialog `0x103` and the LAN Cooperative
//! campaign end `0xBC7`.
//!
//! Native `GameExit::Victory` opens `0x103` through `0x6C8E80`, and
//! Cooperative `+0xA4` opens `0xBC7` through `0x00471420`; this renderer
//! consumes the open dialog's retail controls and texts from
//! `ui::campaign_score_shell`. It deliberately does not borrow the Skirmish
//! score table (`0x108`) or its side-art owner.

use crate::util::time::Instant;

use anyhow::Result;

use crate::app::AppState;
use crate::app::frontend::shell_pass::{
    ShellComposition, TexturedDraw, encode_shell_pass, owner_draw_button_label_rect, resolve_csf,
    software_cursor,
};
use crate::render::shell_paint::{
    self, CURSOR_DEPTH, DIALOG_BACKGROUND_DEPTH, PARENT_BACKGROUND_DEPTH, PaintButton, PaintLabel,
    SHELL_TEXT_RGB_ENABLED,
};
use crate::render::shell_text::ShellAlign;
use crate::render::shell_text_reveal::PathAReveal;
use crate::ui::campaign_score_shell::{CampaignScoreDialog, SCORE_ART_REGION_STATIC};

const SHELL_TEXT: [u8; 3] = [255, 255, 0];
const HIGHLIGHT: [u8; 3] = [255, 255, 255];

/// Paint campaign dialog `0x103` or `0xBC7`. It needs the shared shell chrome,
/// but no Skirmish score art: each template has its own static controls.
pub(crate) fn render_campaign_score_page(
    state: &mut AppState,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Texture,
) -> Result<bool> {
    if state.frontend.main_menu_shell_chrome.is_none()
        || state.frontend.campaign_score_page.is_none()
    {
        return Ok(false);
    }

    let (width, height) = crate::app::frontend::shell_layout::shell_screen_size(state);
    let fills_window = crate::app::frontend::shell_layout::fills_window(state);
    let now = Instant::now();
    // `0x006071E0` owns both slides; `SetSlideChildSuppressProc` hides the
    // dialog's statics (including the SCRA kind-4 static) until SHOW completes,
    // and the teardown repaint covers them while the column slides out.
    let dialog = state
        .frontend
        .campaign_score_page
        .as_ref()
        .expect("checked before render")
        .dialog;
    let exit_wave = crate::app::frontend::shell_transition::shell_exit_wave(
        state,
        crate::app::frontend::shell_transition::ShellSlideKind::campaign_score(dialog),
    )
    .cloned();
    let leaving = exit_wave.is_some();
    let wave = exit_wave.or_else(|| state.frontend.shell_first_paint_slide.clone());
    let children = wave.is_none();
    // `0x006071E0` composites SCRT itself during a slide: frames 5..0 on SHOW,
    // 0..5 on CLOSE, then SCRBK at the dialog origin. Steady WM_PAINT draws
    // RadarBackground's frame 0.
    let scrt_frame = match wave.as_ref() {
        None => Some(0),
        Some(wave) => wave.panel_art().iter().find_map(|art| match art {
            crate::ui::shell::slide::PanelArt::Scrt(frame) => Some(*frame),
            _ => None,
        }),
    };
    let animation_frames = state
        .frontend
        .campaign_score_art
        .as_ref()
        .and_then(|art| art.scra.as_ref())
        .map_or(0, |strip| strip.frames.len());
    let (layout, emblem_frame) = {
        let page = state
            .frontend
            .campaign_score_page
            .as_mut()
            .expect("checked before render");
        // Production always arms a SHOW wave; if one cannot be created (or a
        // future route reuses the page without a new target edge), show the
        // page instead of leaving every static hidden forever.
        if wave.is_none() && !leaving && !page.presentation_started() {
            page.start_presentation(animation_frames, now);
        }
        let frame = if leaving {
            None
        } else if wave.is_some() {
            page.emblem_animation_frame()
        } else {
            page.poll_emblem_animation(now)
        };
        (page.layout(width, height, fills_window), frame)
    };
    // `0xBC7`'s monitor `0x71C` runs the shared kind-4 timer.
    let monitor_frame = (children && dialog == CampaignScoreDialog::CampaignEnd)
        .then(|| crate::app::frontend::menu_page_render::paint_shell_monitor(state))
        .flatten();
    let continue_id = dialog.continue_button();
    let controller = &state.frontend.shell_controller;
    let active = controller.top_id() == Some(dialog.id());
    let hovered = children
        .then(|| active.then(|| controller.hovered()).flatten())
        .flatten();
    let status = hovered
        .and_then(|id| dialog.control(id))
        .and_then(|control| control.tooltip_key)
        .map(|key| resolve_csf(state, key).into_owned())
        .unwrap_or_default();
    let pressed = active.then(|| controller.pressed()).flatten();
    // VERA's 0x103 victory/campaign-end pages: the B25 board also carries the
    // statistics projection and the tab/stats-page state.
    #[cfg(feature = "vmce")]
    let (vmce_page, vmce_stats_open, vmce_statistics) = {
        let page = state
            .frontend
            .campaign_score_page
            .as_ref()
            .expect("checked before render");
        (
            page.vmce_board().is_some_and(|board| board.victory),
            page.vmce_stats_open(),
            page.vmce_statistics().cloned(),
        )
    };
    #[cfg(feature = "vmce")]
    let stats_open = vmce_page && vmce_stats_open && vmce_statistics.is_some();
    #[cfg(not(feature = "vmce"))]
    let stats_open = false;

    let mut labels: Vec<PaintLabel<'static>> = Vec::new();
    let campaign_page = state
        .frontend
        .campaign_score_page
        .as_mut()
        .expect("checked before render");
    if children {
        campaign_page.set_status_help(&status, now);
        for static_text in campaign_page.statics_mut() {
            // Advance every native kind-1 reveal first (the page's terminal
            // state depends on the hidden kills statics too), then skip the
            // enemies report's paint on VERA's pages: its label and value are
            // hidden, but the native top panel keeps its fixed height.
            let window = static_text.paint(now);
            #[cfg(feature = "vmce")]
            let hide = vmce_page
                && crate::ui::campaign_score_shell::vmce_page_hides_enemy_kills(static_text.id);
            #[cfg(not(feature = "vmce"))]
            let hide = false;
            let Some(window) = window else {
                continue;
            };
            if hide {
                continue;
            }
            let Some(rect) = layout.control_rect(static_text.id) else {
                continue;
            };
            labels.push(PaintLabel {
                text: static_text.text().to_owned().into(),
                rect,
                align: static_align(dialog, static_text.id),
                rgb: static_text.rgb.map_or(SHELL_TEXT_RGB_ENABLED, rgb_f32),
                path_a_reveal: Some(PathAReveal {
                    count: window.count,
                    range: window.range,
                    base_rgb: static_text.rgb.unwrap_or(SHELL_TEXT),
                    highlight_rgb: HIGHLIGHT,
                }),
            });
        }
        // `0xBC7`'s score cells paint whole in their row colour.
        for cell in campaign_page.cells() {
            let Some(rect) = layout.control_rect(cell.id) else {
                continue;
            };
            labels.push(PaintLabel {
                text: cell.text.clone().into(),
                rect,
                align: static_align(dialog, cell.id),
                rgb: rgb_f32(cell.rgb),
                path_a_reveal: None,
            });
        }
    }

    let chrome = state
        .frontend
        .main_menu_shell_chrome
        .as_ref()
        .expect("checked before render");
    let mut chrome_sprites = if state.frontend.campaign_score_art.is_none() {
        crate::app::frontend::main_menu_shell_render::shell_parent_background_instances(
            chrome,
            layout.screen.w,
            layout.screen.h,
            layout.scale,
        )
    } else {
        Vec::new()
    };
    chrome_sprites.extend(shell_paint::paint_chrome(
        chrome,
        layout.right_panel,
        Some(layout.lower_strip),
        layout.screen.w,
    ));
    chrome_sprites.extend(monitor_frame.and_then(|frame| {
        shell_paint::paint_warning_monitor(chrome, layout.warning_monitor, frame, layout.scale)
    }));

    // The board is an opaque overlay on top of SCRBK/SCRT, so its sprites are
    // their own draw submitted after the dialog art (a chrome draw would be
    // covered by the art).
    #[cfg(feature = "vmce")]
    let mut vmce_board_sprites: Vec<crate::render::batch::SpriteInstance> = Vec::new();
    #[cfg(feature = "vmce")]
    if !stats_open
        && let (Some(board_layout), Some(board)) = (
            layout.vmce_result,
            state
                .frontend
                .campaign_score_page
                .as_ref()
                .and_then(|page| page.vmce_board()),
        )
    {
        let board = board.clone();
        paint_vmce_result_board(
            state,
            &layout,
            board_layout,
            &board,
            &mut vmce_board_sprites,
            &mut labels,
        );
    }

    if let Some(wave) = wave.as_ref() {
        // The slide engine paints the whole right-panel column; the steady
        // Continue cell is not drawn until the wave ends.
        chrome_sprites.extend(shell_paint::paint_slide_column(
            chrome,
            layout.right_panel,
            &wave.button_draws(),
        ));
    }
    // VMCE's result page owns its right-panel buttons. Both roles see Continue;
    // the guest's button records readiness. Stock pages keep their own button.
    #[cfg(feature = "vmce")]
    let vmce_buttons = state
        .frontend
        .campaign_score_page
        .as_ref()
        .and_then(|page| {
            let board = page.vmce_board()?;
            let buttons = if stats_open && board.victory && page.vmce_statistics().is_some() {
                crate::ui::campaign_score_shell::vmce_stats_page_buttons(&layout)
            } else {
                let can_continue = if state.match_state.session.cooperative_round().is_some() {
                    state
                        .match_state
                        .session
                        .cooperative_round_continue_enabled()
                } else {
                    board.can_continue
                };
                crate::ui::campaign_score_shell::vmce_result_buttons(
                    &board,
                    &layout,
                    continue_id,
                    can_continue,
                )
            };
            Some(buttons)
        });
    let buttons: Vec<PaintButton> = if wave.is_some() {
        Vec::new()
    } else {
        #[cfg(feature = "vmce")]
        if let Some(vmce) = vmce_buttons.as_ref() {
            vmce.iter()
                .map(|button| PaintButton {
                    rect: button.rect,
                    pressed: button.pressed || pressed == Some(button.id),
                    hovered: hovered == Some(button.id),
                    enabled: button.enabled,
                })
                .collect()
        } else {
            vec![PaintButton {
                rect: layout.continue_button,
                pressed: pressed == Some(continue_id),
                hovered: false,
                enabled: true,
            }]
        }
        #[cfg(not(feature = "vmce"))]
        {
            vec![PaintButton {
                rect: layout.continue_button,
                pressed: pressed == Some(continue_id),
                hovered: false,
                enabled: true,
            }]
        }
    };
    let button_sprites = shell_paint::paint_buttons(
        chrome,
        &buttons,
        crate::app::frontend::menu_page_render::MENU_PAGE_BUTTON_POLICY,
        now,
        None,
    );
    if wave.is_none() {
        #[cfg(feature = "vmce")]
        if let Some(vmce) = vmce_buttons.as_ref() {
            for button in vmce {
                let is_pressed = button.pressed || pressed == Some(button.id);
                labels.push(PaintLabel {
                    text: resolve_csf(state, button.key).into_owned().into(),
                    rect: crate::ui::campaign_score_shell::vmce_button_caption_rect(button.rect),
                    align: ShellAlign::BUTTON_CAPTION,
                    rgb: if is_pressed {
                        crate::ui::campaign_score_shell::vmce_result_button_text_rgb(true)
                    } else if !button.enabled {
                        shell_paint::SHELL_TEXT_RGB_DISABLED
                    } else {
                        crate::ui::campaign_score_shell::vmce_result_button_text_rgb(false)
                    },
                    path_a_reveal: None,
                });
            }
        } else {
            labels.push(PaintLabel {
                text: resolve_csf(state, "GUI:Continue").into_owned().into(),
                rect: owner_draw_button_label_rect(
                    layout.continue_button,
                    pressed == Some(continue_id),
                ),
                align: ShellAlign::BUTTON_CAPTION,
                rgb: SHELL_TEXT_RGB_ENABLED,
                path_a_reveal: None,
            });
        }
        #[cfg(not(feature = "vmce"))]
        labels.push(PaintLabel {
            text: resolve_csf(state, "GUI:Continue").into_owned().into(),
            rect: owner_draw_button_label_rect(
                layout.continue_button,
                pressed == Some(continue_id),
            ),
            align: ShellAlign::BUTTON_CAPTION,
            rgb: SHELL_TEXT_RGB_ENABLED,
            path_a_reveal: None,
        });
    }
    #[cfg(feature = "vmce")]
    if vmce_page && stats_open {
        labels.clear();
        paint_vmce_stats_labels(state, &layout, vmce_statistics.as_ref(), &mut labels);
    }
    let text =
        shell_paint::paint_labels_scaled(&state.renderer.bit_font, &labels, layout.scale.factor());
    let mut draws = Vec::with_capacity(6);
    if let Some(art) = state.frontend.campaign_score_art.as_ref() {
        let background = layout.background;
        let mut background_sprites = Vec::with_capacity(1);
        // Paint mode 1's `Background_Overlay` (`0x0072E730` from
        // `WM_PAINT_Handler 0x00621E90`) draws SCRBK, `0x0060CF00`'s
        // background shape for both dialogs, at `[0x00B0FC1C]` with its
        // native 472x448 or 632x568 size (`0x103` also draws it once
        // before it runs, `0x0072E9F0`). At 1024x768 the origin is the
        // centred (112, 84), never (0, 0).
        shell_paint::push_entry_sized(
            &mut background_sprites,
            art.background,
            background.x as f32,
            background.y as f32,
            [background.w as f32, background.h as f32],
            PARENT_BACKGROUND_DEPTH,
        );
        draws.push(TexturedDraw {
            texture: &art.texture,
            instances: background_sprites,
        });
    }
    draws.push(TexturedDraw {
        texture: &chrome.texture,
        instances: chrome_sprites,
    });
    if let Some(art) = state.frontend.campaign_score_art.as_ref() {
        // BC6-style stats backdrop: the failure page draws SCRBK alone; the
        // 0x103 SCRT panel (the armoured-eagle dialog art) is skipped.
        if !stats_open
            && let (Some(strip), Some(frame)) = (art.scrt.as_ref(), scrt_frame)
            && let Some(entry) = strip.frames.get(frame).copied()
        {
            // `WM_PAINT_Handler 0x00621E90` draws the right-panel chrome
            // first, then `RadarBackground 0x0072E920` submits SCRT frame 0
            // at `[0x00B0FC1C]`, plus x=0x50 at 800 pixels and wider.
            let panel = layout.panel;
            let mut panel_sprites = Vec::with_capacity(1);
            shell_paint::push_entry_sized(
                &mut panel_sprites,
                entry,
                panel.x as f32,
                panel.y as f32,
                [panel.w as f32, panel.h as f32],
                DIALOG_BACKGROUND_DEPTH,
            );
            draws.push(TexturedDraw {
                texture: &strip.texture,
                instances: panel_sprites,
            });
        }
        if !stats_open
            && let (Some(strip), Some(frame)) = (art.scra.as_ref(), emblem_frame)
            && let (Some(entry), Some(window)) = (
                strip.frames.get(frame).copied(),
                layout.control_rect(SCORE_ART_REGION_STATIC),
            )
            && let Some(sprite) = shell_paint::paint_kind4_static(entry, window, layout.scale)
        {
            draws.push(TexturedDraw {
                texture: &strip.texture,
                instances: vec![sprite],
            });
        }
    }
    draws.push(TexturedDraw {
        texture: &chrome.texture,
        instances: button_sprites,
    });
    #[cfg(feature = "vmce")]
    if !vmce_board_sprites.is_empty()
        && let Some(skirmish) = state.frontend.skirmish_shell_chrome.as_ref()
    {
        draws.push(TexturedDraw {
            texture: &skirmish.texture,
            instances: vmce_board_sprites,
        });
    }
    encode_shell_pass(
        state,
        encoder,
        destination,
        "Campaign Score Shell",
        ShellComposition {
            draws: &draws,
            text: &text,
            cursor: software_cursor(state, CURSOR_DEPTH),
            effects: None,
            smooth_art: false,
        },
    );
    Ok(true)
}

/// The low two bits of Win32 STATIC style select left, centre or right.
fn static_align(dialog: CampaignScoreDialog, id: u16) -> ShellAlign {
    match dialog.control(id).map(|control| control.native_style & 0x3) {
        Some(1) => ShellAlign::H_CENTER,
        Some(2) => ShellAlign::H_RIGHT,
        _ => ShellAlign::NONE,
    }
}

fn rgb_f32(rgb: [u8; 3]) -> [f32; 3] {
    rgb.map(|channel| f32::from(channel) / 255.0)
}

/// The full-screen statistics page's text: time header, six native headings,
/// four role rows in House colours and the right-panel Back caption. The page
/// reuses the native BC6-style backdrop, so there is no panel fill and no row
/// strip; only the right panel's first slot carries a normal Back button.
#[cfg(feature = "vmce")]
fn paint_vmce_stats_labels(
    state: &AppState,
    layout: &crate::ui::campaign_score_shell::CampaignScoreLayout,
    statistics: Option<&crate::ui::campaign_score_shell::VmceStatistics>,
    labels: &mut Vec<PaintLabel<'static>>,
) {
    let Some(statistics) = statistics else {
        return;
    };
    let stats_layout = layout.vmce_statistics;
    let mut push = |text: String, rect, align, rgb| {
        labels.push(PaintLabel {
            text: text.into(),
            rect,
            align,
            rgb,
            path_a_reveal: None,
        });
    };
    push(
        crate::i18n::tr("VERA:VmceStatistics"),
        stats_layout.title,
        ShellAlign::H_CENTER,
        SHELL_TEXT_RGB_ENABLED,
    );
    push(
        format!(
            "{} {}",
            resolve_csf(state, "GUI:YourTime"),
            statistics.total_time
        ),
        stats_layout.total_time,
        ShellAlign::NONE,
        SHELL_TEXT_RGB_ENABLED,
    );
    push(
        format!(
            "{} {}",
            resolve_csf(state, "GUI:ParTime"),
            statistics.par_time
        ),
        stats_layout.par_time,
        ShellAlign::NONE,
        SHELL_TEXT_RGB_ENABLED,
    );
    for header in stats_layout.headers {
        let text = if let Some(key) = header.key.strip_prefix("VERA:") {
            crate::i18n::tr(&format!("VERA:{key}"))
        } else {
            resolve_csf(state, header.key).into_owned()
        };
        push(
            text,
            header.rect,
            if header.centred {
                ShellAlign::H_CENTER
            } else {
                ShellAlign::NONE
            },
            SHELL_TEXT_RGB_ENABLED,
        );
    }
    let mut row_rgb = |row: &crate::ui::campaign_score_shell::VmceStatisticsRow| {
        crate::ui::campaign_score_shell::vmce_result_text_rgb(
            &crate::ui::campaign_score_shell::VmceResultRow {
                player: String::new(),
                role: String::new(),
                country: String::new(),
                flag: None,
                won: true,
                ready: None,
                host: false,
                rgb: row.rgb,
            },
        )
        .map(|channel| f32::from(channel) / 255.0)
    };
    for (row, cells) in statistics.rows.iter().zip(stats_layout.rows.iter()) {
        let rgb = row_rgb(row);
        let values = [
            row.player.clone(),
            row.kills.to_string(),
            row.losses.to_string(),
            row.built.to_string(),
            row.score.to_string(),
            row.income.to_string(),
        ];
        for (column, value) in values.iter().enumerate() {
            let rect = cells.cells[column];
            let align = if column == 0 {
                ShellAlign::NONE
            } else {
                ShellAlign::H_CENTER
            };
            push(value.clone(), rect, align, rgb);
        }
    }
    let back = crate::ui::campaign_score_shell::vmce_button_caption_rect(layout.vmce_leave_button);
    push(
        crate::i18n::tr("VERA:VmceBack"),
        back,
        ShellAlign::BUTTON_CAPTION,
        SHELL_TEXT_RGB_ENABLED,
    );
}

/// VMCE's settlement board: one flat native dialog fill and the native list
/// frame behind four role rows and their single Ready glyph. Every row's text
/// uses its authored House colour directly on the native panel; captions and
/// icons keep the native yellow/art.
/// The board never touches the native `0x103` statics (rank, commendation,
/// times, kills) or the Continue cell.
#[cfg(feature = "vmce")]
#[allow(clippy::too_many_arguments)]
fn paint_vmce_result_board(
    state: &AppState,
    layout: &crate::ui::campaign_score_shell::CampaignScoreLayout,
    board_layout: crate::ui::campaign_score_shell::VmceResultLayout,
    board: &crate::ui::campaign_score_shell::VmceResultBoard,
    sprites: &mut Vec<crate::render::batch::SpriteInstance>,
    labels: &mut Vec<PaintLabel<'static>>,
) {
    use crate::app::frontend::skirmish_shell_render::vmce_panel;
    let Some(chrome) = state.frontend.skirmish_shell_chrome.as_ref() else {
        return;
    };
    let scale = layout.scale;
    let depth = shell_paint::DIALOG_BACKGROUND_DEPTH;
    vmce_panel::push_fill(sprites, chrome, board_layout.fill, depth);
    vmce_panel::push_list_frame(sprites, chrome, board_layout.frame, scale, depth);
    vmce_panel::push_rule(sprites, chrome, board_layout.header_rule, depth);
    for header in board_layout.headers {
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
    let font = &state.renderer.bit_font;
    let mut push = |text: String, rect, align, rgb: [f32; 3]| {
        labels.push(PaintLabel {
            text: text.into(),
            rect,
            align,
            rgb,
            path_a_reveal: None,
        });
    };
    for (row, cells) in board.rows.iter().zip(board_layout.rows.iter()) {
        let rgb = crate::ui::campaign_score_shell::vmce_result_text_rgb(row)
            .map(|channel| f32::from(channel) / 255.0);
        push(
            vmce_panel::ellipsize_native(font, &row.player, scale.base_rect(cells.player).w),
            cells.player,
            ShellAlign::NONE,
            rgb,
        );
        push(
            vmce_panel::ellipsize_native(font, &row.role, scale.base_rect(cells.role).w),
            cells.role,
            ShellAlign::NONE,
            rgb,
        );
        if let Some(flag) = row
            .flag
            .as_deref()
            .and_then(|flag| chrome.lan_player_side(flag))
        {
            vmce_panel::push_native_centred(sprites, flag, cells.country_icon, scale);
        }
        if !row.country.is_empty() {
            push(
                vmce_panel::ellipsize_native(font, &row.country, scale.base_rect(cells.country).w),
                cells.country,
                ShellAlign::NONE,
                rgb,
            );
        }
        push(
            crate::i18n::tr(if row.won {
                "VERA:VmceResultWon"
            } else {
                "VERA:VmceResultLost"
            }),
            cells.verdict,
            ShellAlign::H_CENTER,
            rgb,
        );
        let icon = if row.host {
            chrome.lan_player_status(true, false)
        } else if row.ready == Some(true) {
            chrome.lan_player_status(false, true)
        } else {
            None
        };
        if let Some(icon) = icon {
            vmce_panel::push_native_centred(sprites, icon, cells.ready, scale);
        }
    }
}
