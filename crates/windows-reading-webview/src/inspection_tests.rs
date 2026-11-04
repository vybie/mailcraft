// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

#[test]
fn rejects_malformed_or_tiny_classification_results() {
    let valid = r#"{"version":1,"scanned":1,"truncated":false,"images":[{"url":"https://example.org/a","width":4,"height":4,"kind":"background"}]}"#;
    assert_eq!(urls(valid).unwrap(), ["https://example.org/a"]);
    assert!(urls(&valid.replace("\"width\":4", "\"width\":1")).is_none());
    assert!(urls(&valid.replace("\"version\":1", "\"version\":2")).is_none());
    assert!(urls("not json").is_none());
    let collapsed = valid.replace("\"width\":4,\"height\":4,\"kind\":\"background\"", "\"width\":0,\"height\":0,\"kind\":\"collapsed-image\"");
    assert_eq!(urls(&collapsed).unwrap(), ["https://example.org/a"]);
    assert!(urls(&collapsed.replace("\"width\":0", "\"width\":1")).is_none());
}
