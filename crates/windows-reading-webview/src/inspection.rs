// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Validate the fixed host-authored image classification result as untrusted data.

pub(crate) struct Classification { pub known: Vec<String>, pub unknown: Vec<String> }

pub(crate) fn urls(json: &str) -> Option<Classification> {
    if json.len() > 512 * 1024 { return None; }
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value.get("version")?.as_u64()? != 1 || value.get("scanned")?.as_u64()? > 10_000
        || value.get("truncated")?.as_bool().is_none() { return None; }
    let items = value.get("images")?.as_array()?;
    if items.len() > 60 { return None; }
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for item in items {
        let url = item.get("url")?.as_str()?;
        let width = item.get("width")?.as_f64()?;
        let height = item.get("height")?.as_f64()?;
        let kind = item.get("kind")?.as_str()?;
        if url.len() > 8192 || !width.is_finite() || !height.is_finite() { return None; }
        let target = match kind {
            "image" | "background" if width > 2.0 && height > 2.0 => &mut known,
            "unknown" if width == 0.0 && height == 0.0 => &mut unknown,
            _ => return None,
        };
        if (url.starts_with("https://") || url.starts_with("http://")) && !target.iter().any(|v| v == url) {
            target.push(url.to_string());
        }
    }
    Some(Classification { known, unknown })
}

#[cfg(test)]
#[path = "inspection_tests.rs"]
mod tests;
