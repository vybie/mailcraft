// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windows taskbar overlay, isolated here because the native COM API is unsafe.

#[cfg(windows)]
pub fn set_overlay(hwnd: isize, rgba: Option<&[u8]>, size: u32, description: &str) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
    use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
    use windows::Win32::UI::WindowsAndMessaging::{CreateIcon, DestroyIcon, HICON};
    use windows::core::HSTRING;

    if hwnd == 0 || size == 0 || size > 256 || rgba.is_some_and(|p| p.len() != size as usize * size as usize * 4) {
        return Err("invalid window or icon".into());
    }
    let mut bgra = rgba.unwrap_or_default().to_vec();
    for pixel in bgra.chunks_exact_mut(4) { pixel.swap(0, 2); }
    // AND masks are monochrome scanlines padded to 16-bit boundaries.
    let mask = vec![0u8; ((size as usize + 15) / 16 * 2) * size as usize];
    // The HWND belongs to the live winit window. Windows copies the icon in SetOverlayIcon;
    // the matching DestroyIcon and COM uninitialization happen before returning.
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| {
            let taskbar: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
            taskbar.HrInit()?;
            let icon = if rgba.is_some() { CreateIcon(None, size as i32, size as i32, 1, 32, mask.as_ptr(), bgra.as_ptr())? } else { HICON::default() };
            let result = taskbar.SetOverlayIcon(HWND(hwnd as *mut _), icon, &HSTRING::from(description));
            if rgba.is_some() { let _ = DestroyIcon(icon); }
            result
        })();
        if initialized { CoUninitialize(); }
        result.map_err(|e: windows::core::Error| e.to_string())
    }
}
