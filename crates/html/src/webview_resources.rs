// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Extract local raster pictures for a browser host. This is not an HTML sanitizer: the host
//! must block all other loads, scripts, navigation and active content independently.
use crate::{
    data, entities,
    tokenizer::{Token, Tokenizer},
};
use std::sync::Arc;

/// Whether a resource is an attached picture or data embedded directly in the HTML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Cid,
    Data,
}

/// A local, message-scoped response. The host must validate image dimensions before serving it.
#[derive(Debug, Clone)]
pub struct Resource {
    pub url: String,
    pub kind: ResourceKind,
    pub mime: &'static str,
    pub bytes: Arc<Vec<u8>>,
}
/// Original markup, with recognized local picture URLs replaced, and the responses to serve.
#[derive(Debug)]
pub struct Transformed {
    pub html: String,
    pub resources: Vec<Resource>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformError {
    InvalidScope,
    InputTooLarge,
}

/// Extract CID and data pictures in image `src`, `background`, inline style and style blocks.
/// `scope` must be unique per document load; the caller resolves CID only within that message,
/// returning the attachment's existing shared bytes without copying them.
/// Unknown or malformed syntax is left intact and must be blocked by the host's default-deny policy.
pub fn transform(html: &str, scope: &str, mut resolve_cid: impl FnMut(&str) -> Option<Arc<Vec<u8>>>) -> Result<Transformed, TransformError> {
    if scope.is_empty() || scope.len() > 64 || !scope.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(TransformError::InvalidScope);
    }
    if html.len() > crate::MAX_INPUT + data::MAX_DATA_TEXT {
        return Err(TransformError::InputTooLarge);
    }
    let mut ctx = Extract {
        html,
        scope,
        resolve_cid: &mut resolve_cid,
        data_budget: data::Budget::default(),
        cid_count: 0,
        resources: Vec::new(),
        edits: Vec::new(),
    };
    let mut tokens = Tokenizer::new(html);
    while let Some(token) = tokens.next() {
        if let Token::Open(tag) = token {
            if tag.name == "img" {
                if let Some(src) = tag.raw_attr("src") {
                    ctx.url(src);
                }
            }
            if tag.name == "img" || tag.name == "source" {
                if let Some(srcset) = tag.raw_attr("srcset") {
                    ctx.srcset(srcset);
                }
            }
            if let Some(background) = tag.raw_attr("background") {
                ctx.url(background);
            }
            if let Some(style) = tag.raw_attr("style") {
                ctx.css(style);
            }
        }
        for (_, style) in tokens.styles.drain(..) {
            ctx.css(style);
        }
    }
    for (_, style) in tokens.styles.drain(..) {
        ctx.css(style);
    }
    ctx.edits.sort_by_key(|(start, _, _)| *start);
    let mut output = String::with_capacity(html.len());
    let mut at = 0;
    for (start, end, url) in ctx.edits {
        if start < at || end > html.len() {
            continue;
        }
        output.push_str(&html[at..start]);
        output.push_str(&url);
        at = end;
    }
    output.push_str(&html[at..]);
    Ok(Transformed { html: output, resources: ctx.resources })
}

