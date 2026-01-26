use crate::app::background;
use crate::app::messages::BackgroundMessage;
use crate::app::RuntimeContext;
use crate::ui::{App, LoadingState, Overlay};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle key events when an overlay is active
pub(super) fn handle_overlay_input(
    app: &mut App,
    key: KeyEvent,
    ctx: &RuntimeContext,
) -> Result<()> {
    // Handle overlay mode
    if app.overlay != Overlay::None {
        // Handle Reset cosmos overlay
        if let Overlay::Reset { .. } = &app.overlay {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    app.close_overlay();
                }
                KeyCode::Down => {
                    app.reset_navigate(1);
                }
                KeyCode::Up => {
                    app.reset_navigate(-1);
                }
                KeyCode::Char(' ') => {
                    app.reset_toggle_selected();
                }
                KeyCode::Enter => {
                    let selected = app.get_reset_selections();
                    if selected.is_empty() {
                        app.show_toast("Select at least one reset option");
                        return Ok(());
                    }

                    app.loading = LoadingState::Resetting;
                    app.close_overlay();

                    let tx_reset = ctx.tx.clone();
                    let repo_path = app.repo_path.clone();
                    background::spawn_background(ctx.tx.clone(), "reset_cosmos", async move {
                        match crate::cache::reset_cosmos(&repo_path, &selected).await {
                            Ok(_) => {
                                let _ = tx_reset
                                    .send(BackgroundMessage::ResetComplete { options: selected });
                            }
                            Err(e) => {
                                let _ = tx_reset.send(BackgroundMessage::Error(e.to_string()));
                            }
                        }
                    });
                }
                _ => {}
            }
            return Ok(());
        }

        // Handle Startup Check overlay
        if let Overlay::StartupCheck {
            confirming_discard, ..
        } = &app.overlay
        {
            let confirming = *confirming_discard;
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    app.close_overlay();
                }
                KeyCode::Down => {
                    app.overlay_scroll_down();
                }
                KeyCode::Up => {
                    app.overlay_scroll_up();
                }
                // === Confirmation mode handlers ===
                KeyCode::Char('y') if confirming => {
                    // Confirm discard - actually discard changes
                    app.loading = LoadingState::Discarding;
                    app.close_overlay();
                    let tx = ctx.tx.clone();
                    let repo_path = app.repo_path.clone();
                    background::spawn_background(ctx.tx.clone(), "discard_changes", async move {
                        match crate::git_ops::discard_all_changes(&repo_path) {
                            Ok(()) => {
                                let _ = tx.send(BackgroundMessage::DiscardComplete);
                            }
                            Err(e) => {
                                let _ = tx.send(BackgroundMessage::Error(e.to_string()));
                            }
                        }
                    });
                }
                KeyCode::Enter if confirming => {
                    // Confirm discard via Enter
                    app.loading = LoadingState::Discarding;
                    app.close_overlay();
                    let tx = ctx.tx.clone();
                    let repo_path = app.repo_path.clone();
                    background::spawn_background(ctx.tx.clone(), "discard_changes", async move {
                        match crate::git_ops::discard_all_changes(&repo_path) {
                            Ok(()) => {
                                let _ = tx.send(BackgroundMessage::DiscardComplete);
                            }
                            Err(e) => {
                                let _ = tx.send(BackgroundMessage::Error(e.to_string()));
                            }
                        }
                    });
                }
                KeyCode::Char('n') if confirming => {
                    // Cancel confirmation - go back to main menu
                    app.startup_check_confirm_discard(false);
                }
                KeyCode::Char('c') if confirming => {
                    // Cancel confirmation - go back to main menu
                    app.startup_check_confirm_discard(false);
                }
                // === Initial menu handlers ===
                KeyCode::Char('s') if !confirming => {
                    // Save my work and start fresh (git stash)
                    app.loading = LoadingState::Stashing;
                    app.close_overlay();
                    let tx = ctx.tx.clone();
                    let repo_path = app.repo_path.clone();
                    background::spawn_background(ctx.tx.clone(), "stash_changes", async move {
                        match crate::git_ops::stash_changes(&repo_path) {
                            Ok(message) => {
                                let _ = tx.send(BackgroundMessage::StashComplete { message });
                            }
                            Err(e) => {
                                let _ = tx.send(BackgroundMessage::Error(e.to_string()));
                            }
                        }
                    });
                }
                KeyCode::Char('d') if !confirming => {
                    // Discard and start fresh - show confirmation
                    app.startup_check_confirm_discard(true);
                }
                KeyCode::Char('c') if !confirming => {
                    // Continue as-is - just close the overlay
                    app.close_overlay();
                }
                _ => {}
            }
            return Ok(());
        }

        // Handle other overlays (generic scroll/close)
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
            KeyCode::Down => app.overlay_scroll_down(),
            KeyCode::Up => app.overlay_scroll_up(),
            _ => {}
        }
        return Ok(());
    }

    Ok(())
}
