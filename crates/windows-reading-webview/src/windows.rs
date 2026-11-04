// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! The only code that crosses from winit's borrowed HWND to WebView2's native API.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use crate::policy::Images;
use windows_webview::{Color, Controller, ControllerOptions, Environment, EnvironmentOptions, EventRegistration, PermissionState, WebResourceResponse, WebView};

const DOCUMENT: &str = "https://mailcraft.invalid/reading/document";
const CSP: &str = "default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src http: https:; font-src 'none'; connect-src 'none'; frame-src 'none'; object-src 'none'; media-src 'none'; base-uri 'none'; form-action 'none'; sandbox allow-same-origin allow-popups allow-scripts";
const VISIBLE_IMAGES: &str = include_str!("visible_images.js");
const SCROLL_POSITION: &str = "[window.scrollX,window.scrollY,document.documentElement.clientWidth,document.documentElement.clientHeight]";

/// A live child of the application's window. It must be dropped before that window.
pub struct Host {
    controller: Controller,
    view: WebView,
    html: Rc<RefCell<Vec<u8>>>,
    allow_document: Rc<RefCell<bool>>,
    links: Rc<RefCell<Vec<String>>>,
    images: Rc<RefCell<Images>>,
    generation: u64,
    shown: std::cell::Cell<bool>,
    navigation_started: std::cell::Cell<bool>,
    status: Rc<std::cell::Cell<i8>>,
    refreshing: Rc<std::cell::Cell<bool>>,
    document_epoch: Rc<std::cell::Cell<u64>>,
    theme_failed: Rc<std::cell::Cell<bool>>,
    theme_generation: Rc<std::cell::Cell<u64>>,
    theme_pending: Rc<std::cell::Cell<u8>>,
    inspected: Rc<std::cell::Cell<bool>>,
    inspection_pending: Rc<std::cell::Cell<bool>>,
    need_inspection: Rc<std::cell::Cell<bool>>,
    probe_pending: Rc<std::cell::Cell<bool>>,
    last_scroll: Rc<RefCell<Option<[f64; 4]>>>,
    last_probe: std::cell::Cell<Option<std::time::Instant>>,
    restore_scroll: Rc<std::cell::Cell<Option<(f64, f64)>>>,
    wake: Rc<dyn Fn()>,
    _handlers: Vec<EventRegistration>,
    _window: Arc<dyn HasWindowHandle + Send + Sync>,
}

