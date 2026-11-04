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
    let html = format!("<style>body{{background:#fff;color:#111}}.hero{{width:200px;height:100px;background-image:url(http://{address}/background.png);background-size:100px 100px}}.long{{height:3000px}}</style>\
        <div class='hero'></div><img width=40 height=40 src='http://{address}/image.png'>\
        <img width=1 height=1 src='http://{address}/tracker.png'><img src='http://{address}/unknown.png'><div class='long'>Scrollable synthetic message</div>");
    host.show(&html, [0, 0, 800, 600])?;
    host.set_image_access(1, true);
    let until = Instant::now() + Duration::from_secs(15);
    let mut success = false;
    while Instant::now() < until {
        windows_window::pump();
        host.poll()?;
        let found = host.image_requests();
        if host.ready() && host.theme_ready() && host.unknown_image_count() == 1 && found.iter().any(|u| u.ends_with("/image.png")) && found.iter().any(|u| u.ends_with("/background.png")) {
            assert!(!found.iter().any(|u| u.ends_with("/tracker.png")));
            success = true;
            break;
        }
        if host.failed() { break; }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(success, "initial message did not become ready");
    host.set_visible(true)?;
    #[cfg(debug_assertions)]
    fn eval(host: &Host, script: &str) -> Result<String, Box<dyn std::error::Error>> {
        let result=std::rc::Rc::new(std::cell::RefCell::new(None));
        let output=result.clone();
        host.smoke_eval(script,move |value| *output.borrow_mut()=Some(value))?;
        let deadline=Instant::now()+Duration::from_secs(5);
        while result.borrow().is_none() && Instant::now()<deadline { windows_window::pump(); std::thread::sleep(Duration::from_millis(5)); }
        let value=result.borrow_mut().take().ok_or("script timed out")??;
        Ok(value)
    }
    #[cfg(debug_assertions)]
    assert_eq!(eval(&host,"window.scrollTo(0,900); window.scrollY")?,"900");
    #[cfg(debug_assertions)]
    fn capture(host: &Host, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        host.set_visible(true)?;
        let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
        let result = captured.clone();
        host.smoke_capture(move |value| *result.borrow_mut() = Some(value))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while captured.borrow().is_none() && Instant::now() < deadline {
            windows_window::pump();
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = captured.borrow_mut().take().ok_or("capture timed out")??;
        std::fs::write(path, output)?;
        Ok(())
    }
    #[cfg(debug_assertions)]
    let capture_path = std::env::var_os("MAILCRAFT_SMOKE_CAPTURE").map(std::path::PathBuf::from);
    #[cfg(debug_assertions)]
    if let Some(path) = &capture_path { capture(&host, path)?; }
    // Supply a delayed, validated raster after the blocked first request.
    let gif=Arc::new(b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\x00\x00\x00\x00\x00\x21\xf9\x04\x01\x00\x00\x00\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b".to_vec());
    let unknown = format!("http://{address}/unknown.png");
    assert!(!host.image_requests().contains(&unknown), "automatic permission must not approve unknown sizes");
    assert!(!host.add_image(1, &unknown, gif.clone()));
    host.set_unknown_image_access(1, true);
    assert!(host.image_requests().contains(&unknown));
    assert!(host.add_image(1, &unknown, gif.clone()));
    assert!(host.add_image(1, &format!("http://{address}/image.png"), gif));
    host.refresh()?;
    assert!(!host.ready() && host.refreshing(), "same-message reload should remain visible");
    let until = Instant::now() + Duration::from_secs(15);
    while !host.ready() && !host.failed() && Instant::now() < until {
        windows_window::pump();
        host.poll()?;
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(host.ready(), "WebView2 did not complete the image refresh");
    assert!(!host.refreshing());
    #[cfg(debug_assertions)]
    { assert_eq!(eval(&host,"window.scrollY")?,"900"); assert_eq!(eval(&host,"document.querySelector('img').naturalWidth")?,"1"); }
    #[cfg(debug_assertions)]
    if let Some(path) = &capture_path { capture(&host, &path.with_extension("reload.json"))?; }
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
