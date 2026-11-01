use super::*;
use egui::{
    Align, Button, CornerRadius, FontFamily, FontId, Frame, Layout as UiLayout, Margin, Stroke,
    TextStyle, Vec2,
};

const ACCENT: Color32 = Color32::from_rgb(121, 224, 191);
#[derive(Clone, Copy)]
struct Palette {
    background: Color32,
    surface: Color32,
    border: Color32,
    text: Color32,
    muted: Color32,
    accent: Color32,
    selected: Color32,
    canvas: Color32,
}
fn palette(theme: UiTheme) -> Palette {
    match theme {
        UiTheme::Dark => Palette {
            background: Color32::from_rgb(15, 22, 30),
            surface: Color32::from_rgb(23, 33, 44),
            border: Color32::from_rgb(49, 66, 81),
            text: Color32::from_rgb(234, 241, 245),
            muted: Color32::from_rgb(159, 180, 196),
            accent: ACCENT,
            selected: Color32::from_rgb(29, 63, 61),
            canvas: Color32::from_rgb(8, 14, 21),
        },
        UiTheme::Light => Palette {
            background: Color32::from_rgb(242, 246, 245),
            surface: Color32::WHITE,
            border: Color32::from_rgb(187, 203, 199),
            text: Color32::from_rgb(24, 46, 41),
            muted: Color32::from_rgb(74, 99, 91),
            accent: Color32::from_rgb(15, 111, 84),
            selected: Color32::from_rgb(211, 239, 228),
            canvas: Color32::from_rgb(226, 235, 231),
        },
    }
}

#[derive(Default, PartialEq)]
pub(super) enum InspectorTab {
    #[default]
    Slide,
    Story,
    Music,
}

pub(super) fn configure(ctx: &egui::Context, theme: UiTheme) {
    let colors = palette(theme);
    ctx.set_theme(if theme == UiTheme::Light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    });
    let mut style = (*ctx.style()).clone();
    style.visuals = if theme == UiTheme::Light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    for (kind, size) in [
        (TextStyle::Heading, 23.0),
        (TextStyle::Body, 15.0),
        (TextStyle::Button, 14.0),
        (TextStyle::Small, 12.0),
    ] {
        style
            .text_styles
            .insert(kind, FontId::new(size, FontFamily::Proportional));
    }
    style.spacing.item_spacing = Vec2::new(8.0, 9.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.interact_size = Vec2::new(32.0, 30.0);
    style.visuals.override_text_color = Some(colors.text);
    style.visuals.weak_text_color = Some(colors.muted);
    style.visuals.panel_fill = colors.background;
    style.visuals.window_fill = colors.surface;
    style.visuals.extreme_bg_color = colors.canvas;
    style.visuals.faint_bg_color = colors.background;
    style.visuals.window_stroke = Stroke::new(1.0, colors.border);
    style.visuals.window_corner_radius = CornerRadius::same(12);
    style.visuals.selection.bg_fill = colors.selected;
    style.visuals.selection.stroke = Stroke::new(1.0, colors.accent);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colors.border);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, colors.text);
    style.visuals.widgets.inactive.bg_fill = colors.surface;
    style.visuals.widgets.inactive.weak_bg_fill = colors.surface;
    style.visuals.widgets.hovered.bg_fill = colors.selected;
    style.visuals.widgets.hovered.weak_bg_fill = colors.selected;
    style.visuals.widgets.active.bg_fill = colors.selected;
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = CornerRadius::same(6);
        widget.fg_stroke = Stroke::new(1.0, colors.text);
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
    ui.add(
        egui::Label::new(
            RichText::new(text)
                .small()
                .color(ui.visuals().weak_text_color()),
        )
        .wrap(),
    );
}
fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(5.0);
    ui.label(RichText::new(label).strong());
}