impl Host {
    /// Keep the winit window alive for as long as its child exists. Call on its UI thread.
    pub fn new(window: Arc<dyn HasWindowHandle + Send + Sync>, data_dir: &str, generation: u64, wake: impl Fn() + 'static) -> Result<Self, String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else { return Err("not a Windows window".into()); };
        let hwnd = handle.hwnd.get();
        if data_dir.is_empty() { return Err("no WebView2 data directory".into()); }
        let options = EnvironmentOptions::new().user_data_folder(data_dir);
        let env = Environment::with_options(&options).map_err(|e| e.to_string())?;
        let opts = ControllerOptions::new().in_private_mode(true);
        // The caller supplies winit's live HWND on its UI thread; the app owns Host and drops it
        // before the winit window. No other code constructs or retains this borrowed handle.
        let controller = unsafe { env.create_controller_with_options_for_hwnd(hwnd as *mut _, &opts) }.map_err(|e| e.to_string())?;
        controller.set_visible(false).map_err(|e| e.to_string())?;
        controller.set_allow_external_drop(false).map_err(|e| e.to_string())?;
        let view = controller.webview().map_err(|e| e.to_string())?;
        let settings = view.settings().map_err(|e| e.to_string())?;
        settings.set_script_enabled(false).map_err(|e| e.to_string())?;
        settings.set_web_message_enabled(false).map_err(|e| e.to_string())?;
        settings.set_host_objects_allowed(false).map_err(|e| e.to_string())?;
        settings.set_dev_tools_enabled(false).map_err(|e| e.to_string())?;
        settings.set_default_script_dialogs_enabled(false).map_err(|e| e.to_string())?;
        settings.set_status_bar_enabled(false).map_err(|e| e.to_string())?;
        settings.set_general_autofill_enabled(false).map_err(|e| e.to_string())?;
        settings.set_password_autosave_enabled(false).map_err(|e| e.to_string())?;
        let html = Rc::new(RefCell::new(Vec::<u8>::new()));
        let allow_document = Rc::new(RefCell::new(false));
        let links = Rc::new(RefCell::new(Vec::<String>::new()));
        let status = Rc::new(std::cell::Cell::new(0i8));
        let refreshing = Rc::new(std::cell::Cell::new(false));
        let document_epoch = Rc::new(std::cell::Cell::new(0u64));
        let inspected = Rc::new(std::cell::Cell::new(false));
        let inspection_pending = Rc::new(std::cell::Cell::new(false));
        let need_inspection = Rc::new(std::cell::Cell::new(true));
        let probe_pending = Rc::new(std::cell::Cell::new(false));
        let last_scroll = Rc::new(RefCell::new(None));
        let restore_scroll = Rc::new(std::cell::Cell::new(None));
        let wake: Rc<dyn Fn()> = Rc::new(wake);
        let images = Rc::new(RefCell::new(Images::default()));
        images.borrow_mut().reset(generation);
        let body = html.clone();
        let pending_images = images.clone();
        let wake_resource = wake.clone();
        let resource = view.on_web_resource_requested("*", move |request| {
            let uri = request.uri();
            if request.is_document() && request.method() == "GET" && uri == DOCUMENT {
                return Some(WebResourceResponse::new(body.borrow().clone()).content_type("text/html; charset=utf-8")
                    .header("Content-Security-Policy", CSP).header("Cache-Control", "no-store").header("X-Content-Type-Options", "nosniff"));
            }
            let is_image = request.method() == "GET" && request.is_image();
            let answer = pending_images.borrow_mut().request(&uri, is_image);
            if is_image { wake_resource(); }
            if let Some((mime, bytes)) = answer {
                return Some(WebResourceResponse::new(bytes.as_ref().clone()).content_type(mime).header("Cache-Control", "no-store").header("X-Content-Type-Options", "nosniff"));
            }
            Some(WebResourceResponse::new(Vec::new()).status(403, "Forbidden").header("Cache-Control", "no-store"))
        }).map_err(|e| e.to_string())?;
        let may_load = allow_document.clone();
        let pending_links = links.clone();
        let wake_link = wake.clone();
        let navigation = view.on_navigation_starting(move |event| {
            let uri = event.uri();
            if uri == DOCUMENT && std::mem::take(&mut *may_load.borrow_mut()) { return; }
            let _ = event.set_cancel(true);
            if event.is_user_initiated() && pending_links.borrow().len() < 16 { pending_links.borrow_mut().push(uri); wake_link(); }
        }).map_err(|e| e.to_string())?;
        let pending_links = links.clone();
        let wake_link = wake.clone();
        let new_window = view.on_new_window_requested(move |event| {
            let _ = event.set_handled(true);
            if event.is_user_initiated() && pending_links.borrow().len() < 16 {
                pending_links.borrow_mut().push(event.uri());
                wake_link();
            }
        }).map_err(|e| e.to_string())?;
        let download = view.on_download_starting(|event| { let _ = event.set_cancel(true); }).map_err(|e| e.to_string())?;
        let permission = view.on_permission_requested(|event| { let _ = event.set_state(PermissionState::Deny); }).map_err(|e| e.to_string())?;
        let navigation_status = status.clone();
        let wake_status = wake.clone();
        let need_after_load = need_inspection.clone();
        let inspected_after_load = inspected.clone();
        let restore_after_load = restore_scroll.clone();
        let view_after_load = view.clone();
        let completed = view.on_navigation_completed(move |event| {
            navigation_status.set(if event.is_success() { 1 } else { -1 });
            inspected_after_load.set(false);
            need_after_load.set(true);
            if event.is_success() && let Some((x, y)) = restore_after_load.take() {
                // The numbers were parsed and clamped by the host, never interpolated from HTML.
                let script = format!("window.scrollTo({x},{y})");
                let _ = view_after_load.execute_script(&script, |_| {});
            }
            wake_status();
        }).map_err(|e| e.to_string())?;
        let process_status = status.clone();
        let wake_failure = wake.clone();
        let failed = view.on_process_failed(move |_| { process_status.set(-1); wake_failure(); }).map_err(|e| e.to_string())?;
        Ok(Self { controller, view, html, allow_document, links, images, generation, shown: std::cell::Cell::new(false), navigation_started: std::cell::Cell::new(false), status, refreshing, document_epoch, theme_failed: Rc::new(std::cell::Cell::new(false)), theme_generation: Rc::new(std::cell::Cell::new(0)), theme_pending: Rc::new(std::cell::Cell::new(0)), inspected, inspection_pending, need_inspection, probe_pending, last_scroll, last_probe: std::cell::Cell::new(None), restore_scroll, wake, _handlers: vec![resource, navigation, new_window, download, permission, completed, failed], _window: window })
    }

