#![cfg_attr(windows, windows_subsystem = "windows")]
mod app_icon;
mod capture;
mod desktop;
mod rasterizer;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|flag| flag == "--verify-native-icons")
    {
        if arguments.len() != 2 {
            return Err("Use --verify-native-icons ABSOLUTE_JSON_PATH".into());
        }
        return app_icon::verify_native(arguments[1].clone().into());
    }
    if arguments
        .first()
        .is_some_and(|flag| flag == "--diagnostic-report")
    {
        if arguments.len() != 2 {
            return Err("Use --diagnostic-report ABSOLUTE_JSON_PATH".into());
        }
        return desktop::run_diagnostic(arguments[1].clone().into());
    }
    if !arguments.is_empty() {
        return capture::run(capture::Options::parse(&arguments)?);
    }
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
