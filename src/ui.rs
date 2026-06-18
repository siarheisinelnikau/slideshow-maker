use egui::{self, Color32, RichText};
mod screens;
use slideshow_maker::{
    export::{self, Format, Progress},
    model::*,
    render::Renderer,
    storage::{self, Settings},
    updater::{self, Staged, Update},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct PreviewToken {
    revision: u64,
    generation: u64,
}
enum Job {
    Import,
    Music,
    Relink(usize),
    Ffmpeg,
    Open,
    Save(Project, Option<PathBuf>),
    Settings(Settings),
    Preview(Project, f64, PreviewToken),
    Export(Project, Format, Option<PathBuf>, Progress),
}
enum Event {
    Photos(Vec<PathBuf>),
    Music(PathBuf),
    Relink(usize, PathBuf),
    Ffmpeg(PathBuf),
    Opened(Project, PathBuf),
    Saved(PathBuf),
    SettingsSaved,
    Preview(image::RgbImage, PreviewToken),
    PreviewError(String, PreviewToken),
    Exported(PathBuf),
    Cancelled,
    Error(String),
}
struct Worker {
    tx: SyncSender<Job>,
    rx: Receiver<Event>,
    busy: bool,
    previewing: bool,
    pending: Option<(Job, String)>,
    label: String,
}
impl Worker {
    fn new(ctx: egui::Context) -> Self {
        let (tx, requests) = mpsc::sync_channel(1);
        let (events, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut renderer = Renderer::default();
            while let Ok(job) = requests.recv() {
                let preview_token = match &job {
                    Job::Preview(_, _, token) => Some(*token),
                    _ => None,
                };
                let result = (|| -> Result<Event> {
                    let images = || {
                        rfd::FileDialog::new()
                            .add_filter("Photos", &["jpg", "jpeg", "png", "webp", "bmp"])
                    };
                    Ok(match job {
                        Job::Import => images()
                            .pick_files()
                            .map(Event::Photos)
                            .unwrap_or(Event::Cancelled),
                        Job::Music => rfd::FileDialog::new()
                            .add_filter("PCM 16-bit WAV", &["wav"])
                            .pick_file()
                            .map(|p| slideshow_maker::avi::Audio::read(&p).map(|_| Event::Music(p)))
                            .transpose()?
                            .unwrap_or(Event::Cancelled),
                        Job::Relink(i) => images()
                            .pick_file()
                            .map(|p| Event::Relink(i, p))
                            .unwrap_or(Event::Cancelled),
                        Job::Ffmpeg => rfd::FileDialog::new()
                            .add_filter("FFmpeg executable", &["exe"])
                            .pick_file()
                            .map(Event::Ffmpeg)
                            .unwrap_or(Event::Cancelled),
                        Job::Open => {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Slideshow project", &["json"])
                                .pick_file()
                            {
                                Event::Opened(storage::load_project(&path)?, path)
                            } else {
                                Event::Cancelled
                            }
                        }
                        Job::Save(project, path) => {
                            let path = path.or_else(|| {
                                rfd::FileDialog::new()
                                    .set_file_name("Family memories.slideshow.json")
                                    .add_filter("Slideshow project", &["json"])
                                    .save_file()
                            });
                            if let Some(path) = path {
                                storage::save_project(&path, &project)?;
                                Event::Saved(path)
                            } else {
                                Event::Cancelled
                            }
                        }
                        Job::Settings(s) => {
                            storage::atomic_json(&storage::settings_path(), &s)?;
                            Event::SettingsSaved
                        }
                        Job::Preview(p, t, rev) => {
                            let size = p.layout.size(360);
                            Event::Preview(renderer.frame(&p, t, size)?, rev)
                        }
                        Job::Export(p, format, ffmpeg, progress) => {
                            let filename = export::suggested_name(&p, format);
                            let ext = if format == Format::Avi { "avi" } else { "mp4" };
                            if let Some(mut path) = rfd::FileDialog::new()
                                .set_file_name(filename.to_string_lossy())
                                .add_filter(ext, &[ext])
                                .save_file()
                            {
                                if path
                                    .extension()
                                    .is_none_or(|e| !e.eq_ignore_ascii_case(ext))
                                {
                                    path.set_extension(ext);
                                }
                                export::export(&p, &path, format, ffmpeg.as_deref(), &progress)?;
                                Event::Exported(path)
                            } else {
                                Event::Cancelled
                            }
                        }
                    })
                })();
                let event = result.unwrap_or_else(|error| match preview_token {
                    Some(token) => Event::PreviewError(error, token),
                    None => Event::Error(error),
                });
                if events.send(event).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            tx,
            rx,
            busy: false,
            previewing: false,
            pending: None,
            label: String::new(),
        }
    }
    fn send(&mut self, job: Job, label: &str) {
        let preview = matches!(&job, Job::Preview(..));
        if self.busy {
            // One explicit action may wait behind a preview. Frames themselves
            // are never queued; the scheduler requests only the latest state.
            if self.previewing && !preview && self.pending.is_none() {
                self.pending = Some((job, label.into()));
            }
        } else if self.tx.try_send(job).is_ok() {
            self.busy = true;
            self.previewing = preview;
            self.label = label.into();
        }
    }
    fn blocks_ui(&self) -> bool {
        (self.busy && !self.previewing) || self.pending.is_some()
    }
    fn foreground_label(&self) -> &str {
        self.pending
            .as_ref()
            .map_or(&self.label, |(_, label)| label)
    }
    fn complete(&mut self) -> bool {
        let was_preview = self.previewing;
        self.busy = false;
        self.previewing = false;
        if let Some((job, label)) = self.pending.take() {
            self.send(job, &label);
        }
        was_preview
    }
}
enum NetEvent {
    Checked(Result<Option<Update>>),
    Downloaded(Result<Staged>),
}
#[derive(Clone, Copy)]
enum Discard {
    New,
    Open,
    Close,
}
pub struct Editor {
    project: Project,
    path: Option<PathBuf>,
    dirty: bool,
    selected: usize,
    time: f64,
    revision: u64,
    preview_generation: u64,
    preview_revision: u64,
    worker: Worker,
    texture: Option<egui::TextureHandle>,
    message: String,
    playing: bool,
    last_tick: Instant,
    settings: Settings,
    show_settings: bool,
    show_export: bool,
    inspector_tab: screens::InspectorTab,
    save_settings: bool,
    total_input: f64,
    format: Format,
    progress: Option<Progress>,
    net_tx: SyncSender<NetEvent>,
    net_rx: Receiver<NetEvent>,
    net_busy: bool,
    update: Option<Update>,
    update_text: String,
    downloaded: Arc<AtomicU64>,
    staged: Option<Staged>,
    discard: Option<Discard>,
    allow_close: bool,
    installing: bool,
}
impl Editor {
    pub fn new(ctx: egui::Context) -> Self {
        let mut app = Self::with_context(ctx.clone(), storage::load_settings());
        if app.settings.startup_updates {
            app.check_updates(&ctx);
        }
        app
    }
    fn with_context(ctx: egui::Context, settings: Settings) -> Self {
        screens::configure(&ctx);
        let (net_tx, net_rx) = mpsc::sync_channel(1);
        Self {
            project: Project::default(),
            path: None,
            dirty: false,
            selected: 0,
            time: 0.0,
            revision: 1,
            preview_generation: 1,
            preview_revision: 0,
            worker: Worker::new(ctx.clone()),
            texture: None,
            message: "Add photos or a title card to begin. Source files are never modified.".into(),
            playing: false,
            last_tick: Instant::now(),
            settings,
            show_settings: false,
            show_export: false,
            inspector_tab: Default::default(),
            save_settings: false,
            total_input: 0.0,
            format: Format::Avi,
            progress: None,
            net_tx,
            net_rx,
            net_busy: false,
            update: None,
            update_text: "Not checked".into(),
            downloaded: Arc::default(),
            staged: None,
            discard: None,
            allow_close: false,
            installing: false,
        }
    }
    pub(crate) fn for_capture(ctx: egui::Context, project: Project, screen: &str) -> Result<Self> {
        project.validate()?;
        let mut app = Self::with_context(ctx.clone(), Settings::default());
        app.project = project;
        app.total_input = app.project.duration();
        app.show_settings = screen == "settings";
        app.show_export = screen == "export";
        app.message = "Your originals stay unchanged.".into();
        if !app.project.slides.is_empty() {
            let image =
                Renderer::default().frame(&app.project, 0.0, app.project.layout.size(360))?;
            let size = [image.width() as usize, image.height() as usize];
            app.texture = Some(ctx.load_texture(
                "composite",
                egui::ColorImage::from_rgb(size, image.as_raw()),
                egui::TextureOptions::LINEAR,
            ));
        }
        app.preview_revision = app.revision;
        Ok(app)
    }
    fn change(&mut self) {
        self.playing = false;
        self.dirty = true;
        self.revision += 1;
        self.preview_generation += 1;
        self.total_input = self.project.duration();
    }
    fn preview_token(&self) -> PreviewToken {
        PreviewToken {
            revision: self.revision,
            generation: self.preview_generation,
        }
    }
    fn check_updates(&mut self, ctx: &egui::Context) {
        if self.net_busy {
            return;
        }
        self.net_busy = true;
        self.update_text = "Checking published releases…".into();
        let tx = self.net_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = updater::check();
            let _ = tx.send(NetEvent::Checked(result));
            ctx.request_repaint();
        });
    }
    fn download(&mut self, ctx: &egui::Context) {
        let Some(update) = self.update.clone() else {
            return;
        };
        if self.net_busy {
            return;
        }
        self.net_busy = true;
        self.update_text = "Downloading and verifying update…".into();
        self.downloaded.store(0, Ordering::Relaxed);
        let tx = self.net_tx.clone();
        let progress = self.downloaded.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = updater::download(&update, progress);
            let _ = tx.send(NetEvent::Downloaded(result));
            ctx.request_repaint();
        });
    }
    fn request_discard(&mut self, action: Discard, ctx: &egui::Context) {
        self.playing = false;
        if self.dirty {
            self.discard = Some(action);
        } else {
            self.discard_action(action, ctx);
        }
    }
    fn discard_action(&mut self, action: Discard, ctx: &egui::Context) {
        self.discard = None;
        match action {
            Discard::New => {
                self.project = Project::default();
                self.selected = 0;
                self.path = None;
                self.dirty = false;
                self.time = 0.0;
                self.texture = None;
                self.revision += 1;
                self.preview_generation += 1;
                self.total_input = 0.0;
            }
            Discard::Open => self.worker.send(Job::Open, "Opening project…"),
            Discard::Close => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn receive(&mut self, ctx: &egui::Context) {
        if let Ok(event) = self.worker.rx.try_recv() {
            let was_preview = self.worker.complete();
            let was_export = !was_preview && self.progress.take().is_some();
            match event {
                Event::Photos(paths) => {
                    let room = 500usize.saturating_sub(self.project.slides.len());
                    let count = paths.len().min(room);
                    for path in paths.into_iter().take(room) {
                        self.project.slides.push(Slide::photo(path));
                    }
                    self.message = format!(
                        "Added {count} photos (maximum 500 slides). Select a slide to edit it."
                    );
                    self.change();
                }
                Event::Music(path) => {
                    self.project.audio = Some(path);
                    self.change();
                }
                Event::Relink(i, path) => {
                    if let Some(slide) = self.project.slides.get_mut(i) {
                        slide.media = Media::Photo(path);
                        self.change();
                    }
                }
                Event::Ffmpeg(path) => {
                    self.settings.ffmpeg = Some(path);
                    self.save_settings = true;
                }
                Event::Opened(p, path) => {
                    self.project = p;
                    self.path = Some(path);
                    self.dirty = false;
                    self.selected = 0;
                    self.time = 0.0;
                    self.total_input = self.project.duration();
                    self.revision += 1;
                    self.preview_generation += 1;
                    self.message = "Project opened".into();
                }
                Event::Saved(path) => {
                    self.path = Some(path);
                    self.dirty = false;
                    self.message = "Project saved".into();
                }
                Event::SettingsSaved => {}
                Event::Preview(img, token) => {
                    // Playback can advance while decoding a frame. A ready
                    // frame is still useful until an edit or seek stops play.
                    if token.generation == self.preview_generation
                        && (token.revision == self.revision || self.playing)
                    {
                        let size = [img.width() as usize, img.height() as usize];
                        let color = egui::ColorImage::from_rgb(size, img.as_raw());
                        // The software backend caches meshes by texture ID,
                        // without invalidating them on pixel replacement.
                        // A fresh handle invalidates the image primitive;
                        // dropping the preceding handle frees its frame data.
                        self.texture = Some(ctx.load_texture(
                            "composite",
                            color,
                            egui::TextureOptions::LINEAR,
                        ));
                        self.preview_revision = token.revision;
                    }
                }
                Event::Exported(path) => {
                    self.message = format!("Video exported: {}", path.display())
                }
                Event::PreviewError(error, token) => {
                    if token.generation == self.preview_generation
                        && (token.revision == self.revision || self.playing)
                    {
                        self.message = error;
                        self.playing = false;
                        self.preview_revision = self.revision;
                    }
                }
                Event::Cancelled => {}
                Event::Error(e) => {
                    self.message = e;
                    self.playing = false;
                    self.preview_revision = self.revision;
                }
            }
            if was_export {
                self.playing = false;
            }
        }
        if let Ok(event) = self.net_rx.try_recv() {
            self.net_busy = false;
            match event {
                NetEvent::Checked(Ok(Some(update))) => {
                    self.update_text = format!("Version {} is available", update.version);
                    self.update = Some(update);
                }
                NetEvent::Checked(Ok(None)) => {
                    self.update = None;
                    self.update_text = "You have the latest stable version".into();
                }
                NetEvent::Checked(Err(e)) => self.update_text = e,
                NetEvent::Downloaded(Ok(staged)) => {
                    self.update_text = format!(
                        "Version {} verified and ready. Save your project before restarting.",
                        staged.version
                    );
                    self.staged = Some(staged);
                }
                NetEvent::Downloaded(Err(e)) => {
                    self.update_text = format!("Update download failed: {e}")
                }
            }
        }
    }
}
impl Editor {
    pub fn update(&mut self, ctx: &egui::Context) {
        let ctx = ctx.clone();
        self.receive(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.worker.blocks_ui() || self.net_busy || self.save_settings {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.message =
                    "Wait for the current operation or cancel the export before closing.".into();
            } else if self.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.discard = Some(Discard::Close);
            }
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S))
            && !self.worker.blocks_ui()
        {
            self.playing = false;
            self.worker.send(
                Job::Save(self.project.clone(), self.path.clone()),
                "Saving project…",
            );
        }
        if self.playing {
            let elapsed = self.last_tick.elapsed();
            if elapsed >= Duration::from_millis(83) {
                self.time += elapsed.as_secs_f64();
                self.last_tick = Instant::now();
                self.revision += 1;
                if self.time >= self.project.duration() {
                    self.time = 0.0;
                    self.playing = false;
                }
            }
            ctx.request_repaint_after(
                Duration::from_millis(83).saturating_sub(self.last_tick.elapsed()),
            );
        }
        egui::TopBottomPanel::top("toolbar").show(&ctx, |ui| self.toolbar(ui, &ctx));
        egui::TopBottomPanel::bottom("status").show(&ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if self.dirty {
                    ui.colored_label(Color32::YELLOW, "Unsaved changes ·");
                }
                ui.label(&self.message);
            });
            if let Some(progress) = &self.progress {
                let n = progress.done.load(Ordering::Relaxed);
                let total = progress.total.load(Ordering::Relaxed).max(1);
                ui.horizontal(|ui| {
                    ui.label(if progress.encoding.load(Ordering::Relaxed) {
                        "Encoding MP4"
                    } else {
                        "Rendering AVI"
                    });
                    ui.add(egui::ProgressBar::new(n as f32 / total as f32).show_percentage());
                    if ui.button("Cancel export").clicked() {
                        progress.cancel();
                    }
                });
                ctx.request_repaint_after(Duration::from_millis(150));
            } else if self.worker.blocks_ui() {
                ui.label(self.worker.foreground_label());
            }
        });
        egui::TopBottomPanel::bottom("timeline")
            .min_height(176.0)
            .show(&ctx, |ui| self.timeline(ui));
        egui::SidePanel::right("properties")
            .default_width(308.0)
            .resizable(true)
            .show(&ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.properties(ui));
            });
        self.canvas(&ctx);
        self.settings_window(&ctx);
        self.export_window(&ctx);
        if let Some(action) = self.discard {
            egui::Window::new("Unsaved project")
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.label("Save your project first, or discard these changes.");
                    if ui.button("Keep editing").clicked() {
                        self.discard = None;
                    }
                    if ui.button("Discard changes").clicked() {
                        self.discard_action(action, &ctx);
                    }
                });
        }
        if !self.worker.busy {
            if self.save_settings {
                self.worker
                    .send(Job::Settings(self.settings.clone()), "Saving settings…");
                self.save_settings = false;
            } else if self.preview_revision != self.revision && !self.project.slides.is_empty() {
                self.worker.send(
                    Job::Preview(self.project.clone(), self.time, self.preview_token()),
                    "Rendering preview…",
                );
            } else if self.project.slides.is_empty() {
                self.texture = None;
            }
        }
    }
    pub fn shutdown(&mut self) {
        if let Some(progress) = &self.progress {
            progress.cancel();
        }
        if let Some(staged) = self.staged.take()
            && !self.installing
        {
            let _ = std::fs::remove_file(staged.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        Editor::with_context(
            egui::Context::default(),
            Settings {
                startup_updates: false,
                ffmpeg: None,
            },
        )
    }
    fn controlled_worker() -> (Worker, Receiver<Job>, SyncSender<Event>) {
        let (tx, requests) = mpsc::sync_channel(1);
        let (responses, rx) = mpsc::sync_channel(1);
        (
            Worker {
                tx,
                rx,
                busy: false,
                previewing: false,
                pending: None,
                label: String::new(),
            },
            requests,
            responses,
        )
    }
    #[test]
    fn explicit_action_waits_for_preview_and_is_dispatched_once() {
        let (mut worker, requests, _responses) = controlled_worker();
        worker.send(
            Job::Preview(
                Project::default(),
                0.0,
                PreviewToken {
                    revision: 1,
                    generation: 1,
                },
            ),
            "Rendering preview…",
        );
        assert!(!worker.blocks_ui());
        assert!(matches!(requests.recv().unwrap(), Job::Preview(..)));
        worker.send(Job::Save(Project::default(), None), "Saving project…");
        assert!(worker.blocks_ui());
        assert_eq!(worker.foreground_label(), "Saving project…");
        assert!(worker.complete());
        assert!(matches!(requests.recv().unwrap(), Job::Save(..)));
        assert!(worker.blocks_ui());
        assert!(!worker.complete());
        assert!(!worker.blocks_ui());
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn queued_export_keeps_progress_when_preview_completes() {
        let ctx = egui::Context::default();
        let mut app = editor();
        let (worker, requests, responses) = controlled_worker();
        app.worker = worker;
        app.worker.send(
            Job::Preview(app.project.clone(), 0.0, app.preview_token()),
            "Rendering preview…",
        );
        assert!(matches!(requests.recv().unwrap(), Job::Preview(..)));
        let progress = Progress::default();
        app.progress = Some(progress.clone());
        app.worker.send(
            Job::Export(app.project.clone(), Format::Avi, None, progress.clone()),
            "Exporting video…",
        );
        responses
            .send(Event::Preview(
                image::RgbImage::new(8, 8),
                app.preview_token(),
            ))
            .unwrap();
        app.receive(&ctx);
        assert!(Arc::ptr_eq(
            &app.progress.as_ref().unwrap().total,
            &progress.total
        ));
        assert!(matches!(requests.recv().unwrap(), Job::Export(..)));
        responses.send(Event::Cancelled).unwrap();
        app.receive(&ctx);
        assert!(app.progress.is_none());
    }
    #[test]
    fn editing_or_seeking_rejects_an_old_preview_after_play_resumes() {
        let ctx = egui::Context::default();
        let mut app = Editor::for_capture(
            ctx.clone(),
            Project {
                slides: vec![Slide::title()],
                ..Project::default()
            },
            "editor",
        )
        .unwrap();
        let (worker, requests, responses) = controlled_worker();
        app.worker = worker;
        let old = app.preview_token();
        app.worker.send(
            Job::Preview(app.project.clone(), 0.0, old),
            "Rendering preview…",
        );
        requests.recv().unwrap();
        let expected = app.texture.as_ref().unwrap().size();
        app.change();
        app.playing = true;
        responses
            .send(Event::Preview(image::RgbImage::new(8, 8), old))
            .unwrap();
        app.receive(&ctx);
        assert_eq!(app.texture.as_ref().unwrap().size(), expected);
        app.worker.send(
            Job::Preview(app.project.clone(), 0.0, old),
            "Rendering preview…",
        );
        requests.recv().unwrap();
        responses
            .send(Event::PreviewError(
                "An obsolete request failed".into(),
                old,
            ))
            .unwrap();
        app.receive(&ctx);
        assert!(
            app.playing,
            "A previous generation's error must not stop the new preview"
        );
        let current = app.preview_token();
        app.worker.send(
            Job::Preview(app.project.clone(), 0.0, current),
            "Rendering preview…",
        );
        requests.recv().unwrap();
        responses
            .send(Event::Preview(image::RgbImage::new(16, 16), current))
            .unwrap();
        app.receive(&ctx);
        assert_eq!(app.texture.as_ref().unwrap().size(), [16, 16]);
        app.revision += 1;
        app.worker.send(
            Job::Preview(app.project.clone(), 0.0, current),
            "Rendering preview…",
        );
        requests.recv().unwrap();
        responses
            .send(Event::Preview(image::RgbImage::new(32, 32), current))
            .unwrap();
        app.receive(&ctx);
        assert_eq!(
            app.texture.as_ref().unwrap().size(),
            [32, 32],
            "Keep a useful delayed frame from the same playback generation"
        );
        app.seek_selection();
        app.playing = true;
        app.worker.send(
            Job::Preview(app.project.clone(), 0.0, current),
            "Rendering preview…",
        );
        requests.recv().unwrap();
        responses
            .send(Event::Preview(image::RgbImage::new(8, 8), current))
            .unwrap();
        app.receive(&ctx);
        assert_eq!(
            app.texture.as_ref().unwrap().size(),
            [32, 32],
            "A seek invalidates an earlier playback generation"
        );
    }
    #[test]
    fn preview_rendering_keeps_surrounding_controls_visually_stable() {
        use egui_software_backend::BufferMutRef;
        let ctx = egui::Context::default();
        let mut second = Slide::title();
        second.caption = "Another favorite moment".into();
        let project = Project {
            slides: vec![Slide::title(), second],
            ..Project::default()
        };
        let mut app = Editor::for_capture(ctx.clone(), project, "editor").unwrap();
        let (requests, _request_receiver) = mpsc::sync_channel(1);
        let (responses, response_receiver) = mpsc::sync_channel(1);
        app.worker = Worker {
            tx: requests,
            rx: response_receiver,
            busy: false,
            previewing: false,
            pending: None,
            label: String::new(),
        };
        app.playing = true;
        let mut renderer = crate::rasterizer::create();
        let mut frame = 0;
        let mut draw = |app: &mut Editor| {
            app.last_tick = Instant::now();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1180.0, 780.0),
                )),
                time: Some(frame as f64 * 0.25),
                ..Default::default()
            };
            frame += 1;
            let output = ctx.run(input, |ctx| app.update(ctx));
            let mut pixels = vec![0u32; 1180 * 780];
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            renderer.render(
                &mut BufferMutRef::new(bytemuck::cast_slice_mut(&mut pixels), 1180, 780),
                &primitives,
                &output.textures_delta,
                output.pixels_per_point,
            );
            pixels
        };
        for _ in 0..3 {
            draw(&mut app);
        }
        let ready = draw(&mut app);
        app.time = 3.1;
        app.revision += 1;
        app.worker.send(
            Job::Preview(app.project.clone(), app.time, app.preview_token()),
            "Rendering preview…",
        );
        let rendering = draw(&mut app);
        let image = Renderer::default()
            .frame(&app.project, app.time, app.project.layout.size(360))
            .unwrap();
        responses
            .send(Event::Preview(image, app.preview_token()))
            .unwrap();
        let completed = draw(&mut app);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".test-output/preview-flicker");
        std::fs::create_dir_all(&root).unwrap();
        for (name, pixels) in [
            ("ready", &ready),
            ("rendering", &rendering),
            ("completed", &completed),
        ] {
            image::RgbImage::from_fn(1180, 780, |x, y| {
                let p = pixels[y as usize * 1180 + x as usize];
                image::Rgb([(p >> 16) as u8, (p >> 8) as u8, p as u8])
            })
            .save(root.join(format!("{name}.png")))
            .unwrap();
        }
        let differences = |pixels: &[u32]| {
            [(0, 0, 1180, 68), (880, 70, 1180, 570), (0, 600, 1180, 780)]
                .into_iter()
                .map(|(left, top, right, bottom)| {
                    (top..bottom)
                        .flat_map(|y| (left..right).map(move |x| y * 1180 + x))
                        .filter(|&i| ready[i] != pixels[i])
                        .count()
                })
                .sum::<usize>()
        };
        assert_eq!(
            differences(&rendering),
            0,
            "Preview work must not dim or change the toolbar, inspector or timeline"
        );
        assert_eq!(
            differences(&completed),
            0,
            "Finishing a preview must not flash surrounding controls"
        );
        assert!(
            (131..477)
                .flat_map(|y| (18..854).map(move |x| y * 1180 + x))
                .any(|i| ready[i] != completed[i]),
            "The video pixels themselves must change"
        );
    }
    #[test]
    fn new_project_resets_selection_and_does_not_start_network() {
        let mut app = editor();
        app.selected = 20;
        app.discard_action(Discard::New, &egui::Context::default());
        assert_eq!(app.selected, 0);
        assert!(!app.net_busy);
        assert!(!app.project.duration().is_sign_negative());
    }
    #[test]
    fn ordinary_close_removes_staged_download_without_installing() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".test-output");
        std::fs::create_dir_all(&root).unwrap();
        let dir = tempfile::tempdir_in(root).unwrap();
        let path = dir.path().join("unused-update.exe");
        std::fs::write(&path, b"synthetic staged data").unwrap();
        let mut app = editor();
        app.staged = Some(Staged {
            path: path.clone(),
            target: dir.path().join("app.exe"),
            version: semver::Version::new(0, 2, 0),
        });
        app.discard_action(Discard::Close, &egui::Context::default());
        app.shutdown();
        assert!(
            !path.exists(),
            "Closing without installing must discard the staged download"
        );
    }
    #[test]
    fn pointer_motion_over_empty_canvas_does_not_require_repainting() {
        let ctx = egui::Context::default();
        // Use the same context for UI/font state and the fixture editor.
        let mut app = Editor::with_context(
            ctx.clone(),
            Settings {
                startup_updates: false,
                ffmpeg: None,
            },
        );
        let mut scene = crate::rasterizer::SceneCache::default();
        for frame in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1180.0, 780.0),
                )),
                time: Some(frame as f64 * 0.25),
                ..Default::default()
            };
            let output = ctx.run(input, |ctx| app.update(ctx));
            scene.changed(
                &output.shapes,
                (1180, 780),
                output.pixels_per_point,
                !output.textures_delta.is_empty(),
            );
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1180.0, 780.0),
            )),
            time: Some(1.25),
            events: vec![egui::Event::PointerMoved(egui::pos2(100.0, 150.0))],
            ..Default::default()
        };
        let output = ctx.run(input, |ctx| app.update(ctx));
        assert!(!scene.changed(
            &output.shapes,
            (1180, 780),
            output.pixels_per_point,
            !output.textures_delta.is_empty()
        ));
    }
}
