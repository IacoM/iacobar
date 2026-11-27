//! Microsoft Store build: the same exe runs inside an MSIX package.
//! Packaged, Windows handles what the NSIS installer handles otherwise:
//! updates come from the Store and autostart is the package's startup task.

use std::sync::OnceLock;

use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_INSUFFICIENT_BUFFER};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::core::h;

/// Must match the `TaskId` of the startup task in `msix/AppxManifest.xml`.
const STARTUP_TASK: &windows::core::HSTRING = h!("IacobarStartup");

/// True when running from the Microsoft Store (MSIX) package.
pub fn is_store() -> bool {
    static STORE: OnceLock<bool> = OnceLock::new();
    *STORE.get_or_init(|| {
        let mut len = 0u32;
        let r = unsafe { GetCurrentPackageFullName(&mut len, None) };
        // with no buffer a packaged process answers "buffer too small"
        r == ERROR_INSUFFICIENT_BUFFER || (r != APPMODEL_ERROR_NO_PACKAGE && len > 0)
    })
}

/// Turns the package's startup task on or off to match the user's setting.
/// Once the user disabled it in Task Manager / Settings, only they can turn
/// it back on: Windows refuses, and we leave it alone.
pub fn apply_autostart(enabled: bool) {
    let res = (|| -> windows::core::Result<()> {
        let task = StartupTask::GetAsync(STARTUP_TASK)?.get()?;
        let state = task.State()?;
        let on = state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy;
        if enabled && state == StartupTaskState::Disabled {
            task.RequestEnableAsync()?.get()?;
        } else if !enabled && on {
            task.Disable()?;
        }
        Ok(())
    })();
    if let Err(e) = res {
        eprintln!("[store] startup task: {e}");
    }
}
