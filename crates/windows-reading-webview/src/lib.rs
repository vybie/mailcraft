// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Isolated Windows WebView2 host for the reading pane. The host is not active until the
//! application supplies a message and installs the rest of the message-resource bridge.

#[cfg(windows)]
mod windows;
mod policy;
#[cfg(windows)]
mod inspection;

#[cfg(windows)]
pub use windows::Host;
