// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! The bounded, message-generation-specific image handoff. A request is never a network fetch.

use std::collections::HashMap;
use std::sync::Arc;

const MAX_IMAGES: usize = 60;
const MAX_DISCOVERED: usize = 512;
const MAX_URL: usize = 8192;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 24 * 1024 * 1024;
// CID attachments use the existing 200-picture, 24 MiB-per-picture limits. Data-URI
// pictures additionally pass their separate 32-picture/40 MiB budget in the transform.
const MAX_LOCAL_IMAGES: usize = 200;
const MAX_LOCAL_IMAGE_BYTES: usize = 24 * 1024 * 1024;
const MAX_LOCAL_TOTAL_BYTES: usize = MAX_LOCAL_IMAGES * MAX_LOCAL_IMAGE_BYTES;

#[derive(Default)]
pub(crate) struct Images {
    generation: u64,
    allowed: bool,
    discovered: Vec<String>,
    eligible: Vec<String>,
    ready: HashMap<String, Arc<Vec<u8>>>,
    bytes: usize,
    local: HashMap<String, Arc<Vec<u8>>>,
    local_bytes: usize,
}

impl Images {
    pub(crate) fn reset(&mut self, generation: u64) {
        *self = Self { generation, ..Self::default() };
    }

    pub(crate) fn set_allowed(&mut self, generation: u64, allowed: bool) {
        if generation != self.generation { return; }
        self.allowed = allowed;
        if !allowed { self.ready.clear(); self.bytes = 0; }
    }

    /// Record only browser image requests; return bytes solely after explicit permission.
    pub(crate) fn request(&mut self, url: &str, is_image: bool) -> Option<(&'static str, Arc<Vec<u8>>)> {
        if !is_image || url.len() > MAX_URL || !(url.starts_with("https://") || url.starts_with("http://")) { return None; }
        if url.starts_with("https://mailcraft.invalid/reading/asset/") {
            let bytes = self.local.get(url)?;
            return Some((mime(bytes)?, bytes.clone()));
        }
        if url.starts_with("https://mailcraft.invalid/") { return None; }
        if !self.discovered.iter().any(|known| known == url) {
            if self.discovered.len() >= MAX_DISCOVERED { return None; }
            self.discovered.push(url.to_string());
        }
        if !self.allowed || !self.eligible.iter().any(|known| known == url) { return None; }
        let bytes = self.ready.get(url)?;
        Some((mime(bytes)?, bytes.clone()))
    }

    pub(crate) fn discovered(&self) -> Vec<String> {
        self.eligible.iter().filter(|url| self.discovered.contains(url)).take(MAX_IMAGES).cloned().collect()
    }

    /// Only the current document's validated host inspection may classify resources.
    pub(crate) fn classify(&mut self, generation: u64, urls: Vec<String>) {
        if generation != self.generation { return; }
        self.eligible = urls.into_iter().filter(|url| url.len() <= MAX_URL && (url.starts_with("https://") || url.starts_with("http://"))).take(MAX_IMAGES).collect();
    }

    /// Add a validated embedded raster from this message under its opaque generation URL.
    pub(crate) fn put_local(&mut self, generation: u64, url: &str, bytes: Arc<Vec<u8>>) -> bool {
        let prefix = format!("https://mailcraft.invalid/reading/asset/{generation}/");
        if generation != self.generation || !url.starts_with(&prefix) || url.len() > MAX_URL
            || self.local.contains_key(url) || self.local.len() >= MAX_LOCAL_IMAGES
            || bytes.len() > MAX_LOCAL_IMAGE_BYTES || mime(&bytes).is_none() { return false; }
        let total = self.local_bytes.saturating_add(bytes.len());
        if total > MAX_LOCAL_TOTAL_BYTES { return false; }
        self.local_bytes = total;
        self.local.insert(url.to_string(), bytes);
        true
    }

    /// Bytes must already have passed the mail image validator and download budgets.
    pub(crate) fn put(&mut self, generation: u64, url: &str, bytes: Arc<Vec<u8>>) -> bool {
        if generation != self.generation || !self.allowed || !self.discovered.iter().any(|known| known == url) || !self.eligible.iter().any(|known| known == url)
            || bytes.len() > MAX_IMAGE_BYTES || mime(&bytes).is_none() { return false; }
        let prior = self.ready.get(url).map_or(0, |item| item.len());
        let total = self.bytes.saturating_sub(prior).saturating_add(bytes.len());
        if total > MAX_TOTAL_BYTES { return false; }
        self.bytes = total;
        self.ready.insert(url.to_string(), bytes);
        true
    }
}

fn mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { Some("image/png") }
    else if bytes.starts_with(b"\xff\xd8\xff") { Some("image/jpeg") }
    else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") { Some("image/gif") }
    else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") { Some("image/webp") }
    else { None }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
