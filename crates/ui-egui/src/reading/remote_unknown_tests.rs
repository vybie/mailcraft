// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use mailcraft_engine::Session;
use mailcraft_model::{FolderKind, Mailbox, Message};

#[test]
fn unknown_size_requires_message_choice_even_with_automatic_download() {
    let mut mailbox = Mailbox::with_local_account("Test", "test@example.invalid");
    let account = mailbox.accounts[0].id;
    let folder = mailbox.folder_of_kind(account, FolderKind::Inbox).unwrap();
    let id = mailbox.add_message(Message { account, folder, ..Default::default() }).unwrap();
    let services = crate::Services { pictures: Some(Box::new(|_| std::sync::mpsc::channel().1)), ..Default::default() };
    let mut app = MailApp::new(Session::new(mailbox), services);
    app.ui.download_pictures = true;
    let message = app.session.mailbox.messages[&id].clone();
    assert_eq!(access(&app, &message), Access::Allowed);
    assert!(!unknown_approved(&app, &message));
    approve_unknown(&mut app, &message);
    assert!(unknown_approved(&app, &message));
    clear(&mut app).unwrap();
    assert!(!unknown_approved(&app, &message));
    approve_unknown(&mut app, &message);
    let other = app.session.mailbox.clone();
    app.session.replace_mailbox(other);
    assert!(!unknown_approved(&app, &message));
}
