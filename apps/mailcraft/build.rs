// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windows only: embed the app icon and version info (VERSIONINFO) into `mailcraft.exe`, so it
//! shows in Explorer, the taskbar, the Start menu and Alt-Tab.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `MAILCRAFT_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/mailcraft.ico");
    println!("cargo:rerun-if-env-changed=MAILCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    copy_webview_loader();
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/mailcraft.ico")
        .set("ProductName", "MailCraft")
        .set("FileDescription", "MailCraft mail and calendar")
        .set("CompanyName", "MailCraft contributors")
        .set("LegalCopyright", "Copyright (c) the MailCraft authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "mailcraft.exe")
        .set("InternalName", "mailcraft");
    if let Err(e) = res.compile() {
        if std::env::var_os("MAILCRAFT_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=mailcraft.exe built without icon/version resources: {e}");
    }
}

/// Use the locally installed, version-pinned Microsoft SDK for personal Windows builds.
/// Keep the license and notices beside the binary for redistribution of the loader.
fn copy_webview_loader() {
    use std::path::PathBuf;
    println!("cargo:rerun-if-env-changed=MAILCRAFT_WEBVIEW2_SDK");
    let sdk = std::env::var_os("MAILCRAFT_WEBVIEW2_SDK").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".nuget/packages/microsoft.web.webview2/1.0.3719.77"))
    });
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => return,
    };
    let Some(sdk) = sdk else { return };
    let loader = sdk.join("build/native").join(arch).join("WebView2Loader.dll");
    let license = sdk.join("LICENSE.txt");
    let notice = sdk.join("NOTICE.txt");
    for path in [&loader, &license, &notice] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let Some(out) = std::env::var_os("OUT_DIR").map(PathBuf::from) else { return };
    let Some(profile) = out.ancestors().nth(3) else { return };
    for (source, name) in [(&loader, "WebView2Loader.dll"), (&license, "WebView2Loader-LICENSE.txt"), (&notice, "WebView2Loader-NOTICE.txt")] {
        if let Err(error) = std::fs::copy(source, profile.join(name)) {
            println!("cargo:warning=Could not copy {}: {error}; WebView2 will fall back if the loader is unavailable", source.display());
        }
    }
}