struct Extract<'a, F> {
    html: &'a str,
    scope: &'a str,
    resolve_cid: &'a mut F,
    data_budget: data::Budget,
    cid_count: usize,
    resources: Vec<Resource>,
    edits: Vec<(usize, usize, String)>,
}
impl<F: FnMut(&str) -> Option<Arc<Vec<u8>>>> Extract<'_, F> {
    fn url(&mut self, raw: &str) {
        let decoded = entities::unescape(raw);
        let src = decoded.trim();
        let (kind, bytes) = if data::is_data(src) {
            (ResourceKind::Data, data::picture(src, true, &mut self.data_budget).1.map(|p| p.0))
        } else if src.get(..4).is_some_and(|s| s.eq_ignore_ascii_case("cid:")) {
            if self.cid_count >= 200 {
                return;
            }
            (ResourceKind::Cid, (self.resolve_cid)(src))
        } else {
            return;
        };
        let Some(bytes) = bytes else { return };
        // The reading pane accepts attached pictures up to 24 MiB each. Embedded data uses
        // its own limits in data::picture; neither kind consumes the other's budget.
        if kind == ResourceKind::Cid && bytes.len() > 24 * 1024 * 1024 {
            return;
        }
        let Some(mime) = raster_mime(&bytes) else { return };
        let base = self.html.as_ptr() as usize;
        let Some(start) = (raw.as_ptr() as usize).checked_sub(base) else { return };
        let Some(end) = start.checked_add(raw.len()) else { return };
        if self.html.get(start..end) != Some(raw) {
            return;
        }
        let url = format!("https://mailcraft.invalid/reading/asset/{}/{}", self.scope, self.resources.len());
        if kind == ResourceKind::Cid {
            self.cid_count += 1;
        }
        self.resources.push(Resource { url: url.clone(), kind, mime, bytes });
        self.edits.push((start, end, url));
    }
    /// Parse srcset candidates as URLs followed by descriptors, preserving separators.
    fn srcset(&mut self, srcset: &str) {
        let b = srcset.as_bytes();
        let mut i = 0;
        while i < b.len() {
            while b.get(i).is_some_and(|c| c.is_ascii_whitespace() || *c == b',') {
                i += 1;
            }
            let start = i;
            while b.get(i).is_some_and(|c| !c.is_ascii_whitespace()) {
                i += 1;
            }
            let mut end = i;
            while end > start && b.get(end - 1) == Some(&b',') {
                end -= 1;
            }
            if let Some(url) = srcset.get(start..end) {
                if !url.is_empty() {
                    self.url(url);
                }
            }
            if end < i {
                continue;
            }
            // Descriptor parsing: a comma outside parentheses starts the next candidate.
            let mut depth = 0usize;
            while let Some(&c) = b.get(i) {
                i += 1;
                match c {
                    b'(' => depth = depth.saturating_add(1),
                    b')' => depth = depth.saturating_sub(1),
                    b',' if depth == 0 => break,
                    _ => {}
                }
            }
        }
    }
    /// Conservative CSS URL scanner: comments and quoted strings are skipped; escaped and
    /// malformed URL spellings remain untouched for the host to block.
    fn css(&mut self, css: &str) {
        let b = css.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b.get(i..i + 2) == Some(b"/*") {
                i += 2;
                while i + 1 < b.len() && b.get(i..i + 2) != Some(b"*/") {
                    i += 1;
                }
                i = (i + 2).min(b.len());
                continue;
            }
            if matches!(b[i], b'\'' | b'"') {
                let q = b[i];
                i += 1;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i = (i + 2).min(b.len());
                    } else if b[i] == q {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                continue;
            }
            if b.get(i..i + 3).is_some_and(|x| x.eq_ignore_ascii_case(b"url"))
                && (i == 0 || !b[i - 1].is_ascii_alphanumeric() && b[i - 1] != b'-' && b[i - 1] != b'_')
            {
                let mut j = i + 3;
                while b.get(j).is_some_and(u8::is_ascii_whitespace) {
                    j += 1;
                }
                if b.get(j) == Some(&b'(') {
                    j += 1;
                    while b.get(j).is_some_and(u8::is_ascii_whitespace) {
                        j += 1;
                    }
                    let quote = match b.get(j) {
                        Some(b'\'' | b'"') => {
                            let q = b[j];
                            j += 1;
                            Some(q)
                        }
                        _ => None,
                    };
                    let start = j;
                    while let Some(&c) = b.get(j) {
                        if c == b'\\' || quote.is_some_and(|q| c == q) || quote.is_none() && (c == b')' || c.is_ascii_whitespace()) {
                            break;
                        }
                        j += 1;
                    }
                    let end = j;
                    if b.get(j) == Some(&b'\\') {
                        i = j + 1;
                        continue;
                    }
                    if let Some(q) = quote {
                        if b.get(j) != Some(&q) {
                            i = j;
                            continue;
                        }
                        j += 1;
                    }
                    while b.get(j).is_some_and(u8::is_ascii_whitespace) {
                        j += 1;
                    }
                    if b.get(j) == Some(&b')') {
                        if let Some(value) = css.get(start..end) {
                            self.url(value);
                        }
                        i = j + 1;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }
}
fn raster_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.get(..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(test)]
#[path = "webview_resources_tests.rs"]
mod tests;
