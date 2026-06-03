use std::error::Error;
use winit::window::Window;

#[cfg(windows)]
pub fn apply(window: &Window) -> Result<(), Box<dyn Error>> {
    use winit::{
        dpi::PhysicalSize,
        platform::windows::{IconExtWindows, WindowExtWindows},
        window::Icon,
    };
    let scale = window.scale_factor();
    let size = |logical: f64| (logical * scale).round().max(1.0) as u32;
    let small = size(16.0);
    let large = size(32.0);
    // Resource 1 is also the Explorer icon. Load the appropriate native ICO
    // frames for the title bar and taskbar instead of relying on a fallback.
    window.set_window_icon(Some(Icon::from_resource(
        1,
        Some(PhysicalSize::new(small, small)),
    )?));
    window.set_taskbar_icon(Some(Icon::from_resource(
        1,
        Some(PhysicalSize::new(large, large)),
    )?));
    Ok(())
}
#[cfg(not(windows))]
pub fn apply(window: &Window) -> Result<(), Box<dyn Error>> {
    let image = image::load_from_memory(include_bytes!("../assets/SlideshowMaker.png"))?.to_rgba8();
    window.set_window_icon(Some(winit::window::Icon::from_rgba(
        image.clone().into_raw(),
        image.width(),
        image.height(),
    )?));
    Ok(())
}

#[cfg(windows)]
pub fn inspect(window: &Window) -> serde_json::Value {
    use windows_sys::Win32::{
        Graphics::Gdi::{BITMAP, DeleteObject, GetObjectW},
        UI::WindowsAndMessaging::{
            GetIconInfo, ICON_BIG, ICON_SMALL, ICONINFO, SendMessageW, WM_GETICON,
        },
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else {
        return serde_json::Value::Null;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return serde_json::Value::Null;
    };
    let dimensions = |kind: u32| -> Option<[i32; 2]> {
        // Read only this application's own native window/icon handles.
        unsafe {
            let icon = SendMessageW(handle.hwnd.get() as _, WM_GETICON, kind as usize, 0) as _;
            let mut info = ICONINFO::default();
            if GetIconInfo(icon, &mut info) == 0 {
                return None;
            }
            let mut bitmap = BITMAP::default();
            let read = GetObjectW(
                info.hbmColor,
                std::mem::size_of::<BITMAP>() as i32,
                (&mut bitmap as *mut BITMAP).cast(),
            );
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
            (read != 0).then_some([bitmap.bmWidth, bitmap.bmHeight])
        }
    };
    let small = dimensions(ICON_SMALL);
    let large = dimensions(ICON_BIG);
    serde_json::json!({"small":small,"large":large,"scale":window.scale_factor(),"present":small.is_some()&&large.is_some()})
}
#[cfg(not(windows))]
pub fn inspect(_: &Window) -> serde_json::Value {
    serde_json::Value::Null
}

pub fn verify_native(output: std::path::PathBuf) -> Result<(), Box<dyn Error>> {
    use winit::{
        application::ApplicationHandler,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    };
    if !output.is_absolute() {
        return Err("Icon report path must be absolute".into());
    }
    struct Probe {
        path: std::path::PathBuf,
        error: Option<String>,
        complete: bool,
        deadline: std::time::Instant,
    }
    impl ApplicationHandler for Probe {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let result = (|| -> Result<(), Box<dyn Error>> {
                let window = event_loop.create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_active(false)
                        .with_title("SlideshowMaker icon validation"),
                )?;
                apply(&window)?;
                let icons = inspect(&window);
                if icons["present"] != true {
                    return Err("The native small/large icons are missing".into());
                }
                slideshow_maker::storage::atomic_json(
                    &self.path,
                    &serde_json::json!({"native_icons":icons,"pid":std::process::id(),"hidden_window":true,"ui_frames":0,"version":env!("CARGO_PKG_VERSION")}),
                )?;
                Ok(())
            })();
            if let Err(error) = result {
                self.error = Some(error.to_string());
            } else {
                self.complete = true;
            }
            event_loop.exit();
        }
        fn window_event(
            &mut self,
            _: &ActiveEventLoop,
            _: winit::window::WindowId,
            _: winit::event::WindowEvent,
        ) {
        }
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if std::time::Instant::now() >= self.deadline {
                self.error = Some("Native icon probe timed out".into());
                event_loop.exit();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.deadline));
            }
        }
    }
    let event_loop = EventLoop::new()?;
    let mut probe = Probe {
        path: output,
        error: None,
        complete: false,
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(3),
    };
    event_loop.run_app(&mut probe)?;
    if let Some(error) = probe.error {
        return Err(error.into());
    }
    if !probe.complete {
        return Err("Native icon probe did not complete".into());
    }
    Ok(())
}
