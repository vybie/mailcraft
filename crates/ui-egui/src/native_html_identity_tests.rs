// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::native_html_same_message;
use mailcraft_model::{Attachment, Message};

#[test]
fn metadata_changes_keep_identity_but_content_and_cid_changes_do_not() {
    let mut original = Message { id: 7, account: 2, folder: 3, message_id: "test@example".into(), ..Default::default() };
    original.body.html = Some("<img src='cid:logo'>".into());
    let mut image = Attachment::new("logo.png", "image/png", vec![1, 2, 3]);
    image.content_id = "logo".into();
    original.attachments.push(image);
    let mut changed = original.clone();
    changed.read = !changed.read;
    changed.subject = "changed metadata".into();
    assert!(native_html_same_message(&original, &changed));
    changed.body.html = Some("<p>changed</p>".into());
    assert!(!native_html_same_message(&original, &changed));
    changed = original.clone();
    changed.attachments[0].data = std::sync::Arc::new(vec![1, 2, 4]);
    assert!(!native_html_same_message(&original, &changed));
    changed = original.clone();
    changed.attachments[0].content_id = "other".into();
    assert!(!native_html_same_message(&original, &changed));
    changed = original.clone();
    changed.account += 1;
    assert!(!native_html_same_message(&original, &changed));
}
