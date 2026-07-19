//! Reactive egui host. Rasterization stays on the CPU; softbuffer presents with Win32 GDI.
use crate::ui::Editor;
mod diagnostics;
use diagnostics::{Counters, Diagnostic};
use egui_software_backend::{BufferMutRef, EguiSoftwareRender};
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
    scene: crate::rasterizer::SceneCache,
    pending_textures: egui::TexturesDelta,
    pixels: Vec<u32>,
    pixel_size: (u32, u32),
    deadline: Option<Instant>,
    needs_ui: bool,
    force_present: bool,
    occluded: bool,
    close_requested: bool,
    error: Option<String>,
    counters: Counters,
    diagnostic: Option<Diagnostic>,
    cleanup: Option<std::path::PathBuf>,
}
impl Desktop {
    fn visible(&self) -> bool {
        self.native.as_ref().is_some_and(|n| {
            let size = n.window.inner_size();
            !self.occluded
                && n.window.is_visible() != Some(false)
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
        if !visible && !self.close_requested {
            return Ok(());
        }
        let Some(native) = &mut self.native else {
            return Ok(());
        };
        let mut present = self.force_present || !self.needs_ui;
        if self.needs_ui || self.close_requested {
            let layout_started = Instant::now();
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
            self.counters.layout += 1;
            self.counters.layout_ms += layout_started.elapsed().as_secs_f64() * 1000.0;
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
                let raster_started = Instant::now();
                let size = native.window.inner_size();
                let changed = self.scene.changed(
                    &output.shapes,
                    (size.width, size.height),
                    output.pixels_per_point,
                    !self.pending_textures.is_empty(),
                );
                if !changed && !present {
                    return Ok(());
                }
                if changed {
                    present = true;
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
                    self.counters.raster += 1;
                    self.counters.raster_ms += raster_started.elapsed().as_secs_f64() * 1000.0;
                }
            } else {
                // Retain texture uploads until a visible frame can consume them.
                self.needs_ui = true;
            }
        }
        if visible && present && !self.pixels.is_empty() {
            let present_started = Instant::now();
            let (width, height) = self.pixel_size;
            native.surface.resize(
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            )?;
            let mut buffer = native.surface.buffer_mut()?;
            buffer.copy_from_slice(&self.pixels);
            buffer.present()?;
            self.force_present = false;
            self.counters.present += 1;
            self.counters.present_ms += present_started.elapsed().as_secs_f64() * 1000.0;
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
                .with_active(self.diagnostic.is_none())
                .with_inner_size([1180.0, 780.0])
                .with_min_inner_size([900.0, 640.0]);
            let window = Arc::new(egui_winit::create_window(&self.ctx, event_loop, &builder)?);
            crate::app_icon::apply(&window)?;
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
                self.editor = Some(if self.diagnostic.is_some() {
                    Editor::for_capture(
                        self.ctx.clone(),
                        slideshow_maker::model::Project::default(),
                        "editor",
                    )
                    .expect("The empty diagnostic project is valid")
                } else {
                    Editor::new(self.ctx.clone())
                });
                self.needs_ui = true;
                if let Some(directory) = self.cleanup.take() {
                    match slideshow_maker::updater::config() {
                        Ok(config) => {
                            rust_auto_updator::cleanup_after_update(directory, config.install)
                        }
                        Err(error) => eprintln!("Could not clean update staging: {error}"),
                    }
                }
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
                if matches!(event, WindowEvent::ScaleFactorChanged { .. }) {
                    crate::app_icon::apply(&native.window).unwrap_or_else(|error| {
                        eprintln!("Could not update native icons for DPI: {error}")
                    });
                }
                self.counters.input += 1;
                let cause = match &event {
                    WindowEvent::CursorMoved { .. } => 0,
                    WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } => 1,
                    WindowEvent::MouseInput { .. } => 2,
                    WindowEvent::MouseWheel { .. } => 3,
                    WindowEvent::KeyboardInput { .. } => 4,
                    WindowEvent::Ime(_) => 5,
                    WindowEvent::Focused(_) => 6,
                    WindowEvent::Moved(_)
                    | WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. } => 7,
                    WindowEvent::ModifiersChanged(_) => 8,
                    _ => 9,
                };
                self.counters.causes[cause] += 1;
                if matches!(
                    event,
                    WindowEvent::Resized(_)
                        | WindowEvent::ScaleFactorChanged { .. }
                        | WindowEvent::Occluded(false)
                ) {
                    self.force_present = true;
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
        if !self.visible() {
            // Keep dirty state for restore, but discard cosmetic repaint deadlines.
            self.deadline = None;
        }
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
        self.diagnostic_tick(event_loop);
        if let Some(diagnostic) = &self.diagnostic {
            wake = earliest(wake, diagnostic.next);
        }
        // Always restore waiting after an input/redraw. There is no polling mode.
        event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let Some(editor) = &mut self.editor {
            editor.shutdown();
        }
        self.ctx.set_request_repaint_callback(|_| {});
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
    run_internal(None, None)
}
pub fn run_updated(directory: std::path::PathBuf) -> Result<(), Box<dyn Error>> {
    run_internal(None, Some(directory))
}
pub fn run_diagnostic(path: std::path::PathBuf) -> Result<(), Box<dyn Error>> {
    if !path.is_absolute() {
        return Err("Diagnostic report path must be absolute".into());
    }
    run_internal(Some(path), None)
}
fn run_internal(
    diagnostic: Option<std::path::PathBuf>,
    cleanup: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn Error>> {
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
        renderer: crate::rasterizer::create(),
        scene: Default::default(),
        pending_textures: Default::default(),
        pixels: Vec::new(),
        pixel_size: (0, 0),
        deadline: None,
        needs_ui: true,
        force_present: true,
        occluded: false,
        close_requested: false,
        error: None,
        counters: Counters::default(),
        diagnostic: diagnostic.map(Diagnostic::new),
        cleanup,
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    if app.diagnostic.as_ref().is_some_and(|d| !d.completed()) {
        return Err("Diagnostic ended before all phases completed".into());
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
