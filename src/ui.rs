use eframe::egui::{self, Color32, RichText};
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

enum Job {
    Import,
    Music,
    Relink(usize),
    Ffmpeg,
    Open,
    Save(Project, Option<PathBuf>),
    Settings(Settings),
    Preview(Project, f64, u64),
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
    Preview(image::RgbImage, u64),
    Exported(PathBuf),
    Cancelled,
    Error(String),
}
struct Worker {
    tx: SyncSender<Job>,
    rx: Receiver<Event>,
    busy: bool,
    label: String,
}
impl Worker {
    fn new(ctx: egui::Context) -> Self {
        let (tx, requests) = mpsc::sync_channel(1);
        let (events, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut renderer = Renderer::default();
            while let Ok(job) = requests.recv() {
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
                if events.send(result.unwrap_or_else(Event::Error)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            tx,
            rx,
            busy: false,
            label: String::new(),
        }
    }
    fn send(&mut self, job: Job, label: &str) {
        if !self.busy && self.tx.try_send(job).is_ok() {
            self.busy = true;
            self.label = label.into();
        }
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
    preview_revision: u64,
    worker: Worker,
    texture: Option<egui::TextureHandle>,
    message: String,
    playing: bool,
    last_tick: Instant,
    settings: Settings,
    show_settings: bool,
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
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let (net_tx, net_rx) = mpsc::sync_channel(1);
        let settings = storage::load_settings();
        let mut app = Self {
            project: Project::default(),
            path: None,
            dirty: false,
            selected: 0,
            time: 0.0,
            revision: 1,
            preview_revision: 0,
            worker: Worker::new(cc.egui_ctx.clone()),
            texture: None,
            message: "Add photos or a title card to begin. Source files are never modified.".into(),
            playing: false,
            last_tick: Instant::now(),
            settings,
            show_settings: false,
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
        };
        if app.settings.startup_updates {
            app.check_updates(&cc.egui_ctx);
        }
        app
    }
    fn change(&mut self) {
        self.dirty = true;
        self.revision += 1;
        self.total_input = self.project.duration();
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
                self.path = None;
                self.dirty = false;
                self.time = 0.0;
                self.texture = None;
                self.revision += 1;
                self.total_input = 0.0;
            }
            Discard::Open => self.worker.send(Job::Open, "Opening project…"),
            Discard::Close => {
                self.installing = true;
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn receive(&mut self, ctx: &egui::Context) {
        if let Ok(event) = self.worker.rx.try_recv() {
            self.worker.busy = false;
            let was_export = self.progress.take().is_some();
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
                    self.message = "Project opened".into();
                }
                Event::Saved(path) => {
                    self.path = Some(path);
                    self.dirty = false;
                    self.message = "Project saved".into();
                }
                Event::SettingsSaved => {}
                Event::Preview(img, rev) => {
                    if rev == self.revision {
                        let size = [img.width() as usize, img.height() as usize];
                        let color = egui::ColorImage::from_rgb(size, img.as_raw());
                        if let Some(t) = &mut self.texture {
                            t.set(color, egui::TextureOptions::LINEAR);
                        } else {
                            self.texture = Some(ctx.load_texture(
                                "composite",
                                color,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                        self.preview_revision = rev;
                    }
                }
                Event::Exported(path) => {
                    self.message = format!("Video exported: {}", path.display())
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
    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("SlideshowMaker").color(Color32::from_rgb(126, 220, 198)));
            ui.separator();
            let enabled = !self.worker.busy;
            ui.add_enabled_ui(enabled, |ui| {
                if ui.button("New").clicked() {
                    self.request_discard(Discard::New, ctx);
                }
                if ui.button("Open…").clicked() {
                    self.request_discard(Discard::Open, ctx);
                }
                if ui.button("Save").clicked() {
                    self.worker.send(
                        Job::Save(self.project.clone(), self.path.clone()),
                        "Saving project…",
                    );
                    self.playing = false;
                }
                if ui.button("Save as…").clicked() {
                    self.worker
                        .send(Job::Save(self.project.clone(), None), "Saving project…");
                    self.playing = false;
                }
                ui.separator();
                if ui.button("Add photos…").clicked() {
                    self.worker.send(Job::Import, "Choosing photos…");
                    self.playing = false;
                }
                if ui
                    .add_enabled(
                        self.project.slides.len() < 500,
                        egui::Button::new("Add title"),
                    )
                    .clicked()
                {
                    self.project.slides.push(Slide::title());
                    self.selected = self.project.slides.len() - 1;
                    self.time = self.project.duration() - 3.0;
                    self.change();
                }
            });
            if ui.button("Settings").clicked() {
                self.show_settings = true;
            }
            if self.update.is_some() {
                ui.colored_label(Color32::LIGHT_GREEN, "Update available");
            }
        });
    }
    fn properties(&mut self, ui: &mut egui::Ui) {
        let busy = self.worker.busy;
        ui.heading("Make it yours");
        ui.add_enabled_ui(!busy,|ui| {
            let mut changed=false;
            ui.label("Project name");changed|=ui.text_edit_singleline(&mut self.project.name).changed();
            ui.horizontal(|ui| {ui.label("Layout");egui::ComboBox::from_id_salt("layout").selected_text(format!("{:?}",self.project.layout)).show_ui(ui,|ui| {
                for value in [Layout::Landscape,Layout::Portrait,Layout::Square] {changed|=ui.selectable_value(&mut self.project.layout,value,format!("{value:?}")).changed();}
            });});
            ui.horizontal(|ui| {ui.label("Template");egui::ComboBox::from_id_salt("theme").selected_text(format!("{:?}",self.project.theme)).show_ui(ui,|ui| {
                for value in [Theme::Midnight,Theme::WarmPaper,Theme::Ocean] {changed|=ui.selectable_value(&mut self.project.theme,value,format!("{value:?}")).changed();}
            });});
            egui::ComboBox::from_label("Transition").selected_text(format!("{:?}",self.project.transition)).show_ui(ui,|ui| {
                for value in [Transition::Cut,Transition::Dissolve,Transition::FadeThroughBackground] {changed|=ui.selectable_value(&mut self.project.transition,value,format!("{value:?}")).changed();}
            });
            changed|=ui.add(egui::Slider::new(&mut self.project.transition_seconds,0.0..=2.0).text("Transition sec")).changed();
            ui.separator();ui.strong("Selected slide");
            if let Some(slide)=self.project.slides.get_mut(self.selected) {
                ui.label(slide.label());ui.label("Caption / title (up to 500 characters)");
                changed|=ui.add(egui::TextEdit::multiline(&mut slide.caption).desired_rows(3).char_limit(500)).changed();
                changed|=ui.add(egui::Slider::new(&mut slide.seconds,0.5..=120.0).logarithmic(true).text("Seconds")).changed();
                if matches!(slide.media,Media::Photo(_)) {
                    ui.horizontal(|ui| {changed|=ui.selectable_value(&mut slide.fit,Fit::Fit,"Fit (whole photo)").changed();changed|=ui.selectable_value(&mut slide.fit,Fit::Fill,"Fill (crop)").changed();});
                    ui.horizontal(|ui| {
                        if ui.button("Rotate 90°").clicked() {slide.rotation=(slide.rotation+1)%4;changed=true;}
                        if ui.button("Relink photo…").clicked() {self.worker.send(Job::Relink(self.selected),"Choosing replacement photo…");}
                    });
                }
            }else{ui.label("Add a photo or title card.");}
            ui.separator();ui.strong("Music");
            if let Some(path)=&self.project.audio {ui.label(path.file_name().unwrap_or_default().to_string_lossy());}
            ui.horizontal(|ui| {
                if ui.button("Choose WAV…").clicked() {self.worker.send(Job::Music,"Choosing music…");self.playing=false;}
                if self.project.audio.is_some() && ui.button("Remove").clicked() {self.project.audio=None;changed=true;}
            });
            changed|=ui.add(egui::Slider::new(&mut self.project.audio_volume,0.0..=1.0).text("Volume")).changed();
            changed|=ui.checkbox(&mut self.project.audio_loop,"Repeat music to fill video").changed();
            ui.small("PCM 16-bit WAV, mono/stereo. Export includes fades. Preview is silent.");
            ui.separator();ui.strong("Export");
            ui.horizontal(|ui| {ui.label("Height");for h in [360,720,1080] {changed|=ui.selectable_value(&mut self.project.height,h,h.to_string()).changed();}});
            ui.horizontal(|ui| {ui.label("FPS");for f in [12,24,30] {changed|=ui.selectable_value(&mut self.project.fps,f,f.to_string()).changed();}});
            ui.horizontal(|ui| {ui.selectable_value(&mut self.format,Format::Avi,"AVI (built in)");ui.selectable_value(&mut self.format,Format::Mp4,"MP4 (FFmpeg)");});
            if self.format==Format::Mp4 && self.settings.ffmpeg.is_none(){ui.colored_label(Color32::YELLOW,"Choose FFmpeg in Settings for MP4.");}
            if ui.add_enabled(!self.project.slides.is_empty() && (self.format==Format::Avi || self.settings.ffmpeg.is_some()),egui::Button::new("Export video…")).clicked() {
                let progress=Progress::default();self.worker.send(Job::Export(self.project.clone(),self.format,self.settings.ffmpeg.clone(),progress.clone()),"Exporting video…");self.progress=Some(progress);self.playing=false;
            }
            ui.small("AVI uses MJPEG and PCM audio, with a 2 GB limit. Existing videos are never overwritten.");
            if changed {self.change();}
        });
    }
    fn timeline(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong(format!(
                "Timeline · {} slides · {:.1}s",
                self.project.slides.len(),
                self.project.duration()
            ));
            ui.add_enabled_ui(!self.worker.busy, |ui| {
                ui.label("Total seconds");
                ui.add(
                    egui::DragValue::new(&mut self.total_input)
                        .range(0.5..=3600.0)
                        .speed(0.5),
                );
                if ui.button("Apply total").clicked() {
                    match self.project.set_total(self.total_input) {
                        Ok(()) => self.change(),
                        Err(e) => self.message = e,
                    }
                }
            });
        });
        let mut select = None;
        let mut action = None;
        let mut start = 0.0;
        egui::ScrollArea::horizontal()
            .max_height(112.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, slide) in self.project.slides.iter().enumerate() {
                        ui.group(|ui| {
                            ui.set_min_width(156.0);
                            ui.set_max_width(156.0);
                            if ui
                                .selectable_label(
                                    i == self.selected,
                                    format!("{} · {:.1}s", i + 1, slide.seconds),
                                )
                                .clicked()
                            {
                                select = Some((i, start));
                            }
                            ui.label(egui::RichText::new(slide.label()).small());
                            ui.horizontal(|ui| {
                                ui.add_enabled_ui(!self.worker.busy, |ui| {
                                    if ui.add_enabled(i > 0, egui::Button::new("←")).clicked() {
                                        action = Some((i, 0));
                                    }
                                    if ui
                                        .add_enabled(
                                            i + 1 < self.project.slides.len(),
                                            egui::Button::new("→"),
                                        )
                                        .clicked()
                                    {
                                        action = Some((i, 1));
                                    }
                                    if ui.button("Delete").clicked() {
                                        action = Some((i, 2));
                                    }
                                });
                            });
                        });
                        start += slide.seconds;
                    }
                });
            });
        if let Some((i, t)) = select {
            self.selected = i;
            self.time = t;
            self.playing = false;
            self.revision += 1;
        }
        if let Some((i, a)) = action {
            match a {
                0 => {
                    self.project.slides.swap(i, i - 1);
                    self.selected = i - 1;
                }
                1 => {
                    self.project.slides.swap(i, i + 1);
                    self.selected = i + 1;
                }
                _ => {
                    self.project.slides.remove(i);
                    self.selected = self
                        .selected
                        .min(self.project.slides.len().saturating_sub(1));
                }
            }
            self.change();
        }
    }
    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        egui::Window::new("Settings · About & updates").open(&mut open).default_width(520.0).show(ctx,|ui| {
            ui.heading(format!("SlideshowMaker {}",env!("CARGO_PKG_VERSION")));
            ui.label("Windows x64 · local processing · no bundled photos or music");
            if ui.checkbox(&mut self.settings.startup_updates,"Check for updates on startup").changed(){self.save_settings=true;}
            if self.save_settings {ui.small("Settings change pending save…");}
            if ui.add_enabled(!self.net_busy,egui::Button::new("Check for updates")).clicked(){self.check_updates(ctx);}
            ui.label(&self.update_text);
            if self.update.is_some() && self.staged.is_none() && ui.add_enabled(!self.net_busy,egui::Button::new("Download and install")).clicked(){self.download(ctx);}
            if self.net_busy && let Some(update)=&self.update {ui.add(egui::ProgressBar::new(self.downloaded.load(Ordering::Relaxed) as f32/update.size as f32).show_percentage());ctx.request_repaint_after(Duration::from_millis(250));}
            if let Some(staged)=&self.staged {
                if ui.add_enabled(!self.dirty && !self.worker.busy,egui::Button::new("Restart to install verified update")).clicked() {
                    match updater::restart_install(staged){Ok(())=>{self.allow_close=true;ctx.send_viewport_cmd(egui::ViewportCommand::Close);},Err(e)=>self.update_text=e}
                }
                if self.dirty{ui.label("Save your project before installing the update.");}
            }
            ui.separator();ui.strong("Optional MP4 engine");
            ui.label("Use your own trusted FFmpeg executable with MPEG-4 and AAC encoders. No engine is bundled or downloaded automatically.");
            if let Some(path)=&self.settings.ffmpeg {ui.label(path.display().to_string());}
            if ui.add_enabled(!self.worker.busy,egui::Button::new("Choose FFmpeg executable…")).clicked(){self.worker.send(Job::Ffmpeg,"Choosing FFmpeg…");}
            ui.small("Clip import, MP3 music, 3D effects and sound playback are not supported in this first version.");
        });
        self.show_settings = open;
    }
}
impl eframe::App for Editor {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.worker.busy || self.net_busy {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.message =
                    "Wait for the current operation or cancel the export before closing.".into();
            } else if self.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.discard = Some(Discard::Close);
            }
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) && !self.worker.busy {
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
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui, &ctx));
        egui::Panel::bottom("status").show(ui, |ui| {
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
            } else if self.worker.busy && !self.worker.label.starts_with("Rendering preview") {
                ui.label(&self.worker.label);
            }
        });
        egui::Panel::bottom("timeline")
            .min_size(132.0)
            .show(ui, |ui| self.timeline(ui));
        egui::Panel::right("properties")
            .default_size(302.0)
            .resizable(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.properties(ui));
            });
        egui::CentralPanel::default().show(ui,|ui| {
            ui.horizontal(|ui|{ui.heading(&self.project.name);ui.label("Photo story");});
            let available=ui.available_size()-egui::vec2(0.0,68.0);
            if let Some(texture)=&self.texture {
                let size=texture.size_vec2();let scale=(available.x/size.x).min(available.y/size.y).max(0.01);
                ui.vertical_centered(|ui| {ui.add(egui::Image::new(texture).fit_to_exact_size(size*scale));});
            }else{
                ui.allocate_ui(available.max(egui::vec2(1.0,1.0)),|ui|{ui.centered_and_justified(|ui|{ui.label("Your memories start here\nAdd photos, arrange them, and tell your story.");});});
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.project.slides.is_empty() && self.progress.is_none(),egui::Button::new(if self.playing{"Pause"}else{"Play silent preview"})).clicked(){self.playing = !self.playing;self.last_tick=Instant::now();}
                let duration=self.project.duration().max(0.001);self.time=self.time.clamp(0.0,duration);
                if ui.add(egui::Slider::new(&mut self.time,0.0..=duration).show_value(false)).changed(){self.revision+=1;self.playing=false;}
                ui.label(format!("{:.1} / {:.1}s",self.time,self.project.duration()));
            });
            ui.small("Preview uses the same compositor as export. Music is included only in the exported video.");
        });
        self.settings_window(&ctx);
        if let Some(action) = self.discard {
            egui::Window::new("Unsaved project")
                .collapsible(false)
                .resizable(false)
                .show(ui, |ui| {
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
                    Job::Preview(self.project.clone(), self.time, self.revision),
                    "Rendering preview…",
                );
            } else if self.project.slides.is_empty() {
                self.texture = None;
            }
        }
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
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