    pub fn show(&self, html: &str, bounds: [i32; 4]) -> Result<(), String> {
        if self.shown.get() || html.len() > 2 * 1024 * 1024 || bounds[2] <= bounds[0] || bounds[3] <= bounds[1] { return Err("invalid document or bounds".into()); }
        *self.html.borrow_mut() = html.as_bytes().to_vec();
        self.controller.set_bounds(bounds[0], bounds[1], bounds[2], bounds[3]).map_err(|e| e.to_string())?;
        self.shown.set(true);
        Ok(())
    }

    pub fn hide(&self) -> Result<(), String> { self.controller.set_visible(false).map_err(|e| e.to_string()) }
    pub fn set_bounds(&self, bounds: [i32; 4]) -> Result<(), String> {
        if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] { return Err("invalid bounds".into()); }
        self.controller.set_bounds(bounds[0], bounds[1], bounds[2], bounds[3]).map_err(|e| e.to_string())
    }
    pub fn set_visible(&self, visible: bool) -> Result<(), String> { self.controller.set_visible(visible).map_err(|e| e.to_string()) }
    pub fn ready(&self) -> bool { self.status.get() == 1 && self.inspected.get() }
    /// A reload of this same message may remain visible while it is re-inspected.
    pub fn refreshing(&self) -> bool { self.refreshing.get() }
    pub fn loaded(&self) -> bool { self.status.get() == 1 }
    pub fn theme_ready(&self) -> bool { self.theme_pending.get() == 0 && !self.theme_failed.get() }
    #[cfg(debug_assertions)]
    pub fn smoke_capture(&self, callback: impl FnOnce(Result<String, String>) + 'static) -> Result<(), String> {
        self.view.call_dev_tools_protocol_method("Page.captureScreenshot", r#"{"format":"png"}"#, move |result| callback(result.map_err(|e| e.to_string()))).map_err(|e| e.to_string())
    }
    #[cfg(debug_assertions)]
    pub fn smoke_eval(&self, script: &str, callback: impl FnOnce(Result<String, String>) + 'static) -> Result<(), String> {
        self.view.execute_script(script, move |result| callback(result.map_err(|e| e.to_string()))).map_err(|e| e.to_string())
    }
    pub fn failed(&self) -> bool { self.status.get() < 0 || self.theme_failed.get() }
    /// Re-request after validated images become available; no browser-side network is allowed.
    pub fn refresh(&self) -> Result<(), String> {
        self.refreshing.set(true);
        self.document_epoch.set(self.document_epoch.get().wrapping_add(1));
        self.inspection_pending.set(false);
        self.probe_pending.set(false);
        self.status.set(0);
        self.inspected.set(false);
        let view = self.view.clone();
        let allow = self.allow_document.clone();
        let restore = self.restore_scroll.clone();
        let status = self.status.clone();
        let inspected = self.inspected.clone();
        let wake = self.wake.clone();
        let epoch = self.document_epoch.get();
        let current = self.document_epoch.clone();
        self.view.execute_script(SCROLL_POSITION, move |result| {
            if current.get() != epoch { return; }
            let position = result.ok().and_then(|s| serde_json::from_str::<[f64; 4]>(&s).ok());
            let Some([x, y, _, _]) = position.filter(|a| a.iter().all(|v| v.is_finite())) else { status.set(-1); wake(); return; };
            restore.set(Some((x.clamp(-1_000_000.0, 1_000_000.0), y.clamp(-1_000_000.0, 1_000_000.0))));
            inspected.set(false);
            *allow.borrow_mut() = true;
            if view.reload().is_err() { status.set(-1); }
            wake();
        }).map_err(|e| e.to_string())
    }
    pub fn take_links(&self) -> Vec<String> { std::mem::take(&mut *self.links.borrow_mut()) }
    pub fn image_requests(&self) -> Vec<String> { self.images.borrow().discovered() }
    pub fn unknown_image_count(&self) -> usize { self.images.borrow().unknown_count() }
    pub fn set_image_access(&self, generation: u64, allowed: bool) { self.images.borrow_mut().set_allowed(generation, allowed); }
    pub fn set_unknown_image_access(&self, generation: u64, allowed: bool) { self.images.borrow_mut().set_unknown_allowed(generation, allowed); }
    pub fn add_image(&self, generation: u64, url: &str, bytes: Arc<Vec<u8>>) -> bool { self.images.borrow_mut().put(generation, url, bytes) }
    pub fn add_embedded_image(&self, generation: u64, url: &str, bytes: Arc<Vec<u8>>) -> bool { self.images.borrow_mut().put_local(generation, url, bytes) }
    pub fn generation(&self) -> u64 { self.generation }

    /// Ask Chromium to adapt bright authored colors while preserving layout and image content.
    /// Sender dark-mode media queries follow MailCraft's effective message-body theme.
    pub fn set_dark(&self, dark: bool) -> Result<(), String> {
        let value = if dark { 0x14 } else { 0xff };
        self.controller.set_default_background_color(Color { r: value, g: value, b: value, a: 255 }).map_err(|e| e.to_string())?;
        let generation = self.theme_generation.get().wrapping_add(1);
        self.theme_generation.set(generation);
        self.theme_pending.set(2);
        let auto = if dark { r#"{"enabled":true}"# } else { r#"{"enabled":false}"# };
        let media = if dark { r#"{"features":[{"name":"prefers-color-scheme","value":"dark"}]}"# } else { r#"{"features":[{"name":"prefers-color-scheme","value":"light"}]}"# };
        let failed = self.theme_failed.clone();
        let wake = self.wake.clone();
        let latest = self.theme_generation.clone();
        let pending = self.theme_pending.clone();
        self.view.call_dev_tools_protocol_method("Emulation.setAutoDarkModeOverride", auto, move |result| { if latest.get() == generation { if result.is_err() { failed.set(true); } pending.set(pending.get().saturating_sub(1)); wake(); } }).map_err(|e| e.to_string())?;
        let failed = self.theme_failed.clone();
        let wake = self.wake.clone();
        let inspect = self.need_inspection.clone();
        let latest = self.theme_generation.clone();
        let pending = self.theme_pending.clone();
        self.view.call_dev_tools_protocol_method("Emulation.setEmulatedMedia", media, move |result| { if latest.get() == generation { if result.is_err() { failed.set(true); } else { inspect.set(true); } pending.set(pending.get().saturating_sub(1)); wake(); } }).map_err(|e| e.to_string())
    }

    /// Reclassify on initial load and when the browser's scroll position or viewport changes.
    pub fn poll(&self) -> Result<(), String> {
        if self.shown.get() && !self.navigation_started.get() && self.theme_ready() {
            *self.allow_document.borrow_mut() = true;
            self.view.navigate(DOCUMENT).map_err(|e| e.to_string())?;
            self.navigation_started.set(true);
        }
        if self.status.get() != 1 || self.view.source() != DOCUMENT { return Ok(()); }
        if self.need_inspection.get() && !self.inspection_pending.get() {
            self.need_inspection.set(false);
            self.inspection_pending.set(true);
            let images = self.images.clone();
            let status = self.status.clone();
            let inspected = self.inspected.clone();
            let pending = self.inspection_pending.clone();
            let refreshing = self.refreshing.clone();
            let wake = self.wake.clone();
            let generation = self.generation;
            let epoch = self.document_epoch.get();
            let current = self.document_epoch.clone();
            if let Err(e) = self.view.execute_script(VISIBLE_IMAGES, move |result| {
                if current.get() != epoch { return; }
                pending.set(false);
                match result.ok().and_then(|s| crate::inspection::urls(&s)) {
                    Some(urls) => { images.borrow_mut().classify(generation, urls.known, urls.unknown); inspected.set(true); refreshing.set(false); }
                    None => status.set(-1),
                }
                wake();
            }) { self.inspection_pending.set(false); return Err(e.to_string()); }
        }
        let due = self.last_probe.get().is_none_or(|last| last.elapsed() >= std::time::Duration::from_millis(500));
        if due && !self.probe_pending.replace(true) {
            self.last_probe.set(Some(std::time::Instant::now()));
            let pending = self.probe_pending.clone();
            let prior = self.last_scroll.clone();
            let needs = self.need_inspection.clone();
            let status = self.status.clone();
            let wake = self.wake.clone();
            let epoch = self.document_epoch.get();
            let current = self.document_epoch.clone();
            if let Err(e) = self.view.execute_script(SCROLL_POSITION, move |result| {
                if current.get() != epoch { return; }
                pending.set(false);
                let position = result.ok().and_then(|s| serde_json::from_str::<[f64; 4]>(&s).ok()).filter(|v| v.iter().all(|n| n.is_finite()));
                if let Some(position) = position {
                    if prior.borrow().as_ref() != Some(&position) { *prior.borrow_mut() = Some(position); needs.set(true); }
                } else { status.set(-1); }
                wake();
            }) { self.probe_pending.set(false); return Err(e.to_string()); }
        }
        Ok(())
    }
}

impl Drop for Host {
    fn drop(&mut self) { let _ = self.controller.close(); }
}
