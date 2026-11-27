//! Windows asking us to close: sign-out, shutdown, or an app manager closing
//! the app (the Microsoft Store uninstalling or updating the package). The
//! process is killed right after, so the Windows taskbar goes back NOW: an
//! MSIX package has no uninstaller that could run `--restore` later.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, RegisterClassW, TranslateMessage,
    WINDOW_EX_STYLE, WM_ENDSESSION, WM_QUERYENDSESSION, WNDCLASSW, WS_POPUP,
};
use windows::core::{PCWSTR, w};

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_QUERYENDSESSION => LRESULT(1), // never block a shutdown
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                crate::cleanup();
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// A hidden top-level window (message-only windows don't get these messages).
pub fn start() {
    std::thread::spawn(|| unsafe {
        let Ok(hinst) = GetModuleHandleW(None) else { return };
        let class = w!("IacobarSession");
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        let Ok(_hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst.into()),
            None,
        ) else {
            return;
        };
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
