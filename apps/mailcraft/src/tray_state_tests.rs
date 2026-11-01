// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Close-to-tray decisions, including the existing quit and draft safeguards.

use super::*;
use egui::{RawInput, ViewportEvent, ViewportId};
use mailcraft_engine::Session;
use mailcraft_model::Mailbox;
use mailcraft_ui_egui::Services;
use serde_json::json;

fn app() -> MailApp {
    MailApp::new(Session::new(Mailbox::default()), Services::default())
}

fn frame(state: &mut TrayState, app: &mut MailApp, close: bool, available: bool) -> Vec<ViewportCommand> {
    let ctx = Context::default();
    let mut input = RawInput::default();
    if close {
        input.viewports.entry(ViewportId::ROOT).or_default().events.push(ViewportEvent::Close);
    }
    let mut out = ctx.run_logic(&input, |ctx| {
        if !state.before_quit(ctx, app, available) {
            crate::quit::on_frame(ctx, app);
            state.after_quit(app);
        }
    });
    out.viewport_commands.remove(&ViewportId::ROOT).unwrap_or_default()
}

#[test]
fn x_minimizes_without_closing_or_discarding_a_dirty_draft() {
    let mut state = TrayState::default();
    let mut app = app();
    app.session.run("mail.new", &json!({})).unwrap();
    app.session.compose.as_mut().unwrap().dirty = true;
    let commands = frame(&mut state, &mut app, true, true);
    assert!(commands.contains(&ViewportCommand::CancelClose));
    assert!(commands.contains(&ViewportCommand::Minimized(true)));
    assert!(state.hidden && app.dialog.is_none() && app.session.compose.as_ref().unwrap().dirty);
}

#[test]
fn unavailable_tray_preserves_the_existing_close_and_draft_question() {
    let mut state = TrayState::default();
    let mut app = app();
    app.session.run("mail.new", &json!({})).unwrap();
    app.session.compose.as_mut().unwrap().dirty = true;
    let commands = frame(&mut state, &mut app, true, false);
    assert!(!commands.contains(&ViewportCommand::Minimized(true)));
    assert!(commands.contains(&ViewportCommand::CancelClose));
    assert!(app.dialog.is_some() && !state.hidden);
}

#[test]
fn explicit_quit_reopens_before_asking_and_its_next_close_is_not_hidden() {
    let mut state = TrayState::default();
    let mut app = app();
    app.session.run("mail.new", &json!({})).unwrap();
    app.session.compose.as_mut().unwrap().dirty = true;
    frame(&mut state, &mut app, true, true);
    app.quit_requested = true;
    let commands = frame(&mut state, &mut app, false, true);
    assert!(commands.contains(&ViewportCommand::Visible(true)) && commands.contains(&ViewportCommand::Focus));
    assert!(commands.contains(&ViewportCommand::Close));
    let commands = frame(&mut state, &mut app, true, true);
    assert!(commands.contains(&ViewportCommand::CancelClose));
    assert!(!commands.contains(&ViewportCommand::Minimized(true)) && app.dialog.is_some());
    // A second X while the quit question is pending is still a quit, not a hide.
    let commands = frame(&mut state, &mut app, true, true);
    assert!(!commands.contains(&ViewportCommand::Minimized(true)) && app.dialog.is_some());
    // Cancel clears the intent. A later X hides and keeps the draft.
    app.dialog = None;
    let commands = frame(&mut state, &mut app, true, true);
    assert!(commands.contains(&ViewportCommand::Minimized(true)) && app.session.compose.as_ref().unwrap().dirty);
}

#[test]
fn a_later_quit_retry_is_still_a_quit() {
    let mut state = TrayState::default();
    let mut app = app();
    app.quit_requested = true;
    frame(&mut state, &mut app, false, true);
    frame(&mut state, &mut app, true, true);
    // The saver or a dialog asks again asynchronously.
    app.quit_requested = true;
    frame(&mut state, &mut app, false, true);
    let commands = frame(&mut state, &mut app, true, true);
    assert!(!commands.contains(&ViewportCommand::Minimized(true)));
}

#[test]
fn loss_of_the_tray_restores_a_hidden_window() {
    let mut state = TrayState::default();
    let mut app = app();
    frame(&mut state, &mut app, true, true);
    let commands = frame(&mut state, &mut app, false, false);
    assert!(commands.contains(&ViewportCommand::Visible(true)));
    assert!(commands.contains(&ViewportCommand::Minimized(false)) && commands.contains(&ViewportCommand::Focus));
    assert!(!state.hidden);
}

#[test]
fn without_a_close_minimize_is_not_converted_to_hide() {
    let mut state = TrayState::default();
    let mut app = app();
    let commands = frame(&mut state, &mut app, false, true);
    assert!(!commands.contains(&ViewportCommand::Minimized(true)) && !state.hidden);
}

#[test]
fn an_answered_quit_question_keeps_the_following_close_explicit() {
    let mut state = TrayState::default();
    let mut app = app();
    app.session.run("mail.new", &json!({})).unwrap();
    app.session.compose.as_mut().unwrap().dirty = true;
    app.quit_requested = true;
    frame(&mut state, &mut app, false, true);
    frame(&mut state, &mut app, true, true);
    assert!(app.dialog.is_some());
    app.dialog = None;
    app.session.compose = None;
    app.quit_requested = true;
    assert!(frame(&mut state, &mut app, false, true).contains(&ViewportCommand::Close));
    let commands = frame(&mut state, &mut app, true, true);
    assert!(!commands.contains(&ViewportCommand::Minimized(true)));
}
