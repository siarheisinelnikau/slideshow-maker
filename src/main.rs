#![cfg_attr(windows, windows_subsystem = "windows")]
mod desktop;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            let _ = std::fs::write(
                parent.join("SlideshowMaker-crash.txt"),
                format!("{info}\n{}", std::backtrace::Backtrace::force_capture()),
            );
        }
        original_hook(info);
    }));
    desktop::run()
}
