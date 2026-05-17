use super::*;
use egui::{
    Align, Button, CornerRadius, FontFamily, FontId, Frame, Layout as UiLayout, Margin, Stroke,
    TextStyle, Vec2,
};

const BACKGROUND: Color32 = Color32::from_rgb(15, 22, 30);
const SURFACE: Color32 = Color32::from_rgb(23, 33, 44);
const BORDER: Color32 = Color32::from_rgb(49, 66, 81);
const TEXT: Color32 = Color32::from_rgb(234, 241, 245);
const MUTED: Color32 = Color32::from_rgb(159, 180, 196);
const ACCENT: Color32 = Color32::from_rgb(121, 224, 191);

#[derive(Default, PartialEq)]
pub(super) enum InspectorTab {
    #[default]
    Slide,
    Story,
    Music,
}

pub(super) fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(23.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.spacing.item_spacing = Vec2::new(8.0, 9.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.interact_size = Vec2::new(32.0, 30.0);
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = Color32::from_rgb(10, 17, 24);
    style.visuals.faint_bg_color = Color32::from_rgb(30, 43, 56);
    style.visuals.window_stroke = Stroke::new(1.0, BORDER);
    style.visuals.window_corner_radius = CornerRadius::same(12);
    style.visuals.selection.bg_fill = Color32::from_rgb(34, 88, 76);
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(34, 48, 62);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(30, 43, 56);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(47, 66, 82);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(42, 60, 76);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(35, 91, 77);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = CornerRadius::same(6);
    }
    ctx.set_style(style);
}
fn primary(label: &str) -> Button<'_> {
    Button::new(
        RichText::new(label)
            .color(Color32::from_rgb(13, 43, 34))
            .strong(),
    )
    .fill(ACCENT)
}
fn note(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(MUTED)).wrap());
}
fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(5.0);
    ui.label(RichText::new(label).strong());
}

