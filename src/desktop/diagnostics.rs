use super::*;

#[derive(Default, Clone, Copy)]
pub(super) struct Counters {
    pub causes: [u64; 10],
    pub layout: u64,
    pub raster: u64,
    pub present: u64,
    pub input: u64,
    pub layout_ms: f64,
    pub raster_ms: f64,
    pub present_ms: f64,
}
pub(super) struct Diagnostic {
    pub next: Instant,
    stage: u8,
    path: std::path::PathBuf,
    started: Instant,
    cpu: f64,
    baseline: Counters,
    rows: Vec<serde_json::Value>,
    hidden_present: u64,
    passed: bool,
}
impl Diagnostic {
    pub fn completed(&self) -> bool {
        self.stage == 5
    }
    pub fn new(path: std::path::PathBuf) -> Self {
        let now = Instant::now();
        Self {
            next: now + Duration::from_secs(2),
            stage: 0,
            path,
            started: now,
            cpu: cpu_seconds(),
            baseline: Default::default(),
            rows: Vec::new(),
            hidden_present: 0,
            passed: true,
        }
    }
    fn begin(&mut self, counters: Counters) {
        self.started = Instant::now();
        self.cpu = cpu_seconds();
        self.baseline = counters;
    }
    fn record(&mut self, label: &str, c: Counters, visible: bool) {
        let seconds = self.started.elapsed().as_secs_f64();
        let cpu = 100.0 * (cpu_seconds() - self.cpu) / seconds;
        let layout = c.layout - self.baseline.layout;
        let raster = c.raster - self.baseline.raster;
        let present = c.present - self.baseline.present;
        let input = c.input - self.baseline.input;
        let clean = layout == 0
            && raster == 0
            && present == 0
            && cpu < 5.0
            && visible == (label != "hidden");
        self.passed &= clean;
        let causes: std::collections::BTreeMap<_, _> = [
            "CursorMoved",
            "CursorBoundary",
            "MouseButton",
            "Wheel",
            "Keyboard",
            "IME",
            "Focused",
            "MovedOrResized",
            "ModifiersChanged",
            "Other",
        ]
        .into_iter()
        .enumerate()
        .filter_map(|(i, name)| {
            let count = c.causes[i] - self.baseline.causes[i];
            (count > 0).then_some((name, count))
        })
        .collect();
        self.rows.push(serde_json::json!({"state":label,"seconds":seconds,"visible":visible,"layout_frames":layout,"raster_frames":raster,"present_calls":present,"window_events":input,"event_causes":causes,"one_core_cpu_percent":cpu,"machine_cpu_percent":cpu/std::thread::available_parallelism().map_or(1,|n|n.get()) as f64,"layout_ms":c.layout_ms-self.baseline.layout_ms,"raster_ms":c.raster_ms-self.baseline.raster_ms,"present_ms":c.present_ms-self.baseline.present_ms,"clean_sample":clean}));
    }
}
impl Desktop {
    pub(super) fn diagnostic_tick(&mut self, event_loop: &ActiveEventLoop) {
        let Some(d) = &mut self.diagnostic else {
            return;
        };
        if d.completed() {
            return;
        }
        if Instant::now() < d.next {
            return;
        }
        let Some(native) = &self.native else {
            return;
        };
        match d.stage {
            0 => {
                d.begin(self.counters);
                d.stage = 1;
                d.next = Instant::now() + Duration::from_secs(5);
            }
            1 => {
                d.record(
                    "visible_idle",
                    self.counters,
                    native.window.is_visible() == Some(true),
                );
                native.window.set_visible(false);
                d.begin(self.counters);
                d.stage = 2;
                d.next = Instant::now() + Duration::from_secs(5);
            }
            2 => {
                d.record(
                    "hidden",
                    self.counters,
                    native.window.is_visible() == Some(true),
                );
                d.hidden_present = self.counters.present;
                native.window.set_visible(true);
                self.occluded = false;
                self.needs_ui = true;
                native.window.request_redraw();
                d.stage = 3;
                d.next = Instant::now() + Duration::from_secs(2);
            }
            3 => {
                d.begin(self.counters);
                d.stage = 4;
                d.next = Instant::now() + Duration::from_secs(5);
            }
            _ => {
                d.record(
                    "restored_idle",
                    self.counters,
                    native.window.is_visible() == Some(true),
                );
                let restored = self.counters.present > d.hidden_present;
                let passed = d.passed && restored;
                let report = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"executable":std::env::current_exe().ok(),"pid":std::process::id(),"backend":"egui CPU rasterization + softbuffer Win32 GDI","states":d.rows,"restore_presented":restored,"total_layout_frames":self.counters.layout,"total_raster_frames":self.counters.raster,"total_present_calls":self.counters.present,"gpu_telemetry":"unavailable; native pipeline and executable imports inspected separately","perceptual_responsiveness":"not tested without user interaction","passed":passed});
                if let Err(error) = slideshow_maker::storage::atomic_json(&d.path, &report) {
                    self.error = Some(error);
                } else if !passed {
                    self.error=Some("Rendering gate did not obtain clean idle/hidden/restore samples; inspect the diagnostic report".into());
                }
                d.stage = 5;
                event_loop.exit();
            }
        }
    }
}
