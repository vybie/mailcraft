// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windows notification-area icon. Created and retained on the winit event-loop thread.

use crate::tray_state::TrayState;
use egui::{Context, IconData};
use mailcraft_ui_egui::{MailApp, i18n};
use std::sync::mpsc::{Receiver, sync_channel};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const OPEN: &str = "mailcraft.tray.open";
const QUIT: &str = "mailcraft.tray.quit";

#[derive(Clone, Copy)]
enum Action {
    Open,
    Quit,
}

pub(crate) struct Tray {
    icon: TrayIcon,
    open: MenuItem,
    quit: MenuItem,
    events: Receiver<Action>,
    lang: i18n::Lang,
    state: TrayState,
}

impl Tray {
    pub(crate) fn new(ctx: &Context, icon: Option<IconData>) -> Option<Self> {
        match Self::create(ctx, icon) {
            Ok(tray) => Some(tray),
            Err(error) => {
                log::warn!("the notification-area icon could not be created: {error}");
                None
            }
        }
    }

    fn create(ctx: &Context, icon: Option<IconData>) -> Result<Self, Box<dyn std::error::Error>> {
        let data = icon.ok_or("the application icon is unavailable")?;
        let icon = Icon::from_rgba(data.rgba, data.width, data.height)?;
        let menu = Menu::new();
        let open = MenuItem::with_id(OPEN, i18n::t("Open"), true, None);
        let quit = MenuItem::with_id(QUIT, i18n::t("Quit MailCraft"), true, None);
        menu.append_items(&[&open, &quit])?;
        let icon =
            TrayIconBuilder::new().with_menu(Box::new(menu)).with_icon(icon).with_tooltip("MailCraft").with_menu_on_left_click(false).build()?;
        let (tx, events) = sync_channel(32);
        let wake = ctx.clone();
        let menu_tx = tx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let action = match event.id.as_ref() {
                OPEN => Some(Action::Open),
                QUIT => Some(Action::Quit),
                _ => None,
            };
            if let Some(action) = action
                && menu_tx.try_send(action).is_ok()
            {
                wake.request_repaint();
            }
        }));
        let wake = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. })
                && tx.try_send(Action::Open).is_ok()
            {
                wake.request_repaint();
            }
        }));
        Ok(Self { icon, open, quit, events, lang: i18n::current(), state: TrayState::default() })
    }

    pub(crate) fn show(&mut self, ctx: &Context) {
        self.state.show(ctx);
    }

    pub(crate) fn pump(&mut self, ctx: &Context, app: &mut MailApp) {
        if self.lang != i18n::current() {
            self.lang = i18n::current();
            self.open.set_text(i18n::t("Open"));
            self.quit.set_text(i18n::t("Quit MailCraft"));
        }
        for action in self.events.try_iter().take(32) {
            self.state.show(ctx);
            if matches!(action, Action::Quit) {
                app.quit_requested = true;
            }
        }
    }

    pub(crate) fn after_quit(&mut self, app: &MailApp) {
        self.state.after_quit(app);
    }

    pub(crate) fn before_quit(&mut self, ctx: &Context, app: &MailApp) -> bool {
        // Creating an icon may succeed even if Explorer has not registered it. Its actual
        // rectangle is the available test, including after Explorer restarts.
        let available = !self.state.needs_tray_check(ctx) || self.icon.rect().is_some();
        self.state.before_quit(ctx, app, available)
    }
}