impl Editor {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("SlideshowMaker").size(20.0).strong());
                note(ui, "A little movie. A lasting memory.");
            });
            ui.add_space(14.0);
            ui.add_enabled_ui(!self.worker.busy, |ui| {
                ui.menu_button("Project", |ui| {
                    if ui.button("New project").clicked() {
                        self.request_discard(Discard::New, ctx);
                        ui.close();
                    }
                    if ui.button("Open project…").clicked() {
                        self.request_discard(Discard::Open, ctx);
                        ui.close();
                    }
                    if ui.button("Save as…").clicked() {
                        self.worker
                            .send(Job::Save(self.project.clone(), None), "Saving project…");
                        self.playing = false;
                        ui.close();
                    }
                });
                if ui
                    .button("Save")
                    .on_hover_text("Save project · Ctrl+S")
                    .clicked()
                {
                    self.worker.send(
                        Job::Save(self.project.clone(), self.path.clone()),
                        "Saving project…",
                    );
                    self.playing = false;
                }
                if ui.button("Add photos…").clicked() {
                    self.worker.send(Job::Import, "Choosing photos…");
                    self.playing = false;
                }
                if ui
                    .add_enabled(self.project.slides.len() < 500, Button::new("Add title"))
                    .clicked()
                {
                    self.project.slides.push(Slide::title());
                    self.selected = self.project.slides.len() - 1;
                    self.seek_selection();
                    self.change();
                }
            });
            ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(!self.project.slides.is_empty(), primary("Export video…"))
                    .clicked()
                {
                    self.show_export = true;
                    self.playing = false;
                }
                if ui.button("Settings").clicked() {
                    self.show_settings = true;
                }
                if self.update.is_some() {
                    ui.label(RichText::new("Update available").small().color(ACCENT));
                }
            });
        });
        ui.add_space(5.0);
    }

    pub(super) fn properties(&mut self, ui: &mut egui::Ui) {
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.inspector_tab, InspectorTab::Slide, "Slide");
            ui.selectable_value(&mut self.inspector_tab, InspectorTab::Story, "Story");
            ui.selectable_value(&mut self.inspector_tab, InspectorTab::Music, "Music");
        });
        ui.separator();
        let mut changed = false;
        ui.add_enabled_ui(!self.worker.busy, |ui| {
            match self.inspector_tab {
                InspectorTab::Slide => {
                    if let Some(slide) = self.project.slides.get_mut(self.selected) {
                        section(ui, &format!("Slide {}", self.selected + 1));
                        ui.add(egui::Label::new(RichText::new(slide.label()).small().color(MUTED)).truncate());
                        section(ui, if matches!(slide.media, Media::Title) { "Title" } else { "Caption" });
                        changed |= ui.add(egui::TextEdit::multiline(&mut slide.caption).desired_rows(4).desired_width(f32::INFINITY).char_limit(500).hint_text("What makes this moment special?")).changed();
                        section(ui, "On screen for");
                        ui.horizontal(|ui| { changed |= ui.add(egui::DragValue::new(&mut slide.seconds).range(0.5..=120.0).speed(0.1).suffix(" seconds")).changed(); });
                        if matches!(slide.media, Media::Photo(_)) {
                            section(ui, "Photo framing");
                            ui.horizontal(|ui| { changed |= ui.selectable_value(&mut slide.fit, Fit::Fit, "Fit whole photo").changed(); changed |= ui.selectable_value(&mut slide.fit, Fit::Fill, "Fill frame").changed(); });
                            note(ui, if slide.fit == Fit::Fit { "Every edge stays in view." } else { "The center fills the frame; edges may be cropped." });
                            if ui.button("Rotate 90°").clicked() { slide.rotation = (slide.rotation + 1) % 4; changed = true; }
                            ui.add_space(12.0);
                            if ui.small_button("Relink photo…").clicked() { self.worker.send(Job::Relink(self.selected), "Choosing replacement photo…"); }
                        }
                    } else {
                        section(ui, "Tell your story");
                        ui.label("Add some photos, then select a slide below to give it a caption.");
                        note(ui, "Your originals always stay unchanged.");
                    }
                },
                InspectorTab::Story => {
                    section(ui, "Story name"); changed |= ui.text_edit_singleline(&mut self.project.name).changed();
                    section(ui, "Video shape");
                    egui::ComboBox::from_id_salt("layout").selected_text(match self.project.layout { Layout::Landscape => "Landscape · 16:9", Layout::Portrait => "Portrait · 9:16", Layout::Square => "Square · 1:1" }).width(245.0).show_ui(ui, |ui| {
                        for (value,label) in [(Layout::Landscape,"Landscape · 16:9"),(Layout::Portrait,"Portrait · 9:16"),(Layout::Square,"Square · 1:1")] { changed |= ui.selectable_value(&mut self.project.layout,value,label).changed(); }
                    });
                    section(ui, "Look & feel");
                    egui::ComboBox::from_id_salt("theme").selected_text(match self.project.theme {Theme::Midnight=>"Midnight",Theme::WarmPaper=>"Warm paper",Theme::Ocean=>"Ocean"}).width(245.0).show_ui(ui, |ui| {
                        for (value,label) in [(Theme::Midnight,"Midnight"),(Theme::WarmPaper,"Warm paper"),(Theme::Ocean,"Ocean")] {changed |= ui.selectable_value(&mut self.project.theme,value,label).changed();}
                    });
                    section(ui, "Between slides");
                    egui::ComboBox::from_id_salt("transition").selected_text(match self.project.transition {Transition::Cut=>"Simple cut",Transition::Dissolve=>"Soft dissolve",Transition::FadeThroughBackground=>"Fade through color"}).width(245.0).show_ui(ui,|ui| {
                        for (value,label) in [(Transition::Cut,"Simple cut"),(Transition::Dissolve,"Soft dissolve"),(Transition::FadeThroughBackground,"Fade through color")] {changed |= ui.selectable_value(&mut self.project.transition,value,label).changed();}
                    });
                    if self.project.transition != Transition::Cut { changed |= ui.add(egui::Slider::new(&mut self.project.transition_seconds,0.0..=2.0).text("seconds")).changed(); }
                    section(ui, "Fit story to a duration");
                    ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut self.total_input).range(0.5..=3600.0).speed(0.5).suffix(" sec")); if ui.button("Apply").clicked() {match self.project.set_total(self.total_input){Ok(())=>changed=true,Err(e)=>self.message=e}} });
                    note(ui, "All slide lengths change proportionally.");
                },
                InspectorTab::Music => {
                    section(ui, "Add a soundtrack");
                    if let Some(path)=&self.project.audio {ui.add(egui::Label::new(path.file_name().unwrap_or_default().to_string_lossy()).wrap());} else {ui.label("Choose music that belongs to your story.");}
                    if ui.button("Choose WAV…").clicked() {self.worker.send(Job::Music,"Choosing music…");self.playing=false;}
                    if self.project.audio.is_some() {
                        section(ui,"Volume");changed |= ui.add(egui::Slider::new(&mut self.project.audio_volume,0.0..=1.0).show_value(false)).changed();
                        changed |= ui.checkbox(&mut self.project.audio_loop,"Repeat to fill the video").changed();
                        if ui.small_button("Remove soundtrack").clicked(){self.project.audio=None;changed=true;}
                    }
                    ui.add_space(10.0);note(ui,"Music is included in your export. Preview is silent.");
                    ui.collapsing("Supported audio",|ui|{note(ui,"Use an uncompressed 16-bit WAV file, mono or stereo, 8–96 kHz, up to 128 MiB.");});
                },
            }
        });
        if changed {
            self.change();
        }
    }

    pub(super) fn timeline(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.strong("Your story");
            note(
                ui,
                &format!(
                    "{} slides · {:.1} seconds",
                    self.project.slides.len(),
                    self.project.duration()
                ),
            );
        });
        let mut selected = None;
        let mut action = None;
        egui::ScrollArea::horizontal()
            .max_height(116.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, slide) in self.project.slides.iter().enumerate() {
                        let active = i == self.selected;
                        Frame::new()
                            .fill(if active {
                                Color32::from_rgb(29, 63, 61)
                            } else {
                                SURFACE
                            })
                            .stroke(Stroke::new(1.0, if active { ACCENT } else { BORDER }))
                            .corner_radius(8)
                            .inner_margin(9)
                            .show(ui, |ui| {
                                ui.with_layout(UiLayout::top_down(Align::Min), |ui| {
                                    ui.set_width(164.0);
                                    if ui
                                        .add_sized(
                                            [164.0, 25.0],
                                            egui::Button::new(
                                                RichText::new(format!(
                                                    "{:02}   {} · {:.1}s",
                                                    i + 1,
                                                    if matches!(slide.media, Media::Title) {
                                                        "Title card"
                                                    } else {
                                                        "Photo"
                                                    },
                                                    slide.seconds
                                                ))
                                                .strong(),
                                            )
                                            .frame(false),
                                        )
                                        .clicked()
                                    {
                                        selected = Some(i);
                                    }
                                    ui.add(
                                        egui::Label::new(if slide.caption.is_empty() {
                                            slide.label()
                                        } else {
                                            slide.caption.clone()
                                        })
                                        .truncate(),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.add_enabled_ui(!self.worker.busy, |ui| {
                                            if ui
                                                .add_enabled(i > 0, Button::new("Left").small())
                                                .on_hover_text("Move left")
                                                .clicked()
                                            {
                                                action = Some((i, 0));
                                            }
                                            if ui
                                                .add_enabled(
                                                    i + 1 < self.project.slides.len(),
                                                    Button::new("Right").small(),
                                                )
                                                .on_hover_text("Move right")
                                                .clicked()
                                            {
                                                action = Some((i, 1));
                                            }
                                            if ui.small_button("Delete").clicked() {
                                                action = Some((i, 2));
                                            }
                                        });
                                    });
                                });
                            });
                    }
                });
            });
        if let Some(i) = selected {
            self.selected = i;
            self.seek_selection();
            self.revision += 1;
        }
        if let Some((i, a)) = action {
            if a == 2 {
                self.project.slides.remove(i);
                self.selected = i.min(self.project.slides.len().saturating_sub(1));
            } else {
                let to = if a == 0 { i - 1 } else { i + 1 };
                self.project.slides.swap(i, to);
                self.selected = to;
            }
            self.seek_selection();
            self.change();
        }
    }
    pub(super) fn seek_selection(&mut self) {
        self.time = self
            .project
            .slides
            .iter()
            .take(self.selected)
            .map(|s| s.seconds)
            .sum::<f64>()
            .max(0.0);
        self.playing = false;
    }

    pub(super) fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(Frame::new().fill(BACKGROUND).inner_margin(Margin::same(18))).show(ctx,|ui| {
            ui.horizontal(|ui| {ui.heading(&self.project.name);});
            ui.add_space(5.0);
            let available=(ui.available_size()-Vec2::new(0.0,75.0)).max(Vec2::splat(1.0));
            let (rect,_)=ui.allocate_exact_size(available,egui::Sense::hover());
            ui.painter().rect_filled(rect,10,Color32::from_rgb(8,14,21));
            if let Some(texture)=&self.texture {
                let scale=(available.x/texture.size_vec2().x).min(available.y/texture.size_vec2().y);
                let image_rect=egui::Rect::from_center_size(rect.center(),texture.size_vec2()*scale);
                ui.put(image_rect,egui::Image::new(texture).fit_to_exact_size(image_rect.size()));
            } else {
                let center=egui::Rect::from_center_size(rect.center(),Vec2::new(390.0_f32.min(rect.width()-20.0),180.0));
                let mut content=ui.new_child(egui::UiBuilder::new().id_salt("empty-state").max_rect(center).layout(UiLayout::top_down(Align::Center)));
                let ui=&mut content;
                    ui.label(RichText::new("Every photo has a story.").size(23.0).strong());
                    ui.label("Bring your favorite moments together with\ncaptions, gentle transitions and music.");ui.add_space(10.0);
                    if ui.add_enabled(!self.worker.busy,primary("Add your photos…")).clicked(){self.worker.send(Job::Import,"Choosing photos…");}
                    note(ui,"JPEG · PNG · WebP · BMP");
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.project.slides.is_empty() && self.progress.is_none(),Button::new(if self.playing{"Pause"}else{"Play preview"})).clicked(){self.playing = !self.playing;self.last_tick=Instant::now();}
                let duration=self.project.duration().max(0.001);self.time=self.time.clamp(0.0,duration);
                if ui.add(egui::Slider::new(&mut self.time,0.0..=duration).show_value(false)).changed(){self.revision+=1;self.playing=false;}
                note(ui,&format!("{:.1} / {:.1}s",self.time,self.project.duration()));
            });
            note(ui,"Silent preview · soundtrack included in your export");
        });
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        egui::Window::new("Settings").open(&mut open).default_width(470.0).default_pos([330.0,140.0]).collapsible(false).show(ctx,|ui| {
            ui.heading("SlideshowMaker");note(ui,&format!("Version {} · Windows x64",env!("CARGO_PKG_VERSION")));ui.add_space(8.0);ui.separator();
            section(ui,"Updates");
            if ui.checkbox(&mut self.settings.startup_updates,"Check for updates on startup").changed(){self.save_settings=true;}
            if self.save_settings{note(ui,"Saving preference…");}
            if ui.add_enabled(!self.net_busy,Button::new("Check for updates")).clicked(){self.check_updates(ctx);}
            if self.update_text.chars().count()>180 {ui.label("Unable to check for updates right now.");ui.collapsing("Details",|ui|{ui.label(&self.update_text);});}else{ui.label(&self.update_text);}
            if self.update.is_some() && self.staged.is_none() && ui.add_enabled(!self.net_busy,primary("Download and install")).clicked(){self.download(ctx);}
            if self.net_busy && let Some(update)=&self.update {ui.add(egui::ProgressBar::new(self.downloaded.load(Ordering::Relaxed) as f32/update.size as f32).show_percentage());ctx.request_repaint_after(Duration::from_millis(250));}
            if let Some(staged)=&self.staged {
                if ui.add_enabled(!self.dirty && !self.worker.busy && !self.save_settings,primary("Restart to install update")).clicked(){match updater::restart_install(staged){Ok(())=>{self.installing=true;self.allow_close=true;ctx.send_viewport_cmd(egui::ViewportCommand::Close);},Err(e)=>self.update_text=e}}
                if self.dirty{note(ui,"Save your project before restarting.");}
            }
            ui.add_space(8.0);ui.separator();section(ui,"MP4 export");
            ui.label("Choose a trusted FFmpeg executable to create MP4 videos. AVI export works without it.");
            if let Some(path)=&self.settings.ffmpeg{ui.add(egui::Label::new(path.display().to_string()).wrap());}else{note(ui,"No optional engine selected");}
            if ui.add_enabled(!self.worker.busy,Button::new("Choose FFmpeg…")).clicked(){self.worker.send(Job::Ffmpeg,"Choosing FFmpeg…");}
            ui.add_space(10.0);note(ui,"Your photos stay on this computer.");
        });
        self.show_settings = open;
    }

    pub(super) fn export_window(&mut self, ctx: &egui::Context) {
        if !self.show_export {
            return;
        }
        let mut open = true;
        let previous_output = (self.project.height, self.project.fps);
        egui::Window::new("Export your story")
            .open(&mut open)
            .default_width(430.0)
            .min_width(430.0)
            .default_pos([310.0, 140.0])
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.heading("Ready to share");
                note(
                    ui,
                    &format!(
                        "{} slides · {:.1} seconds",
                        self.project.slides.len(),
                        self.project.duration()
                    ),
                );
                ui.add_enabled_ui(!self.worker.busy, |ui| {
                    section(ui, "Video size");
                    ui.horizontal(|ui| {
                        for (h, label) in
                            [(360, "Small · 360p"), (720, "HD · 720p"), (1080, "Full HD")]
                        {
                            ui.selectable_value(&mut self.project.height, h, label);
                        }
                    });
                    section(ui, "File type");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.format, Format::Avi, "AVI");
                        ui.selectable_value(&mut self.format, Format::Mp4, "MP4");
                    });
                    note(
                        ui,
                        if self.format == Format::Avi {
                            "Built in · works in desktop players · up to 2 GB"
                        } else {
                            "Requires the optional FFmpeg engine selected in Settings."
                        },
                    );
                    ui.collapsing("Frame rate", |ui| {
                        ui.horizontal(|ui| {
                            for fps in [12, 24, 30] {
                                ui.selectable_value(
                                    &mut self.project.fps,
                                    fps,
                                    format!("{fps} fps"),
                                );
                            }
                        });
                    });
                    ui.add_space(12.0);
                    if ui
                        .add_enabled(
                            !self.project.slides.is_empty()
                                && (self.format == Format::Avi || self.settings.ffmpeg.is_some()),
                            primary("Choose destination & export…"),
                        )
                        .clicked()
                    {
                        let progress = Progress::default();
                        self.worker.send(
                            Job::Export(
                                self.project.clone(),
                                self.format,
                                self.settings.ffmpeg.clone(),
                                progress.clone(),
                            ),
                            "Exporting video…",
                        );
                        self.progress = Some(progress);
                        self.playing = false;
                        self.show_export = false;
                    }
                    note(
                        ui,
                        "Choose a new filename. Existing files are never replaced.",
                    );
                });
            });
        self.show_export &= open;
        if previous_output != (self.project.height, self.project.fps) {
            self.change();
        }
    }
}
