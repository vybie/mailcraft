use super::*;
const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00";
const DATA: &str = "data:image/gif;base64,R0lGODlhAQABAA==";
#[test]
fn preserves_markup_and_resolves_pictures_and_css() {
    let html = format!(
        "<!doctype html><!-- &copy; --><style>.a{{background:url('cid:one')}} /* url(cid:ignored) */</style><table><tr><td background=cid:two style=\"color:red;background:url({DATA})\"><img alt='&copy;' src='cid:one'></td></tr></table>"
    );
    let result = transform(&html, "m42", |id| (id == "cid:one" || id == "cid:two").then(|| GIF.to_vec())).unwrap();
    assert_eq!(result.resources.len(), 4);
    assert!(result.html.contains("<!-- &copy; -->"));
    assert!(result.html.contains("/* url(cid:ignored) */"));
    assert!(result.html.contains("<table><tr><td background=https://mailcraft.invalid/reading/asset/m42/"));
    assert!(result.html.contains("alt='&copy;'"));
    assert!(result.resources.iter().all(|r| r.mime == "image/gif" && r.bytes.as_slice() == GIF));
}
#[test]
fn refuses_active_or_missing_content_and_bad_scope() {
    let html = "<img src='cid:svg'><img src='data:image/svg+xml;base64,PHN2Zz4='><img src='file:///secret'><img src='cid:missing'>";
    let out = transform(html, "one", |_| Some(b"<svg><script>x</script></svg>".to_vec())).unwrap();
    assert_eq!(out.html, html);
    assert!(out.resources.is_empty());
    assert!(matches!(transform(html, "../bad", |_| None), Err(TransformError::InvalidScope)));
}
#[test]
fn bounds_and_failed_data_are_not_rewritten() {
    let input = format!("{}{}", "<img src='data:image/gif;base64,R0lGODlhAQABAA=='>".repeat(33), "<img src='data:image/gif;base64,AA=='>");
    let out = transform(&input, "unique", |_| None).unwrap();
    assert_eq!(out.resources.len(), 32);
    assert!(out.html.contains(DATA));
    assert!(matches!(transform(&"x".repeat(crate::MAX_INPUT + data::MAX_DATA_TEXT + 1), "x", |_| None), Err(TransformError::InputTooLarge)));
}
#[test]
fn does_not_treat_css_strings_as_url_functions() {
    let html = "<style>.x{content:'url(cid:x)';background:url(\"cid:x\")}</style>";
    let out = transform(html, "x", |_| Some(GIF.to_vec())).unwrap();
    assert_eq!(out.resources.len(), 1);
    assert!(out.html.contains("content:'url(cid:x)'"));
}

#[test]
fn srcset_candidates_keep_descriptors_and_commas() {
    let html = format!("<picture><source srcset='cid:one 1x, {DATA} 2x'><img srcset=\"cid:two 300w, cid:missing 600w\"></picture>");
    let out = transform(&html, "scope7", |id| (id == "cid:one" || id == "cid:two").then(|| GIF.to_vec())).unwrap();
    assert_eq!(out.resources.len(), 3);
    assert!(out.html.contains("/reading/asset/scope7/0 1x, https://mailcraft.invalid/reading/asset/scope7/1 2x"));
    assert!(out.html.contains("/reading/asset/scope7/2 300w, cid:missing 600w"));
}

proptest::proptest! {
    #[test]
    fn hostile_markup_does_not_panic(html in ".{0,2048}") {
        let _ = transform(&html, "scope", |_| None);
    }
}
