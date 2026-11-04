// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Main-window WebView2 child and its message-scoped bridge to the reading pane.

use std::collections::HashSet;
use std::sync::Arc;

use mailcraft_model::Message;
use mailcraft_ui_egui::{MailApp, NativeHtmlImages, NativeHtmlPane, NativePictureAccess, native_html_same_message};
use mailcraft_windows_reading_webview::Host;

#[derive(Default)]
pub struct Pane {
    current: Option<Current>,
    failed: Option<Arc<Message>>,
    next_generation: u64,
}

struct Current {
    message: Arc<Message>,
    app_instance: Arc<()>,
    mailbox_opening: u64,
    host: Host,
    access: NativePictureAccess,
    supplied: HashSet<String>,
    dirty: bool,
    picture_cache_generation: u64,
    dark: bool,
}

impl Pane {
    pub fn update(&mut self, app: &mut MailApp, ctx: &egui::Context, frame: &eframe::Frame) {
        self.update_inner(app, ctx, frame);
        let changed = match (&app.native_html_failed, &self.failed) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            app.native_html_failed = self.failed.clone();
            ctx.request_repaint();
        }
    }

    fn update_inner(&mut self, app: &mut MailApp, ctx: &egui::Context, frame: &eframe::Frame) {
        let Some(pane) = app.native_html_pane.take() else {
            self.current = None;
            app.native_html_images = None;
            return;
        };
        if self.current.as_ref().is_some_and(|current| current.picture_cache_generation != pane.picture_cache_generation) {
            self.current = None;
            app.native_html_images = None;
        }
        if app.hidden_in_tray || app.dialog.is_some() || egui::Popup::is_any_open(ctx) {
            if let Some(current) = &self.current { let _ = current.host.hide(); }
            return;
        }
        let same = |message: &Arc<Message>| Arc::ptr_eq(message, &pane.message);
        if self.failed.as_ref().is_some_and(same) { return; }
        let replace = self.current.as_ref().is_none_or(|current| !Arc::ptr_eq(&current.app_instance, &pane.app_instance)
            || current.mailbox_opening != pane.mailbox_opening || (!same(&current.message) && !native_html_same_message(&current.message, &pane.message))
            || current.access != pane.access || current.picture_cache_generation != pane.picture_cache_generation);
        if replace {
            self.current = None;
            self.failed = None;
            self.next_generation = self.next_generation.saturating_add(1);
            match Self::open(&pane, ctx, frame, self.next_generation) {
                Ok(current) => self.current = Some(current),
                Err(_) => { self.failed = Some(pane.message); return; }
            }
        }
        let Some(current) = &mut self.current else { return };
        current.message = pane.message.clone();
        if current.dark != pane.dark {
            let _ = current.host.hide();
            if current.host.set_dark(pane.dark).is_err() { self.failed = Some(pane.message); self.current = None; return; }
            current.dark = pane.dark;
        }
        if current.host.failed() {
            self.failed = Some(pane.message);
            self.current = None;
            return;
        }
        if current.host.poll().is_err() {
            self.failed = Some(pane.message);
            self.current = None;
            app.native_html_images = None;
            return;
        }
        current.host.set_unknown_image_access(current.host.generation(), pane.unknown_allowed);
        let requests = current.host.image_requests();
        let unknown_count = current.host.unknown_image_count();
        // Only image requests matched by the bounded host-authored dimension inspection enter
        // the existing image-permission path; unclassified requests stay blocked.
        let changed = app.native_html_images.as_ref().is_none_or(|prior| !Arc::ptr_eq(&prior.message, &pane.message) || prior.urls != requests || prior.unknown_count != unknown_count);
        app.native_html_images = Some(NativeHtmlImages { message: pane.message.clone(), app_instance: pane.app_instance.clone(), mailbox_opening: pane.mailbox_opening, urls: requests.clone(), unknown_count });
        if changed { ctx.request_repaint(); }
        let normal = |url: &str| mailcraft_link::WebLink::parse(url).map(mailcraft_link::WebLink::into_string);
        let known: Vec<(String, String)> = pane.remote_urls.iter().filter_map(|url| normal(url).map(|n| (n, url.to_string()))).collect();
        let mut ready = Vec::new();
        for (canonical, original) in &known {
            if requests.contains(canonical) && !current.supplied.contains(canonical) {
                ready.push((canonical.clone(), original.clone()));
            }
        }
        let originals: Vec<String> = ready.iter().map(|(_, original)| original.clone()).collect();
        for (original, bytes) in app.native_ready_pictures(ctx, &pane.message, &originals) {
            if let Some((canonical, _)) = ready.iter().find(|(_, name)| *name == original)
                && current.host.add_image(current.host.generation(), canonical, bytes)
            {
                current.supplied.insert(canonical.clone());
                current.dirty = true;
            }
        }
        if current.dirty && pane.pictures_waiting == 0 && !current.host.refreshing() {
            current.dirty = false;
            if current.host.refresh().is_err() { self.failed = Some(pane.message); self.current = None; return; }
        }
        for url in current.host.take_links() {
            if let Some(url) = crate::browser::normalised(&url) {
                ctx.open_url(egui::OpenUrl::new_tab(url));
            }
        }
        if current.host.set_bounds(bounds(pane.rect, ctx.pixels_per_point())).is_err() {
            self.failed = Some(pane.message);
            self.current = None;
            return;
        }
        let ready = current.host.ready();
        if current.host.set_visible((current.host.ready() || current.host.refreshing()) && current.host.theme_ready()).is_err() { self.failed = Some(pane.message); self.current = None; return; }
        ctx.request_repaint_after(std::time::Duration::from_millis(if !ready || pane.pictures_waiting > 0 { 100 } else { 500 }));
    }

    fn open(pane: &NativeHtmlPane, ctx: &egui::Context, frame: &eframe::Frame, generation: u64) -> Result<Current, ()> {
        let window = frame.winit_window().cloned().ok_or(())?;
        let html = pane.message.body.html.as_deref().ok_or(())?;
        let scope = generation.to_string();
        let transformed = mailcraft_html::webview_resources::transform(html, &scope, |cid| MailApp::native_cid_picture(&pane.message, cid)).map_err(|_| ())?;
        let dir = std::env::temp_dir().join("MailCraft").join(format!("WebView2-{}", std::process::id()));
        let dir = dir.to_str().ok_or(())?;
        let wake = ctx.clone();
        let host = Host::new(window, dir, generation, move || wake.request_repaint()).map_err(|_| ())?;
        host.set_dark(pane.dark).map_err(|_| ())?;
        for resource in transformed.resources {
            if MailApp::native_picture_valid(&resource.bytes) {
                let _ = host.add_embedded_image(generation, &resource.url, resource.bytes);
            }
        }
        host.set_image_access(generation, pane.access == NativePictureAccess::Allowed);
        host.set_unknown_image_access(generation, pane.unknown_allowed);
        host.show(&transformed.html, bounds(pane.rect, ctx.pixels_per_point())).map_err(|_| ())?;
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        Ok(Current { message: pane.message.clone(), app_instance: pane.app_instance.clone(), mailbox_opening: pane.mailbox_opening, host, access: pane.access, supplied: HashSet::new(), dirty: false, picture_cache_generation: pane.picture_cache_generation, dark: pane.dark })
    }
}

fn bounds(rect: egui::Rect, scale: f32) -> [i32; 4] {
    [(rect.min.x * scale).round() as i32, (rect.min.y * scale).round() as i32, (rect.max.x * scale).round() as i32, (rect.max.y * scale).round() as i32]
}
