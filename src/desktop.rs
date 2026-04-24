//! Reactive egui host. Rasterization stays on the CPU; softbuffer presents with Win32 GDI.
use crate::ui::Editor;
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use std::{
    error::Error,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

#[derive(Debug)]
struct Wake {
    at: Instant,
    pass: u64,
}

fn earliest(current: Option<Instant>, incoming: Instant) -> Option<Instant> {
    Some(current.map_or(incoming, |old| old.min(incoming)))
}

struct NativeWindow {
    // The surface owns a window reference and must drop before the remaining window/state.
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    input: egui_winit::State,
    window: Arc<Window>,
}
struct Desktop {
    native: Option<NativeWindow>,
    ctx: egui::Context,
    editor: Option<Editor>,
    renderer: EguiSoftwareRender,
    pending_textures: egui::TexturesDelta,
    pixels: Vec<u32>,
    pixel_size: (u32, u32),
    deadline: Option<Instant>,
    needs_ui: bool,
    occluded: bool,
    close_requested: bool,
    error: Option<String>,
    stats: Option<Statistics>,
}
impl Desktop {
    fn visible(&self) -> bool {
        self.native.as_ref().is_some_and(|n| {
            let size = n.window.inner_size();
            !self.occluded
                && size.width > 0
                && size.height > 0
                && n.window.is_minimized() != Some(true)
        })
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }
    fn frame(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let visible = self.visible();
        let Some(native) = &mut self.native else {
            return Ok(());
        };
        let started = Instant::now();
        if self.needs_ui || self.close_requested {
            let info = native
                .input
                .egui_input_mut()
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default();
            egui_winit::update_viewport_info(info, &self.ctx, &native.window, false);
            let mut input = native.input.take_egui_input(&native.window);
            if self.close_requested {
                input
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default()
                    .events
                    .push(egui::ViewportEvent::Close);
            }
            let output = self.ctx.run(input, |ctx| {
                if let Some(editor) = &mut self.editor {
                    editor.update(ctx);
                }
            });
            self.needs_ui = false;
            native
                .input
                .handle_platform_output(&native.window, output.platform_output);
            self.pending_textures.append(output.textures_delta);
            if let Some(viewport) = output.viewport_output.get(&egui::ViewportId::ROOT) {
                if let Some(at) = Instant::now().checked_add(viewport.repaint_delay) {
                    self.deadline = earliest(self.deadline, at);
                }
                let mut commands = Vec::new();
                for command in &viewport.commands {
                    match command {
                        egui::ViewportCommand::Close => self.close_requested = true,
                        egui::ViewportCommand::CancelClose => self.close_requested = false,
                        command => commands.push(command.clone()),
                    }
                }
                let info = native
                    .input
                    .egui_input_mut()
                    .viewports
                    .entry(egui::ViewportId::ROOT)
                    .or_default();
                let mut actions = Vec::new();
                egui_winit::process_viewport_commands(
                    &self.ctx,
                    info,
                    commands,
                    &native.window,
                    &mut actions,
                );
                for action in actions {
                    let event = match action {
                        egui_winit::ActionRequested::Copy => Some(egui::Event::Copy),
                        egui_winit::ActionRequested::Cut => Some(egui::Event::Cut),
                        egui_winit::ActionRequested::Paste => {
                            native.input.clipboard_text().map(egui::Event::Paste)
                        }
                        egui_winit::ActionRequested::Screenshot(_) => None,
                    };
                    if let Some(event) = event {
                        native.input.egui_input_mut().events.push(event);
                        self.needs_ui = true;
                    }
                }
            }
            if self.close_requested {
                event_loop.exit();
                return Ok(());
            }
            if visible {
                let size = native.window.inner_size();
                self.pixel_size = (size.width, size.height);
                self.pixels
                    .resize(size.width as usize * size.height as usize, 0);
                let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
                let mut buffer = BufferMutRef::new(
                    bytemuck::cast_slice_mut(&mut self.pixels),
                    size.width as usize,
                    size.height as usize,
                );
                self.renderer.render(
                    &mut buffer,
                    &primitives,
                    &std::mem::take(&mut self.pending_textures),
                    output.pixels_per_point,
                );
                if let Some(stats) = &mut self.stats
                    && !stats.reported
                {
                    stats.ui_frames += 1;
                }
            } else {
                // Retain texture uploads until a visible frame can consume them.
                self.needs_ui = true;
            }
        }
        if visible && !self.pixels.is_empty() {
            let (width, height) = self.pixel_size;
            native.surface.resize(
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            )?;
            let mut buffer = native.surface.buffer_mut()?;
            buffer.copy_from_slice(&self.pixels);
            buffer.present()?;
            if let Some(stats) = &mut self.stats
                && !stats.reported
            {
                stats.presented += 1;
                if stats.frames.len() < 4096 {
                    stats.frames.push(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
        }
        Ok(())
    }
}
impl ApplicationHandler<Wake> for Desktop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.native.is_some() {
            return;
        }
        let result = (|| -> Result<NativeWindow, Box<dyn Error>> {
            let builder = egui::ViewportBuilder::default()
                .with_title("SlideshowMaker")
                .with_inner_size([1180.0, 780.0])
                .with_min_inner_size([900.0, 640.0]);
            let window = Arc::new(egui_winit::create_window(&self.ctx, event_loop, &builder)?);
            let context = softbuffer::Context::new(window.clone())?;
            let surface = softbuffer::Surface::new(&context, window.clone())?;
            let input = egui_winit::State::new(
                self.ctx.clone(),
                egui::ViewportId::ROOT,
                &window,
                Some(window.scale_factor() as f32),
                None,
                None,
            );
            Ok(NativeWindow {
                surface,
                input,
                window,
            })
        })();
        match result {
            Ok(native) => {
                self.native = Some(native);
                self.editor = Some(Editor::new(self.ctx.clone()));
                self.needs_ui = true;
            }
            Err(error) => self.fail(event_loop, error),
        }
    }
    fn user_event(&mut self, _: &ActiveEventLoop, event: Wake) {
        let pass = self.ctx.cumulative_pass_nr();
        if event.pass == pass || event.pass.saturating_add(1) == pass {
            self.deadline = earliest(self.deadline, event.at);
        }
    }
    fn new_events(&mut self, _: &ActiveEventLoop, _: StartCause) {}
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(native) = &mut self.native else {
            return;
        };
        if id != native.window.id() {
            return;
        }
        match event {
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.frame(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::CloseRequested => {
                self.close_requested = true;
                self.needs_ui = true;
                native.window.request_redraw();
            }
            event => {
                if let Some(stats) = &mut self.stats {
                    stats.input_events += 1;
                }
                if let WindowEvent::Occluded(occluded) = &event {
                    self.occluded = *occluded;
                }
                let response = native.input.on_window_event(&native.window, &event);
                if response.repaint
                    || matches!(
                        event,
                        WindowEvent::Resized(_)
                            | WindowEvent::ScaleFactorChanged { .. }
                            | WindowEvent::Occluded(false)
                    )
                {
                    self.needs_ui = true;
                }
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.deadline.is_some_and(|at| at <= now) {
            self.deadline = None;
            self.needs_ui = true;
        }
        if self.needs_ui
            && self.visible()
            && let Some(native) = &self.native
        {
            native.window.request_redraw();
        }
        let mut wake = self.deadline;
        if let Some(stats) = &mut self.stats {
            if !stats.reported && now >= stats.report_at {
                stats.report();
            }
            if !stats.reported {
                wake = earliest(wake, stats.report_at);
            }
        }
        // Always restore waiting after an input/redraw. There is no polling mode.
        event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let Some(editor) = &mut self.editor {
            editor.shutdown();
        }
        if let Some(stats) = &mut self.stats
            && !stats.reported
        {
            stats.report();
        }
        self.ctx.set_request_repaint_callback(|_| {});
    }
}

struct Statistics {
    start: Instant,
    report_at: Instant,
    cpu_start: f64,
    ui_frames: u64,
    presented: u64,
    input_events: u64,
    frames: Vec<f64>,
    reported: bool,
}
impl Statistics {
    fn enabled() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        if !exe
            .parent()?
            .join("SlideshowMaker-diagnostics.enabled")
            .exists()
        {
            return None;
        }
        let start = Instant::now();
        Some(Self {
            start,
            report_at: start + Duration::from_secs(12),
            cpu_start: cpu_seconds(),
            ui_frames: 0,
            presented: 0,
            input_events: 0,
            frames: Vec::new(),
            reported: false,
        })
    }
    fn report(&mut self) {
        self.reported = true;
        let elapsed = self.start.elapsed().as_secs_f64();
        self.frames.sort_by(f64::total_cmp);
        let percentile = |fraction: f64| {
            self.frames
                .get(((self.frames.len().saturating_sub(1)) as f64 * fraction) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let report = serde_json::json!({ "renderer": "CPU rasterization / Win32 GDI", "elapsed_seconds": elapsed,
            "one_core_cpu_percent": 100.0 * (cpu_seconds() - self.cpu_start) / elapsed,
            "ui_frames": self.ui_frames, "presented_frames": self.presented, "window_input_events": self.input_events,
            "frame_ms_p50": percentile(0.5), "frame_ms_p95": percentile(0.95), "frame_ms_max": percentile(1.0) });
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            let _ = std::fs::write(
                parent.join("SlideshowMaker-performance.json"),
                report.to_string(),
            );
        }
    }
}
#[cfg(windows)]
fn cpu_seconds() -> f64 {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // Reads timing counters for this process only; does not change thread or system state.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        );
    }
    let ticks =
        |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    (ticks(kernel) + ticks(user)) as f64 / 10_000_000.0
}
#[cfg(not(windows))]
fn cpu_seconds() -> f64 {
    0.0
}

pub fn run() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::<Wake>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let ctx = egui::Context::default();
    ctx.set_request_repaint_callback(move |info| {
        if let Some(at) = Instant::now().checked_add(info.delay) {
            let _ = proxy.send_event(Wake {
                at,
                pass: info.current_cumulative_pass_nr,
            });
        }
    });
    let mut app = Desktop {
        native: None,
        ctx,
        editor: None,
        renderer: EguiSoftwareRender::new(ColorFieldOrder::Bgra),
        pending_textures: Default::default(),
        pixels: Vec::new(),
        pixel_size: (0, 0),
        deadline: None,
        needs_ui: true,
        occluded: false,
        close_requested: false,
        error: None,
        stats: Statistics::enabled(),
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn later_repaint_cannot_delay_an_earlier_deadline() {
        let now = Instant::now();
        let soon = now + Duration::from_millis(15);
        let later = now + Duration::from_secs(3);
        assert_eq!(earliest(earliest(None, later), soon), Some(soon));
        assert_eq!(earliest(Some(soon), later), Some(soon));
    }
}
