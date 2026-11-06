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
        self.state.before_quit(ctx, app)
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
                image.put_pixel(x as u32, y as u32, image::Rgba([255, 185, 45, 255]));
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
    // Render at twice the target size, then downsample for smooth circular edges.
    let mut image = image::RgbaImage::new(128, 128);
    for y in 0_i32..128 {
        for x in 0_i32..128 {
            if (x - 64).pow(2) + (y - 64).pow(2) <= 63 * 63 {
                image.put_pixel(x as u32, y as u32, image::Rgba([255, 185, 45, 255]));
            }
        }
    }
    let label = if unread > 99 { "99+".to_string() } else { unread.to_string() };
    if draw_badge_text(&mut image, &label) {
        let image = image::imageops::resize(&image, 64, 64, image::imageops::FilterType::Lanczos3);
        return IconData { rgba: image.into_raw(), width: 64, height: 64 };
    }
    // Keep a readable count if both the system font and the bundled fallback fail to load.
    const DIGITS: [[u8; 7]; 11] = [
        [14, 17, 19, 21, 25, 17, 14],
        [4, 12, 4, 4, 4, 4, 14],
        [14, 17, 1, 2, 4, 8, 31],
        [30, 1, 1, 14, 1, 1, 30],
        [2, 6, 10, 18, 31, 2, 2],
        [31, 16, 16, 30, 1, 1, 30],
        [14, 16, 16, 30, 17, 17, 14],
        [31, 1, 2, 4, 8, 8, 8],
        [14, 17, 17, 14, 17, 17, 14],
        [14, 17, 17, 15, 1, 1, 14],
        [0, 4, 4, 31, 4, 4, 0],
    ];
    let scale = if label.len() > 2 { 6_i32 } else { 9_i32 };
    let width = (label.len() as i32 * 6 - 1) * scale;
    let left = 64 - width / 2;
    for (index, character) in label.chars().enumerate() {
        let digit = character.to_digit(10).map_or(10, |n| n as usize);
        for (row, bits) in DIGITS[digit].iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let x = left + index as i32 * 6 * scale + col * scale + dx;
                            let y = (128 - 7 * scale) / 2 + row as i32 * scale + dy;
                            image.put_pixel(x as u32, y as u32, image::Rgba([35, 29, 19, 255]));
                        }
                    }
                }
            }
        }
    }
    let image = image::imageops::resize(&image, 64, 64, image::imageops::FilterType::Lanczos3);
    IconData { rgba: image.into_raw(), width: 64, height: 64 }
}

/// Rasterize real font outlines at badge resolution; Windows supplies Segoe UI.
fn draw_badge_text(image: &mut image::RgbaImage, label: &str) -> bool {
    use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
    use std::sync::OnceLock;

    static FONT: OnceLock<Option<FontArc>> = OnceLock::new();
    let font = FONT.get_or_init(|| {
        let system = std::env::var_os("WINDIR")
            .and_then(|dir| std::fs::read(std::path::PathBuf::from(dir).join("Fonts").join("segoeui.ttf")).ok())
            .and_then(|bytes| FontArc::try_from_vec(bytes).ok());
        system.or_else(|| {
            let definitions = egui::FontDefinitions::default();
            let name = definitions.families.get(&egui::FontFamily::Proportional)?.first()?;
            let font = definitions.font_data.get(name)?;
            FontArc::try_from_vec(font.bytes().to_vec()).ok()
        })
    });
    let Some(font) = font else { return false };
    let scale = PxScale::from(if label.len() > 2 { 76.0 } else { 100.0 });
    let scaled = font.as_scaled(scale);
    let mut glyphs = Vec::new();
    let mut x = 0.0;
    let mut previous = None;
    for ch in label.chars() {
        let id = font.glyph_id(ch);
        if let Some(prior) = previous { x += scaled.kern(prior, id); }
        let glyph = id.with_scale_and_position(scale, point(x, 0.0));
        x += scaled.h_advance(id);
        previous = Some(id);
        if let Some(outline) = font.outline_glyph(glyph) { glyphs.push(outline); }
    }
    if glyphs.is_empty() { return false; }
    let bounds = glyphs.iter().map(|g| g.px_bounds()).fold(None, |acc: Option<(f32, f32, f32, f32)>, b| {
        Some(acc.map_or((b.min.x, b.min.y, b.max.x, b.max.y), |(x0, y0, x1, y1)| {
            (x0.min(b.min.x), y0.min(b.min.y), x1.max(b.max.x), y1.max(b.max.y))
        }))
    });
    let Some((x0, y0, x1, y1)) = bounds else { return false };
    let offset_x = ((128.0 - (x1 - x0)) / 2.0 - x0).round() as i32;
    let offset_y = ((128.0 - (y1 - y0)) / 2.0 - y0).round() as i32;
    for glyph in glyphs {
        let bounds = glyph.px_bounds();
        glyph.draw(|x, y, coverage| {
            let x = bounds.min.x as i32 + x as i32 + offset_x;
            let y = bounds.min.y as i32 + y as i32 + offset_y;
            if x < 0 || y < 0 { return; }
            if let Some(pixel) = image.get_pixel_mut_checked(x as u32, y as u32) {
                for (channel, ink) in pixel.0.iter_mut().take(3).zip([35.0, 29.0, 19.0]) {
                    *channel = (*channel as f32 * (1.0 - coverage) + ink * coverage).round() as u8;
                }
            }
        });
    }
    true
}
