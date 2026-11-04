// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

#[test]
fn resources_require_permission_discovery_and_the_current_message_generation() {
    let mut images = Images::default();
    images.reset(1);
    let url = "https://image.example/a.png";
    let png = Arc::new(b"\x89PNG\r\n\x1a\ncontent".to_vec());
    assert!(images.request(url, true).is_none());
    assert!(!images.put(1, url, png.clone()));
    images.set_allowed(1, true);
    images.classify(1, vec![url.to_string()], vec![]);
    assert!(!images.put(2, url, png.clone()));
    assert!(!images.put(1, "https://other.example/a", png.clone()));
    assert!(images.put(1, url, png));
    assert!(images.request(url, true).is_some());
    images.set_allowed(1, false);
    assert!(images.request(url, true).is_none());
    images.reset(2);
    images.set_allowed(1, true);
    assert!(images.request(url, true).is_none());
}

#[test]
fn non_images_and_non_http_urls_are_never_discovered_or_served() {
    let mut images = Images::default();
    images.reset(1);
    for url in ["file:///private", "data:image/svg+xml,x", "https://example.org/font.woff"] {
        assert!(images.request(url, false).is_none());
    }
    assert!(images.discovered().is_empty());
    images.set_allowed(1, true);
    assert!(images.request("https://example.org/image", true).is_none());
    assert!(!images.put(1, "https://example.org/image", Arc::new(b"<svg/>".to_vec())));
}

#[test]
fn embedded_images_are_bounded_and_scoped_to_one_generation() {
    let mut images = Images::default();
    images.reset(7);
    let url = "https://mailcraft.invalid/reading/asset/7/0";
    let png = Arc::new(b"\x89PNG\r\n\x1a\ncontent".to_vec());
    assert!(!images.put_local(8, url, png.clone()));
    assert!(!images.put_local(7, "https://mailcraft.invalid/reading/asset/8/0", png.clone()));
    assert!(!images.put_local(7, url, Arc::new(b"<svg/>".to_vec())));
    assert!(images.put_local(7, url, png));
    assert!(images.request(url, true).is_some());
    assert!(images.discovered().is_empty());
    images.reset(8);
    assert!(images.request(url, true).is_none());
}

#[test]
fn unknown_dimensions_need_separate_explicit_permission() {
    let mut images = Images::default();
    let url = "https://image.example/unknown";
    let png = Arc::new(b"\x89PNG\r\n\x1a\ncontent".to_vec());
    images.reset(1);
    images.set_allowed(1, true);
    assert!(images.request(url, true).is_none());
    images.classify(1, vec![], vec![url.into()]);
    assert_eq!(images.unknown_count(), 1);
    assert!(images.discovered().is_empty());
    assert!(!images.put(1, url, png.clone()));
    images.set_unknown_allowed(2, true);
    assert!(!images.put(1, url, png.clone()));
    images.set_unknown_allowed(1, true);
    assert_eq!(images.discovered(), [url]);
    assert!(images.put(1, url, png));
    assert!(images.request(url, true).is_some());
    images.reset(2);
    images.set_allowed(2, true);
    assert!(images.request(url, true).is_none());
}
