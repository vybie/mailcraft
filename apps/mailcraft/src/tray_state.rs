// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! The decision to minimize, show or quit, independent of the Windows tray implementation.

use egui::{Context, ViewportCommand};
use mailcraft_ui_egui::MailApp;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct TrayState {
    hidden: bool,
    explicit_close: bool,
    held_quit: bool,
}

impl TrayState {
    pub(crate) fn is_hidden(&self) -> bool {
        self.hidden
    }

    pub(crate) fn show(&mut self, ctx: &Context) {
        self.hidden = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    pub(crate) fn after_quit(&mut self, app: &MailApp) {
        if app.quit_in_progress() {
            self.explicit_close = true;
            self.held_quit = true;
        }
    }

    /// Return true when close was handled by minimizing the running window.
    pub(crate) fn before_quit(&mut self, ctx: &Context, app: &MailApp) -> bool {
        // An alert or another host path may already have brought the window back.
        if ctx.viewport(|v| v.commands.contains(&ViewportCommand::Visible(true)))
            || (self.hidden && ctx.input(|i| i.focused && i.viewport().minimized == Some(false)))
        {
            self.hidden = false;
        }
        if self.held_quit && !app.quit_in_progress() && !app.quit_requested {
            self.explicit_close = false;
            self.held_quit = false;
        }
        if app.quit_requested {
            // A held quit has resumed: keep its close armed even though the held state
            // has ended by the time the generated close event arrives.
            self.held_quit = false;
            self.explicit_close = true;
            if self.hidden {
                self.show(ctx);
            }
        }
        if self.hidden {
            ctx.request_repaint_after(Duration::from_secs(2));
        }
        if !ctx.input(|i| i.viewport().close_requested()) {
            return false;
        }
        if self.explicit_close {
            // The shutdown decision is left to quit.rs. If it waits for a prompt or a save,
            // its next `quit_requested` arms the next close; Cancel leaves ordinary X as hide.
            self.explicit_close = false;
            return false;
        }
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        self.hidden = true;
        ctx.request_repaint_after(Duration::from_secs(2));
        true
    }
}

#[cfg(test)]
#[path = "tray_state_tests.rs"]
mod tray_state_tests;
