// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Synthetic, hidden-window WebView2 smoke check. Uses only a local loopback listener.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};
    use std::time::{Duration, Instant};
    use mailcraft_windows_reading_webview::Host;
    use winit::{dpi::LogicalSize, event_loop::EventLoop, window::Window};

    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let running = Arc::new(AtomicBool::new(true));
    let connections = Arc::new(AtomicUsize::new(0));
    let (active, seen) = (running.clone(), connections.clone());
    let server = std::thread::spawn(move || {
        while active.load(Ordering::Relaxed) {
            if listener.accept().is_ok() { seen.fetch_add(1, Ordering::Relaxed); }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let events = EventLoop::new()?;
    #[allow(deprecated)]
    let window = Arc::new(events.create_window(Window::default_attributes().with_visible(false).with_inner_size(LogicalSize::new(800, 600)))?);
    let dir = std::env::temp_dir().join(format!("mailcraft-webview-smoke-{}", std::process::id()));
    let host = Host::new(window, dir.to_str().ok_or("bad temporary path")?, 1, || {})?;
    host.set_dark(true)?;
    let html = format!("<style>.hero{{width:200px;height:100px;background-image:url(http://{address}/background.png)}}</style>\
        <div class='hero'></div><img width=40 height=40 src='http://{address}/image.png'>\
        <img width=1 height=1 src='http://{address}/tracker.png'>");
    host.show(&html, [0, 0, 800, 600])?;
    let until = Instant::now() + Duration::from_secs(15);
    let mut success = false;
    while Instant::now() < until {
        windows_window::pump();
        host.poll()?;
        if host.loaded() { host.set_visible(true)?; }
        let found = host.image_requests();
        if host.ready() && found.iter().any(|u| u.ends_with("/image.png")) && found.iter().any(|u| u.ends_with("/background.png")) {
            assert!(!found.iter().any(|u| u.ends_with("/tracker.png")));
            success = true;
            break;
        }
        if host.failed() { break; }
        std::thread::sleep(Duration::from_millis(25));
    }
    host.set_dark(false)?;
    for _ in 0..20 {
        windows_window::pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!host.failed(), "WebView2 theme emulation failed");
    drop(host);
    running.store(false, Ordering::Relaxed);
    let _ = server.join();
    assert_eq!(connections.load(Ordering::Relaxed), 0, "browser attempted a network request");
    assert!(success, "WebView2 did not report the expected classified image requests");
    println!("synthetic WebView2 image and network-blocking smoke passed");
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
