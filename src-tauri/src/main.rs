// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // On Linux, WebKitGTK's DMA-BUF renderer causes green video frames,
    // color corruption, and non-deterministic decode failures on many
    // GPU + driver combinations (Intel/AMD VA-API especially).  Disabling
    // it forces shared-memory rendering which is slightly slower but
    // eliminates these artifacts entirely.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    tauri_app_lib::run()
}
