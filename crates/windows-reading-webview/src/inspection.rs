// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Validate the fixed host-authored image classification result as untrusted data.

pub(crate) fn urls(json: &str) -> Option<Vec<String>> {
    if json.len() > 512 * 1024 { return None; }
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value.get("version")?.as_u64()? != 1 || value.get("scanned")?.as_u64()? > 10_000
        || value.get("truncated")?.as_bool().is_none() { return None; }
    let items = value.get("images")?.as_array()?;
    if items.len() > 60 { return None; }
    let mut result = Vec::new();
    for item in items {
        let url = item.get("url")?.as_str()?;
        let width = item.get("width")?.as_f64()?;
        let height = item.get("height")?.as_f64()?;
        let kind = item.get("kind")?.as_str()?;
        let dimensions_ok = match kind {
            "image" | "background" => width > 2.0 && height > 2.0,
            "collapsed-image" => width == 0.0 && height == 0.0,
            _ => false,
        };
        if url.len() > 8192 || !width.is_finite() || !height.is_finite() || !dimensions_ok { return None; }
        if (url.starts_with("https://") || url.starts_with("http://")) && !result.iter().any(|v| v == url) {
            result.push(url.to_string());
        }
    }
    Some(result)
}

#[cfg(test)]
#[path = "inspection_tests.rs"]
mod tests;
