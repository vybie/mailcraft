// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windows notification-area icon. Created and retained on the winit event-loop thread.

use crate::tray_state::TrayState;
use egui::{Context, IconData};
use mailcraft_model::FolderKind;
use mailcraft_ui_egui::{MailApp, i18n};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::collections::HashSet;
use std::sync::mpsc::{Receiver, sync_channel};
use std::time::{Duration, Instant};
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
    base_icon: IconData,
    unread: Option<usize>,
    revision: Option<(u64, u64)>,
    overlay_at: Option<Instant>,
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
        let icon = Icon::from_rgba(data.rgba.clone(), data.width, data.height)?;
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
        Ok(Self { icon, open, quit, events, lang: i18n::current(), state: TrayState::default(), base_icon: data, unread: None, revision: None, overlay_at: None })
    }

    pub(crate) fn is_hidden(&self) -> bool {
        self.state.is_hidden()
    }

    pub(crate) fn update_unread(&mut self, ctx: &Context, app: &MailApp, frame: &eframe::Frame) {
        if app.is_opening() {
            self.revision = None;
            return;
        }
        let revision = (app.session.openings(), app.session.rev());
        if self.revision != Some(revision) {
            self.revision = Some(revision);
            let mailbox = &app.session.mailbox;
            let inboxes: HashSet<_> = mailbox.folders.iter().filter(|f| f.kind == FolderKind::Inbox).map(|f| f.id).collect();
            let unread = mailbox.messages.values().filter(|m| !m.read && inboxes.contains(&m.folder)).count();
            if self.unread != Some(unread) {
                let data = tray_icon(&self.base_icon, unread);
                if let Ok(icon) = Icon::from_rgba(data.rgba, data.width, data.height) {
                    if let Err(error) = self.icon.set_icon(Some(icon)) { log::warn!("tray icon: {error}"); }
                    let tooltip = if unread == 0 { "MailCraft".to_string() } else { format!("MailCraft — {unread} unread") };
                    let _ = self.icon.set_tooltip(Some(tooltip));
                }
                self.unread = Some(unread);
                self.overlay_at = None;
            }
        }
        // Explorer may create the taskbar button after the mailbox opens or restart later.
        if self.overlay_at.is_none_or(|at| at.elapsed() >= Duration::from_secs(10))
            && let Some(window) = frame.winit_window()
            && let Ok(handle) = window.window_handle()
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            let count = self.unread.unwrap_or(0);
            let badge = (count > 0).then(|| badge_icon(count));
            let result = mailcraft_windows_badge::set_overlay(handle.hwnd.get(), badge.as_ref().map(|icon| icon.rgba.as_slice()), 64, &format!("{count} unread"));
            if let Err(error) = result { log::warn!("taskbar badge: {error}"); }
            self.overlay_at = Some(Instant::now());
        }
        ctx.request_repaint_after(Duration::from_secs(10));
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

/// Keep the app icon recognizable with a compact badge in its top corner.
fn tray_icon(base: &IconData, unread: usize) -> IconData {
    if unread == 0 {
        return base.clone();
    }
    let Some(base_image) = image::RgbaImage::from_raw(base.width, base.height, base.rgba.clone()) else { return base.clone() };
    let mut image = image::imageops::resize(&base_image, 64, 64, image::imageops::FilterType::Lanczos3);
    for y in 0_i32..34 {
        for x in 30_i32..64 {
            if (x - 47).pow(2) + (y - 17).pow(2) <= 17 * 17 {
                image.put_pixel(x as u32, y as u32, image::Rgba([255, 255, 255, 255]));
            }
        }
    }
    let badge = badge_icon(unread);
    if let Some(badge_image) = image::RgbaImage::from_raw(badge.width, badge.height, badge.rgba) {
        let badge_image = image::imageops::resize(&badge_image, 30, 30, image::imageops::FilterType::Lanczos3);
        image::imageops::overlay(&mut image, &badge_image, 32, 2);
    }
    IconData { rgba: image.into_raw(), width: 64, height: 64 }
}

/// Draw a count for the Windows taskbar overlay and tray icon.
fn badge_icon(unread: usize) -> IconData {
    let mut image = image::RgbaImage::new(64, 64);
    for y in 0_i32..64 {
        for x in 0_i32..64 {
            if (x - 31).pow(2) + (y - 31).pow(2) <= 31 * 31 {
                image.put_pixel(x as u32, y as u32, image::Rgba([150, 12, 23, 255]));
            }
        }
    }
    const DIGITS: [[u8; 5]; 11] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 1, 1],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
        [0, 2, 7, 2, 0],
    ];
    let label = if unread > 99 { "99+".to_string() } else { unread.to_string() };
    let scale = 5_i32;
    let width = (label.len() as i32 * 4 - 1) * scale;
    let left = 32 - width / 2;
    for (index, character) in label.chars().enumerate() {
        let digit = character.to_digit(10).map_or(10, |n| n as usize);
        for (row, bits) in DIGITS[digit].iter().enumerate() {
            for col in 0..3 {
                if bits & (1 << (2 - col)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let x = left + index as i32 * 4 * scale + col * scale + dx;
                            let y = 19 + row as i32 * scale + dy;
                            image.put_pixel(x as u32, y as u32, image::Rgba([255, 255, 255, 255]));
                        }
                    }
                }
            }
        }
    }
    IconData { rgba: image.into_raw(), width: 64, height: 64 }
}