impl Editor {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let language = self.settings.language;
        let t = |key: &'static str| language.text(key);
        let colors = palette(self.settings.theme);
        ui.add_space(5.0);
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("SlideshowMaker").size(20.0).strong());
                note(ui, t("A little movie. A lasting memory."));
            });
            ui.add_space(14.0);
            ui.add_enabled_ui(!self.worker.blocks_ui(), |ui| {
                ui.menu_button(t("Project"), |ui| {
                    if ui.button(t("New project")).clicked() {
                        self.request_discard(Discard::New, ctx);
                        ui.close();
                    }
                    if ui.button(t("Open project…")).clicked() {
                        self.request_discard(Discard::Open, ctx);
                        ui.close();
                    }
                    if ui.button(t("Save as…")).clicked() {
                        self.worker
                            .send(Job::Save(self.project.clone(), None), "Saving project…");
                        self.playing = false;
                        ui.close();
                    }
                });
                if ui
                    .button(t("Save"))
                    .on_hover_text(t("Save project · Ctrl+S"))
                    .clicked()
                {
                    self.worker.send(
                        Job::Save(self.project.clone(), self.path.clone()),
                        "Saving project…",
                    );
                    self.playing = false;
                }
                if ui.button(t("Add photos…")).clicked() {
                    self.worker.send(Job::Import, "Choosing photos…");
                    self.playing = false;
                }
                if ui
                    .add_enabled(self.project.slides.len() < 500, Button::new(t("Add title")))
                    .clicked()
                {
                    let mut slide = Slide::title();
                    slide.caption = t("Our favorite moments").into();
                    self.project.slides.push(slide);
                    self.selected = self.project.slides.len() - 1;
                    self.seek_selection();
                    self.change();
                }
            });
            ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(!self.project.slides.is_empty(), primary(t("Export video…")))
                    .clicked()
                {
                    self.show_export = true;
                    self.playing = false;
                }
                if ui.button(t("Settings")).clicked() {
                    self.show_settings = true;
                }
                if self.update.is_some() {
                    ui.label(
                        RichText::new(t("Update available"))
                            .small()
                            .color(colors.accent),
                    );
                }
            });
        });
        ui.add_space(5.0);
    }

    pub(super) fn properties(&mut self, ui: &mut egui::Ui) {
        let language = self.settings.language;
        let t = |key: &'static str| language.text(key);
        let colors = palette(self.settings.theme);
        ui.add_space(5.0);
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.inspector_tab, InspectorTab::Slide, t("Slide"));
            ui.selectable_value(&mut self.inspector_tab, InspectorTab::Story, t("Story"));
            ui.selectable_value(&mut self.inspector_tab, InspectorTab::Music, t("Music"));
        });
        ui.separator();
        let mut changed = false;
        ui.add_enabled_ui(!self.worker.blocks_ui(), |ui| {
            match self.inspector_tab {
                InspectorTab::Slide => {
                    if let Some(slide) = self.project.slides.get_mut(self.selected) {
                        section(ui, &language.format("Slide {number}", &[("number", (self.selected + 1).to_string())]));
                        ui.add(egui::Label::new(RichText::new(if matches!(slide.media, Media::Title) { t("Title card").into() } else { slide.label() }).small().color(colors.muted)).truncate());
                        section(ui, if matches!(slide.media, Media::Title) { t("Title") } else { t("Caption") });
                        changed |= ui.add(egui::TextEdit::multiline(&mut slide.caption).desired_rows(4).desired_width(f32::INFINITY).char_limit(500).hint_text(t("What makes this moment special?"))).changed();
                        section(ui, t("On screen for"));
                        ui.horizontal_wrapped(|ui| { changed |= ui.add(egui::DragValue::new(&mut slide.seconds).range(0.5..=120.0).speed(0.1).suffix(t(" seconds"))).changed(); });
                        if matches!(slide.media, Media::Photo(_)) {
                            section(ui, t("Photo framing"));
                            ui.horizontal_wrapped(|ui| { changed |= ui.selectable_value(&mut slide.fit, Fit::Fit, t("Fit whole photo")).changed(); changed |= ui.selectable_value(&mut slide.fit, Fit::Fill, t("Fill frame")).changed(); });
                            note(ui, if slide.fit == Fit::Fit { t("Every edge stays in view.") } else { t("The center fills the frame; edges may be cropped.") });
                            if ui.button(t("Rotate 90°")).clicked() { slide.rotation = (slide.rotation + 1) % 4; changed = true; }
                            ui.add_space(12.0);
                            if ui.small_button(t("Relink photo…")).clicked() { self.worker.send(Job::Relink(self.selected), "Choosing replacement photo…"); }
                        }
                    } else {
                        section(ui, t("Tell your story"));
                        ui.label(t("Add some photos, then select a slide below to give it a caption."));
                        note(ui, t("Your originals always stay unchanged."));
                    }
                },
                InspectorTab::Story => {
                    section(ui, t("Story name")); changed |= ui.text_edit_singleline(&mut self.project.name).changed();
                    section(ui, t("Video shape"));
                    egui::ComboBox::from_id_salt("layout").selected_text(match self.project.layout { Layout::Landscape => t("Landscape · 16:9"), Layout::Portrait => t("Portrait · 9:16"), Layout::Square => t("Square · 1:1") }).width(245.0).show_ui(ui, |ui| {
                        for (value,label) in [(Layout::Landscape,t("Landscape · 16:9")),(Layout::Portrait,t("Portrait · 9:16")),(Layout::Square,t("Square · 1:1"))] { changed |= ui.selectable_value(&mut self.project.layout,value,label).changed(); }
                    });
                    section(ui, t("Look & feel"));
                    egui::ComboBox::from_id_salt("theme").selected_text(match self.project.theme {Theme::Midnight=>t("Midnight"),Theme::WarmPaper=>t("Warm paper"),Theme::Ocean=>t("Ocean")}).width(245.0).show_ui(ui, |ui| {
                        for (value,label) in [(Theme::Midnight,t("Midnight")),(Theme::WarmPaper,t("Warm paper")),(Theme::Ocean,t("Ocean"))] {changed |= ui.selectable_value(&mut self.project.theme,value,label).changed();}
                    });
                    section(ui, t("Between slides"));
                    egui::ComboBox::from_id_salt("transition").selected_text(match self.project.transition {Transition::Cut=>t("Simple cut"),Transition::Dissolve=>t("Soft dissolve"),Transition::FadeThroughBackground=>t("Fade through color")}).width(245.0).show_ui(ui,|ui| {
                        for (value,label) in [(Transition::Cut,t("Simple cut")),(Transition::Dissolve,t("Soft dissolve")),(Transition::FadeThroughBackground,t("Fade through color"))] {changed |= ui.selectable_value(&mut self.project.transition,value,label).changed();}
                    });
                    if self.project.transition != Transition::Cut { changed |= ui.add(egui::Slider::new(&mut self.project.transition_seconds,0.0..=2.0).text(t("seconds"))).changed(); }
                    section(ui, t("Fit story to a duration"));
                    ui.horizontal_wrapped(|ui| { ui.add(egui::DragValue::new(&mut self.total_input).range(0.5..=3600.0).speed(0.5).suffix(t(" sec"))); if ui.button(t("Apply")).clicked() {match self.project.set_total(self.total_input){Ok(())=>changed=true,Err(e)=>self.message=e.into()}} });
                    note(ui, t("All slide lengths change proportionally."));
                },
                InspectorTab::Music => {
                    section(ui, t("Add a soundtrack"));
                    if let Some(path)=&self.project.audio {ui.add(egui::Label::new(path.file_name().unwrap_or_default().to_string_lossy()).wrap());} else {ui.label(t("Choose music that belongs to your story."));}
                    if ui.button(t("Choose music…")).clicked() {self.worker.send(Job::Music,"Choosing music…");self.playing=false;}
                    note(ui,"MP3, WAV, FLAC, Ogg, M4A, AAC or AIFF.");
                    if self.project.audio.is_some() {
                        section(ui,t("Volume"));changed |= ui.add(egui::Slider::new(&mut self.project.audio_volume,0.0..=1.0).show_value(false)).changed();
                        changed |= ui.checkbox(&mut self.project.audio_loop,t("Repeat to fill the video")).changed();
                        if ui.small_button(t("Remove soundtrack")).clicked(){self.project.audio=None;changed=true;}
                    }
                    ui.add_space(10.0);note(ui,t("Music is included in your export. Preview is silent."));
                    ui.collapsing(t("Supported audio"),|ui|{note(ui,t("MP3, WAV, FLAC, Ogg Vorbis, M4A (AAC-LC or ALAC), AAC-LC and AIFF. Mono or stereo, 8–96 kHz. Up to 128 MiB per file and 256 MiB after decoding. Protected tracks and HE-AAC are not supported."));});
                },
            }
        });
        if changed {
            self.change();
        }
    }

    pub(super) fn timeline(&mut self, ui: &mut egui::Ui) {
        let language = self.settings.language;
        let t = |key: &'static str| language.text(key);
        let colors = palette(self.settings.theme);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.strong(t("Your story"));
            note(
                ui,
                &language.format(
                    "{count} slides · {seconds} seconds",
                    &[
                        ("count", self.project.slides.len().to_string()),
                        ("seconds", format!("{:.1}", self.project.duration())),
                    ],
                ),
            );
        });
        let mut selected = None;
        let mut action = None;
        egui::ScrollArea::horizontal()
            .max_height(148.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, slide) in self.project.slides.iter().enumerate() {
                        let active = i == self.selected;
                        Frame::new()
                            .fill(if active {
                                colors.selected
                            } else {
                                colors.surface
                            })
                            .stroke(Stroke::new(
                                1.0,
                                if active { colors.accent } else { colors.border },
                            ))
                            .corner_radius(8)
                            .inner_margin(9)
                            .show(ui, |ui| {
                                ui.with_layout(UiLayout::top_down(Align::Min), |ui| {
                                    ui.set_width(205.0);
                                    if ui
                                        .add_sized(
                                            [205.0, 25.0],
                                            egui::Button::new(
                                                RichText::new(format!(
                                                    "{:02}   {} · {:.1}s",
                                                    i + 1,
                                                    if matches!(slide.media, Media::Title) {
                                                        t("Title card")
                                                    } else {
                                                        t("Photo")
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
                                            if matches!(slide.media, Media::Title) {
                                                t("Title card").into()
                                            } else {
                                                slide.label()
                                            }
                                        } else {
                                            slide.caption.clone()
                                        })
                                        .truncate(),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        ui.add_enabled_ui(!self.worker.blocks_ui(), |ui| {
                                            if ui
                                                .add_enabled(i > 0, Button::new(t("Left")).small())
                                                .on_hover_text(t("Move left"))
                                                .clicked()
                                            {
                                                action = Some((i, 0));
                                            }
                                            if ui
                                                .add_enabled(
                                                    i + 1 < self.project.slides.len(),
                                                    Button::new(t("Right")).small(),
                                                )
                                                .on_hover_text(t("Move right"))
                                                .clicked()
                                            {
                                                action = Some((i, 1));
                                            }
                                            if ui.small_button(t("Delete")).clicked() {
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
        self.preview_generation += 1;
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
        let language = self.settings.language;
        let t = |key: &'static str| language.text(key);
        let colors = palette(self.settings.theme);
        egui::CentralPanel::default().frame(Frame::new().fill(colors.background).inner_margin(Margin::same(18))).show(ctx,|ui| {
            ui.horizontal_wrapped(|ui| {ui.heading(&self.project.name);});
            ui.add_space(5.0);
            let available=(ui.available_size()-Vec2::new(0.0,75.0)).max(Vec2::splat(1.0));
            let (rect,_)=ui.allocate_exact_size(available,egui::Sense::hover());
            ui.painter().rect_filled(rect,10,colors.canvas);
            if let Some(texture)=&self.texture {
                let scale=(available.x/texture.size_vec2().x).min(available.y/texture.size_vec2().y);
                let image_rect=egui::Rect::from_center_size(rect.center(),texture.size_vec2()*scale);
                ui.put(image_rect,egui::Image::new(texture).fit_to_exact_size(image_rect.size()));
            } else {
                let center=egui::Rect::from_center_size(rect.center(),Vec2::new(390.0_f32.min(rect.width()-20.0),180.0));
                let mut content=ui.new_child(egui::UiBuilder::new().id_salt("empty-state").max_rect(center).layout(UiLayout::top_down(Align::Center)));
                let ui=&mut content;
                    ui.label(RichText::new(t("Every photo has a story.")).size(23.0).strong());
                    ui.label(t("Bring your favorite moments together with\ncaptions, gentle transitions and music."));ui.add_space(10.0);
                    if ui.add_enabled(!self.worker.blocks_ui(),primary(t("Add your photos…"))).clicked(){self.worker.send(Job::Import,"Choosing photos…");}
                    note(ui,"JPEG · PNG · WebP · BMP");
            }
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.project.slides.is_empty() && self.progress.is_none(),Button::new(if self.playing{t("Pause")}else{t("Play preview")})).clicked(){self.playing = !self.playing;self.last_tick=Instant::now();self.preview_generation+=1;self.revision+=1;}
                let duration=self.project.duration().max(0.001);self.time=self.time.clamp(0.0,duration);
                if ui.add(egui::Slider::new(&mut self.time,0.0..=duration).show_value(false)).changed(){self.revision+=1;self.preview_generation+=1;self.playing=false;}
                note(ui,&format!("{:.1} / {:.1}s",self.time,self.project.duration()));
            });
            note(ui,t("Silent preview · soundtrack included in your export"));
        });
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        let language = self.settings.language;
        let t = |key: &'static str| language.text(key);
        if !self.show_settings {
            return;
        }
        let previous_appearance = (self.settings.language, self.settings.theme);
        let mut open = true;
        egui::Window::new(t("Settings")).id(egui::Id::new("settings-window")).open(&mut open).default_width(500.0).default_pos([300.0,80.0]).max_height(600.0).vscroll(true).collapsible(false).show(ctx,|ui| {
            ui.heading("SlideshowMaker");note(ui,&language.format("Version {version} · Windows x64", &[("version", env!("CARGO_PKG_VERSION").into())]));ui.add_space(8.0);ui.separator();
            section(ui,t("Appearance"));
            egui::Grid::new("appearance-grid").num_columns(2).spacing([14.0, 10.0]).show(ui, |ui| {
                ui.label(t("Language"));
                egui::ComboBox::from_id_salt("interface-language").selected_text(self.settings.language.native_name()).width(180.0).show_ui(ui, |ui| {
                    for language in Language::ALL {
                        ui.selectable_value(&mut self.settings.language, language, language.native_name());
                    }
                });
                ui.end_row();
                ui.label(t("Appearance"));
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.settings.theme, UiTheme::Light, t("Light"));
                    ui.selectable_value(&mut self.settings.theme, UiTheme::Dark, t("Dark"));
                });
                ui.end_row();
            });
            note(ui,t("Changes apply immediately and are saved automatically."));
            ui.add_space(8.0);ui.separator();
            section(ui,t("Updates"));
            if ui.checkbox(&mut self.settings.startup_updates,t("Check for updates on startup")).changed(){self.save_settings=true;}
            if self.save_settings{note(ui,t("Saving preference…"));}
            if ui.add_enabled(!self.net_busy,Button::new(t("Check for updates"))).clicked(){self.check_updates(ctx);}
            if self.update_text.render(language).chars().count()>180 {ui.label(t("Unable to check for updates right now."));ui.collapsing(t("Details"),|ui|{ui.label(self.update_text.render(language));});}else{ui.label(self.update_text.render(language));}
            if self.update.is_some() && self.staged.is_none() && ui.add_enabled(!self.net_busy,primary(t("Download and install"))).clicked(){self.download(ctx);}
            if self.net_busy && let Some(update)=&self.update {ui.add(egui::ProgressBar::new(self.downloaded.load(Ordering::Relaxed) as f32/update.executable_size() as f32).show_percentage());ctx.request_repaint_after(Duration::from_millis(250));}
            if let Some(staged)=&mut self.staged {
                if ui.add_enabled(!self.dirty && !self.worker.blocks_ui() && !self.save_settings,primary(t("Restart to install update"))).clicked(){match updater::restart_install(staged){Ok(())=>{self.allow_close=true;ctx.send_viewport_cmd(egui::ViewportCommand::Close);},Err(e)=>self.update_text=e.into()}}
                if self.dirty{note(ui,t("Save your project before restarting."));}
            }
            ui.add_space(8.0);ui.separator();section(ui,t("MP4 export"));
            ui.label(t("Choose a trusted FFmpeg executable to create MP4 videos. AVI export works without it."));
            if let Some(path)=&self.settings.ffmpeg{ui.add(egui::Label::new(path.display().to_string()).wrap());}else{note(ui,t("No optional engine selected"));}
            if ui.add_enabled(!self.worker.blocks_ui(),Button::new(t("Choose FFmpeg…"))).clicked(){self.worker.send(Job::Ffmpeg,"Choosing FFmpeg…");}
            ui.add_space(10.0);note(ui,t("Your photos stay on this computer."));
        });
        if previous_appearance != (self.settings.language, self.settings.theme) {
            self.save_settings = true;
            self.worker
                .language
                .store(self.settings.language as u8, Ordering::Relaxed);
            if previous_appearance.1 != self.settings.theme {
                configure(ctx, self.settings.theme);
            }
            ctx.request_repaint();
        }
        self.show_settings = open;
    }

    pub(super) fn export_window(&mut self, ctx: &egui::Context) {
        let language = self.settings.language;
        let t = |key: &'static str| language.text(key);
        if !self.show_export {
            return;
        }
        let mut open = true;
        let previous_output = (self.project.height, self.project.fps);
        egui::Window::new(t("Export your story"))
            .id(egui::Id::new("export-window"))
            .open(&mut open)
            .default_width(430.0)
            .min_width(430.0)
            .default_pos([310.0, 140.0])
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.heading(t("Ready to share"));
                note(
                    ui,
                    &language.format(
                        "{count} slides · {seconds} seconds",
                        &[
                            ("count", self.project.slides.len().to_string()),
                            ("seconds", format!("{:.1}", self.project.duration())),
                        ],
                    ),
                );
                ui.add_enabled_ui(!self.worker.blocks_ui(), |ui| {
                    section(ui, t("Video size"));
                    ui.horizontal_wrapped(|ui| {
                        for (h, label) in [
                            (360, t("Small · 360p")),
                            (720, t("HD · 720p")),
                            (1080, t("Full HD")),
                        ] {
                            ui.selectable_value(&mut self.project.height, h, label);
                        }
                    });
                    section(ui, t("File type"));
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut self.format, Format::Avi, "AVI");
                        ui.selectable_value(&mut self.format, Format::Mp4, "MP4");
                    });
                    note(
                        ui,
                        if self.format == Format::Avi {
                            t("Built in · works in desktop players · up to 2 GB")
                        } else {
                            t("Requires the optional FFmpeg engine selected in Settings.")
                        },
                    );
                    ui.collapsing(t("Frame rate"), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for fps in [12, 24, 30] {
                                ui.selectable_value(
                                    &mut self.project.fps,
                                    fps,
                                    language.format("{fps} fps", &[("fps", fps.to_string())]),
                                );
                            }
                        });
                    });
                    ui.add_space(12.0);
                    if ui
                        .add_enabled(
                            !self.project.slides.is_empty()
                                && (self.format == Format::Avi || self.settings.ffmpeg.is_some()),
                            primary(t("Choose destination & export…")),
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
                        t("Choose a new filename. Existing files are never replaced."),
                    );
                });
            });
        self.show_export &= open;
        if previous_output != (self.project.height, self.project.fps) {
            self.change();
        }
    }
}
