// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

#[test]
fn attachment_survives_owner_drop_and_can_be_tidied_later() {
    let base = std::env::temp_dir().join(format!("mailcraft-attachment-lifetime-{}", std::process::id()));
    let opened = Opened::at(base.clone());
    let path = opened.write("report with spaces.pdf", b"synthetic pdf content").unwrap();
    drop(opened);
    assert_eq!(path.file_name().unwrap(), "report with spaces.pdf");
    assert_eq!(fs::read(&path).unwrap(), b"synthetic pdf content");
    Opened::at(base.clone()).tidy(Duration::ZERO);
    assert!(!path.exists());
    let _ = fs::remove_dir(base);
}

#[cfg(windows)]
#[test]
fn windows_attachments_use_the_system_temp_directory() {
    assert_eq!(base(), Some(std::env::temp_dir().join("MailCraft").join("Attachments")));
}
