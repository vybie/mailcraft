// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

#[test]
fn rejects_malformed_or_tiny_classification_results() {
    let valid = r#"{"version":1,"scanned":1,"truncated":false,"images":[{"url":"https://example.org/a","width":4,"height":4,"kind":"background"}]}"#;
    assert_eq!(urls(valid).unwrap().known, ["https://example.org/a"]);
    assert!(urls(&valid.replace("\"width\":4", "\"width\":1")).is_none());
    assert!(urls(&valid.replace("\"version\":1", "\"version\":2")).is_none());
    assert!(urls("not json").is_none());
}

#[test]
fn unknown_size_is_separate_and_cannot_be_spoofed_as_visible() {
    let input = r#"{"version":1,"scanned":1,"truncated":false,"images":[{"url":"https://example.org/a","width":0,"height":0,"kind":"unknown"}]}"#;
    let classified = urls(input).unwrap();
    assert!(classified.known.is_empty());
    assert_eq!(classified.unknown, ["https://example.org/a"]);
    assert!(urls(&input.replace("\"kind\":\"unknown\"", "\"kind\":\"image\"")).is_none());
}
