//! Multi-viewport egui host for overlay panels.

use crate::chrome::color_with_alpha;
use crate::config::{ConfigContext, WIDGET_KEYS};
use crate::layered;
use crate::settings;
use crate::state::{fit_panel_size, PanelLayout, StateHandle};
use crate::sysstats::SysStats;
use crate::telemetry::{
    demo::DemoFeed, finalize_frame, FuelBurnTracker, IrsdkReader, LapCompareState, LapExtras,
    LapLogAccum, PitStopTracker, RelativeOrderHysteresis, SectorTimer, TelemetryFrame,
};
use crate::widgets::{self, MapPaintMode, WidgetCtx};
use crate::win_click;
use eframe::egui::{
    self, CornerRadius, Pos2, Rect, Sense, Stroke, ViewportBuilder, ViewportCommand, ViewportId,
};
use eframe::glow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MIN_PANEL_W: i32 = 90;
const MIN_PANEL_H: i32 = 44;
const RESIZE_GRIP: f32 = 28.0;
/// Frames to keep applying Visible(false) so macOS/eframe actually drops the window.
const HIDE_RETRY_FRAMES: u8 = 8;
/// Other panels while map composite is hot: keep them rare so ULW budget stays on cars.
const NON_MAP_WHEN_MAP_HOT_MS: u128 = 250;
/// Other panels: throttle Windows glReadPixels + present (~10 Hz idle).
const NON_MAP_READBACK_MS: u128 = 100;
/// Any panel with active easing / blink. Host frame costs ~10-13 ms, so a
/// 16 ms post-frame wait only produced ~35-40 FPS.
const PANEL_ANIM_READBACK_MS: u128 = 8;
/// Map cold/readback cadence (~60 Hz target).
const MAP_READBACK_MS: u128 = 16;
/// Hot map present cadence (~30 Hz, Python `_anim_min_interval_ms`).
/// Every-tick presents (rev 22) hit ~110ms/s ULW without fixing motion stutter.
const MAP_HOT_PRESENT_MS: u128 = 33;
/// If the previous host frame exceeded this, skip non-map paints this tick.
const HEAVY_FRAME_MS: f64 = 40.0;
/// Require this many consecutive absent frames before farewell (avoids config flicker).
const HIDE_DEBOUNCE_FRAMES: u8 = 3;

struct ResizeDrag {
    key: String,
    origin_w: i32,
    origin_h: i32,
    start: Pos2,
}

/// Absolute screen-pointer drag so window moves don't fight pointer.delta().
#[derive(Clone)]
struct PanelDrag {
    key: String,
    origin_x: f32,
    origin_y: f32,
    pointer_origin: Pos2,
}

#[derive(Default)]
struct PerfAccum {
    frames: u32,
    frame_ms_sum: f64,
    panel_slots: u32,
    readbacks: u32,
    presents: u32,
    rb_map_ms: f64,
    rb_other_ms: f64,
    ulw_ms: f64,
    map_dt_sum: f64,
    map_dt_n: u32,
    paint_on: u32,
    paint_off: u32,
}

pub struct OverlayApp {
    state: StateHandle,
    /// Always available; used for `--demo` and as fallback when iRacing is down.
    demo: DemoFeed,
    /// `None` when launched with `--demo` (skip IR entirely).
    irsdk: Option<IrsdkReader>,
    demo_only: bool,
    sysstats: SysStats,
    last_tick: Instant,
    open_viewports: HashSet<String>,
    /// Disabled panels: farewell Visible(false) frames until the OS window dies.
    pending_hide: HashMap<String, u8>,
    /// Consecutive frames a panel was absent from still_open (hide debounce).
    missing_frames: HashMap<String, u8>,
    /// Active panel move in edit mode.
    dragging: Arc<Mutex<Option<PanelDrag>>>,
    /// Active SE-corner resize in edit mode.
    resizing: Arc<Mutex<Option<ResizeDrag>>>,
    last_click_through: HashMap<String, bool>,
    /// Last applied viewport geometry (x, y, w, h) — avoid builder churn.
    last_geom: HashMap<String, (i32, i32, i32, i32)>,
    /// Per-panel GL→BGRA scratch buffers (Windows layered present).
    readback: HashMap<String, crate::layered::ReadbackScratch>,
    /// Map-only pipelined readback (avoids sync glReadPixels on the hot path).
    map_pipe: crate::layered::MapReadbackPipe,
    /// Cached GDI DIB surfaces per HWND.
    present_cache: crate::layered::PresentCache,
    /// Last presented buffer hash per HWND — skip unchanged UpdateLayeredWindow.
    last_present_hash: HashMap<isize, u64>,
    /// Last successful readback per panel (throttle non-map Windows present).
    last_readback: HashMap<String, Instant>,
    /// Previous paint's table-animating flag (drives table present cadence).
    panel_animating: HashMap<String, bool>,
    /// Shared monotonic clock (demo telem + map easing).
    clock_start: Instant,
    gl: Option<Arc<glow::Context>>,
    sector_timer: SectorTimer,
    lap_compare: LapCompareState,
    lap_log: LapLogAccum,
    fuel_burn: FuelBurnTracker,
    pit_stops: PitStopTracker,
    /// Settings form dirty flag (unsaved edits).
    settings_dirty: bool,
    /// Autosave debounce after edits.
    settings_autosave_at: Option<Instant>,
    /// Ephemeral Settings UI chrome state.
    settings_ui: settings::SettingsUi,
    /// System tray (icon + menu + command channel). Kept alive for process lifetime.
    tray: Option<crate::shell::TrayHandle>,
    activate_peer: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Last car/league seen for preset auto-switch (avoid thrashing).
    last_switch_car: Option<String>,
    last_switch_league: Option<i32>,
    /// `--perf`: once-per-second stderr frame/readback summary.
    perf: bool,
    perf_emit_at: Instant,
    perf_acc: PerfAccum,
    perf_last_map_paint_secs: f64,
    /// Sticky Relative EstTime order (live noise hysteresis).
    rel_order: RelativeOrderHysteresis,
    /// Last Relative/Standings focus car (player or spectated) — reset sticky on change.
    last_table_focus: Option<i32>,
    /// Previous update() duration (ms) — defer sibling paints when heavy.
    last_frame_ms: f64,
    /// Last panel_style string per widget — detect Elegant/Data switches for content fit.
    last_panel_style: HashMap<String, String>,
    /// Full-res static map BGRA cache (no cars) for CPU composite presents.
    map_bg: Option<(i32, i32, Vec<u8>)>,
    /// Half-res downsample of `map_bg` (kept for capture; hot ULW uses full + dirty).
    map_bg_half: Option<(i32, i32, Vec<u8>)>,
    map_bg_fp: u64,
    /// `pixels_per_point` at the time `map_bg` was captured (DPI align).
    map_bg_ppp: f32,
    /// StaticOnly paint kicked; next ready PBO becomes `map_bg`.
    map_capturing_bg: bool,
    /// Previous edit-mode flag — invalidate map bg when leaving edit (composite path).
    last_edit_mode: bool,
    /// Prior frame car sprites for dirty-rect ULW (rev 20).
    map_prev_sprites: Vec<crate::layered::MapCarSprite>,
    /// Per-car dirty rects for next map present (empty = full frame).
    map_ulw_dirty: Vec<(i32, i32, i32, i32)>,
}

impl OverlayApp {
    pub fn new(
        state: StateHandle,
        demo: bool,
        perf: bool,
        gl: Option<Arc<glow::Context>>,
        tray: Option<crate::shell::TrayHandle>,
        activate_peer: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let mut lap_log = LapLogAccum::new();
        if demo {
            lap_log.seed_demo(12);
        }
        Self {
            state,
            demo: DemoFeed::new(),
            irsdk: if demo { None } else { Some(IrsdkReader::new()) },
            demo_only: demo,
            sysstats: SysStats::new(),
            last_tick: Instant::now(),
            open_viewports: HashSet::new(),
            pending_hide: HashMap::new(),
            missing_frames: HashMap::new(),
            last_click_through: HashMap::new(),
            last_geom: HashMap::new(),
            readback: HashMap::new(),
            map_pipe: crate::layered::MapReadbackPipe::default(),
            present_cache: crate::layered::PresentCache::default(),
            last_present_hash: HashMap::new(),
            last_readback: HashMap::new(),
            panel_animating: HashMap::new(),
            clock_start: Instant::now(),
            dragging: Arc::new(Mutex::new(None)),
            resizing: Arc::new(Mutex::new(None)),
            gl,
            sector_timer: SectorTimer::new(),
            lap_compare: LapCompareState::new(),
            lap_log,
            fuel_burn: FuelBurnTracker::default(),
            pit_stops: PitStopTracker::default(),
            settings_dirty: false,
            settings_autosave_at: None,
            settings_ui: settings::SettingsUi::default(),
            tray,
            activate_peer,
            last_switch_car: None,
            last_switch_league: None,
            perf,
            perf_emit_at: Instant::now(),
            perf_acc: PerfAccum::default(),
            perf_last_map_paint_secs: 0.0,
            rel_order: RelativeOrderHysteresis::default(),
            last_table_focus: None,
            last_frame_ms: 0.0,
            last_panel_style: HashMap::new(),
            map_bg: None,
            map_bg_half: None,
            map_bg_fp: 0,
            map_bg_ppp: 1.0,
            map_capturing_bg: false,
            last_edit_mode: false,
            map_prev_sprites: Vec::new(),
            map_ulw_dirty: Vec::new(),
        }
    }

    fn clear_map_bg(&mut self) {
        self.map_bg = None;
        self.map_bg_half = None;
        self.map_prev_sprites.clear();
        self.map_ulw_dirty.clear();
    }

    fn set_map_bg(&mut self, w: i32, h: i32, bgra: Vec<u8>, ppp: f32) {
        self.map_bg_half = layered::downsample_bgra_half(&bgra, w, h);
        self.map_bg = Some((w, h, bgra));
        self.map_bg_ppp = ppp;
    }

    fn perf_maybe_emit(&mut self) {
        if !self.perf || self.perf_emit_at.elapsed() < Duration::from_secs(1) {
            return;
        }
        let acc = &self.perf_acc;
        if acc.frames == 0 {
            self.perf_emit_at = Instant::now();
            return;
        }
        let fps = acc.frames as f64;
        let frame_ms = acc.frame_ms_sum / fps;
        let map_dt = if acc.map_dt_n > 0 {
            acc.map_dt_sum / acc.map_dt_n as f64
        } else {
            0.0
        };
        let (track_id, sf, reverse, sess) = {
            let st = self.state.read();
            (
                st.frame.track_id.unwrap_or(-1),
                st.map.cached_start_finish,
                st.config.bool_key("map", "reverse_path", false),
                st.frame.session_state,
            )
        };
        eprintln!(
            "perf fps={fps:.0} frame={frame_ms:.1}ms readbacks={}/{} paints={}/{} (map={:.1}ms other={:.1}ms) presents={} ulw={:.1}ms map_dt={map_dt:.3} map_motion_rev={} track_id={} sf={:.3} rev={} sess={}",
            acc.readbacks,
            acc.panel_slots,
            acc.paint_on,
            acc.paint_off,
            acc.rb_map_ms,
            acc.rb_other_ms,
            acc.presents,
            acc.ulw_ms,
            crate::widgets::MAP_MOTION_REV,
            track_id,
            sf,
            if reverse { 1 } else { 0 },
            sess,
        );
        self.perf_acc = PerfAccum::default();
        self.perf_emit_at = Instant::now();
    }

    fn perf_record_map_dt(&mut self) {
        if !self.perf {
            return;
        }
        let paint_secs = self.state.read().map.last_paint_secs;
        if self.perf_last_map_paint_secs > 0.0 {
            let dt = paint_secs - self.perf_last_map_paint_secs;
            if dt > 0.0 {
                self.perf_acc.map_dt_sum += dt;
                self.perf_acc.map_dt_n += 1;
            }
        }
        self.perf_last_map_paint_secs = paint_secs;
    }

    fn poll_shell_commands(&mut self) {
        use crate::shell::TrayCommand;
        use std::sync::atomic::Ordering;

        if self.activate_peer.swap(false, Ordering::SeqCst) {
            if let Some(mut st) = self.state.try_write() {
                st.settings_open = true;
            }
        }
        // Launch-time update check (background thread) → Settings About.
        if let Some(mut st) = self.state.try_write() {
            if let Some((ver, url)) = st.pending_update.take() {
                self.settings_ui.flash(format!("Update available: {ver}"));
                if let Some(u) = url {
                    self.settings_ui.update_url = Some(u);
                }
                st.settings_open = true;
                st.settings_section = "__app__".into();
                self.settings_ui.top_tab = settings::TopTab::Settings;
            }
        }
        if let Some(tray) = &self.tray {
            for cmd in tray.poll() {
                match cmd {
                    TrayCommand::Settings => {
                        if let Some(mut st) = self.state.try_write() {
                            st.settings_open = true;
                            if st.settings_section == "__scan__" {
                                st.settings_section = "__general__".into();
                            }
                        }
                        self.settings_ui.top_tab = settings::TopTab::Settings;
                    }
                    TrayCommand::TrackScan => {
                        if let Some(mut st) = self.state.try_write() {
                            st.settings_open = true;
                            st.settings_section = "__scan__".into();
                        }
                        self.settings_ui.top_tab = settings::TopTab::Settings;
                    }
                    TrayCommand::ToggleOverlay => {
                        if let Some(mut st) = self.state.try_write() {
                            st.running = !st.running;
                            self.settings_ui.flash(if st.running {
                                "Overlay started"
                            } else {
                                "Overlay stopped"
                            });
                        }
                    }
                    TrayCommand::ToggleEdit => {
                        if let Some(mut st) = self.state.try_write() {
                            st.edit_mode = !st.edit_mode;
                            self.settings_ui.flash(if st.edit_mode {
                                "Edit layout on"
                            } else {
                                "Edit layout off"
                            });
                        }
                    }
                    TrayCommand::CheckUpdates => match crate::updater::fetch_latest(6) {
                        Ok(Some(info)) => {
                            if crate::updater::is_newer(&info.version, crate::updater::VERSION) {
                                self.settings_ui
                                    .flash(format!("Update available: {}", info.version));
                                if let Some(url) = info.url {
                                    self.settings_ui.update_url = Some(url);
                                }
                                if let Some(mut st) = self.state.try_write() {
                                    st.settings_open = true;
                                    st.settings_section = "__app__".into();
                                }
                                self.settings_ui.top_tab = settings::TopTab::Settings;
                            } else {
                                self.settings_ui.flash("You're up to date");
                            }
                        }
                        Ok(None) => self.settings_ui.flash("Update check disabled (dev build)"),
                        Err(e) => self.settings_ui.flash(e.to_string()),
                    },
                    TrayCommand::Quit => {
                        if let Some(mut st) = self.state.try_write() {
                            st.running = false;
                            st.settings_open = false;
                            st.quit_requested = true;
                        }
                    }
                }
            }
        }
    }

    /// One empty frame with Visible(false) so egui/eframe tear down the OS window.
    fn farewell_viewport(ctx: &egui::Context, key: &str) {
        let vid = ViewportId::from_hash_of(key);
        let title = win_click::panel_title(key);
        let builder = ViewportBuilder::default()
            .with_title(title)
            .with_decorations(false)
            .with_transparent(true)
            .with_visible(false)
            .with_taskbar(false);
        ctx.show_viewport_immediate(vid, builder, |vp_ctx, _class| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
                .show(vp_ctx, |_| {});
            vp_ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            vp_ctx.send_viewport_cmd(ViewportCommand::Close);
        });
    }

    fn paint_settings_viewport(&mut self, ctx: &egui::Context) {
        let open = self.state.read().settings_open;
        let vid = ViewportId::from_hash_of("settings");
        if !open {
            // Farewell if we previously showed it — cheap no-op when never opened.
            return;
        }

        let size = settings::default_size();
        let mut builder = ViewportBuilder::default()
            .with_title(settings::window_title())
            .with_inner_size(size)
            .with_min_inner_size(egui::vec2(720.0, 560.0))
            .with_decorations(true)
            .with_transparent(false)
            .with_visible(true)
            .with_taskbar(true);
        if let Some(icon) = crate::app_icon::egui_icon() {
            builder = builder.with_icon(icon);
        }

        let state = self.state.clone();
        let mut section = {
            let st = state.read();
            st.settings_section.clone()
        };
        let mut dirty = self.settings_dirty;
        let mut ui_state = self.settings_ui.clone();

        ctx.show_viewport_immediate(vid, builder, |vp_ctx, _class| {
            settings::apply_viewport_theme(vp_ctx);
            egui::CentralPanel::default().show(vp_ctx, |ui| {
                settings::paint(ui, &state, &mut ui_state, &mut section, &mut dirty);
            });
            if vp_ctx.input(|i| i.viewport().close_requested()) {
                if let Some(mut st) = state.try_write() {
                    st.settings_open = false;
                    let to_tray = st
                        .config
                        .cfg
                        .get("close_settings_to_tray")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    if !to_tray {
                        // Closing Settings exits the app when tray-minimize is off.
                        if st.settings_auto_save {
                            let context = st.effective_context();
                            let _ = Arc::make_mut(&mut st.config).save_for_context(context);
                        }
                        st.running = false;
                        st.quit_requested = true;
                    }
                }
            }
        });

        let auto_save = self.state.read().settings_auto_save;
        if dirty && !self.settings_dirty && auto_save {
            self.settings_autosave_at =
                Some(Instant::now() + std::time::Duration::from_millis(400));
        }
        self.settings_dirty = dirty;
        self.settings_ui = ui_state;
        if let Some(mut st) = self.state.try_write() {
            st.settings_section = section;
        }

        if self.settings_dirty && auto_save {
            if let Some(at) = self.settings_autosave_at {
                if Instant::now() >= at {
                    if let Some(mut st) = self.state.try_write() {
                        let context = st.effective_context();
                        if let Err(e) = Arc::make_mut(&mut st.config).save_for_context(context) {
                            eprintln!("settings autosave: {e}");
                        } else {
                            self.settings_dirty = false;
                        }
                    }
                    self.settings_autosave_at = None;
                }
            }
        }
    }

    fn run_pending_hides(&mut self, ctx: &egui::Context) {
        let keys: Vec<String> = self.pending_hide.keys().cloned().collect();
        for key in keys {
            Self::farewell_viewport(ctx, &key);
            let done = match self.pending_hide.get_mut(&key) {
                Some(n) => {
                    *n = n.saturating_sub(1);
                    *n == 0
                }
                None => true,
            };
            if done {
                self.pending_hide.remove(&key);
                self.missing_frames.remove(&key);
                self.last_click_through.remove(&key);
                self.last_geom.remove(&key);
            }
        }
    }

    fn queue_hide(&mut self, key: String) {
        self.pending_hide.entry(key).or_insert(HIDE_RETRY_FRAMES);
    }

    fn tick_telemetry(&mut self) {
        if self.last_tick.elapsed().as_millis() < 4 {
            return;
        }
        self.last_tick = Instant::now();
        let mono = self.clock_start.elapsed().as_secs_f64();
        let edit_mode = self.state.read().edit_mode;
        let mut frame = if self.demo_only {
            self.demo.tick_at(mono)
        } else {
            let live = self
                .irsdk
                .as_mut()
                .map(|ir| ir.tick())
                .unwrap_or(TelemetryFrame {
                    connected: false,
                    ..Default::default()
                });
            // Edit layout without iRacing: demo feed so widgets have content.
            // Running + disconnected + not editing stays empty (widgets hidden).
            if !live.connected && edit_mode {
                self.demo.tick_at(mono)
            } else {
                live
            }
        };
        let cfg = Arc::clone(&self.state.read().config);

        let hist_n = cfg
            .f64_key("fuel_calc", "history_laps", 10.0)
            .round()
            .max(1.0) as usize;
        let cap = if frame.fuel_max_l > 0.0 {
            frame.fuel_max_l
        } else if frame.fuel_pct > 0.01 {
            frame.fuel_l / frame.fuel_pct
        } else {
            0.0
        };
        self.fuel_burn.observe(frame.lap, frame.fuel_l, cap, hist_n);
        frame.fuel_use_history = self.fuel_burn.uses.clone();

        self.pit_stops.observe(&frame.cars, frame.session_time);

        // Camera / player focus change: drop sticky relative order so rows don't
        // inherit the previous car's ahead/behind ranking.
        let table_focus = frame
            .camera_car_idx
            .or_else(|| frame.cars.iter().find(|c| c.is_player).map(|c| c.car_idx));
        if table_focus != self.last_table_focus {
            self.rel_order = RelativeOrderHysteresis::default();
            self.last_table_focus = table_focus;
        }

        // Header/footer slots (cpu/mem/gpu) read these fields — sample before finalize.
        self.sysstats.sample_into(&mut frame);

        finalize_frame(&mut frame, cfg.as_ref(), &mut self.rel_order);
        self.pit_stops.apply_frame(&mut frame, cfg.as_ref());

        {
            let n = cfg
                .f64_key("sector_timing", "sectors", 3.0)
                .round()
                .max(1.0) as usize;
            let starts = SectorTimer::equal_starts(n);
            self.sector_timer.set_boundaries(&starts);
        }
        self.sector_timer
            .update(frame.player_lap_dist_pct, frame.cur_lap_s, frame.last_lap_s);
        let show_delta = cfg.bool_key("sector_timing", "show_sector_delta", false);
        frame.sectors_ui = self.sector_timer.snapshot(
            frame.cur_lap_s,
            frame.last_lap_s,
            frame.best_lap_s,
            show_delta,
        );

        let ref_mode = cfg.str_key("lap_compare", "reference_mode", "best");
        self.lap_compare.update(
            frame.player_lap_dist_pct,
            frame.cur_lap_s,
            frame.last_lap_s,
            frame.brake,
            frame.throttle,
            &ref_mode,
        );
        let allow_demo_compare = self.demo_only || edit_mode;
        frame.lap_compare =
            self.lap_compare
                .view(frame.session_time, &ref_mode, allow_demo_compare);

        self.lap_log.observe(
            frame.lap,
            frame.last_lap_s,
            frame.track_temp,
            LapExtras::from_frame(&frame),
        );
        if !self.lap_log.laps.is_empty() {
            frame.lap_log = self.lap_log.build_rows(cfg.as_ref());
        }

        // Auto-switch presets when league/car bindings match.
        {
            let league = frame.league_id;
            let car = frame.car_path.clone().unwrap_or_default();
            let car_key = if car.is_empty() {
                None
            } else {
                Some(car.clone())
            };
            if car_key != self.last_switch_car || league != self.last_switch_league {
                self.last_switch_car = car_key;
                self.last_switch_league = league;
                if let Some(mut st) = self.state.try_write() {
                    if let Some(target) = st.config.preset_for_session(league, &car) {
                        if target != st.config.active_preset {
                            let _ = Arc::make_mut(&mut st.config).set_active_preset(&target);
                            st.apply_effective_context();
                        }
                    }
                }
            }
        }
        {
            let mut st = self.state.write();
            // On track only while seated in-car. Spectating / garage / menus use
            // the In garage profile (and Settings profile combo follows this).
            let next_context = if frame.connected && (frame.in_garage || !frame.in_car) {
                ConfigContext::Garage
            } else {
                ConfigContext::Race
            };
            if st.config_context != next_context {
                st.set_config_context(next_context);
            }
            // Keep HTML-imported authoring TrackID across demo/live ticks.
            if let Some(id) = st.map.cached_track_id {
                frame.track_id = Some(id);
                if !st.map.cached_track_name.is_empty() {
                    frame.track_name = Some(st.map.cached_track_name.clone());
                }
            }
            st.mono_secs = mono;
            st.frame = Arc::new(frame);
        }
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_shell_commands();
        self.tick_telemetry();
        let frame_start = Instant::now();
        // Keep mono clock fresh between telem ticks for map prediction.
        let mono = self.clock_start.elapsed().as_secs_f64();
        self.state.write().mono_secs = mono;

        // Snap panel geometry to content size when panel_style changes (Elegant hugs content).
        {
            let mut st = self.state.write();
            let mut changed = false;
            for key in WIDGET_KEYS {
                let style = match st.config.panel_style(key) {
                    crate::config::PanelStyle::Elegant => "elegant",
                    crate::config::PanelStyle::Data => "data",
                };
                let prev = self.last_panel_style.get(*key).map(String::as_str);
                let style_changed = prev != Some(style);
                if style_changed {
                    // Fit on Elegant first-sight (migrate oversized Data layouts) and on
                    // any later style toggle. Leave untouched Data layouts alone on first see.
                    let should_fit = style == "elegant" || prev.is_some();
                    if should_fit {
                        let (w, h) = crate::config::preferred_panel_size(st.config.as_ref(), key);
                        fit_panel_size(&mut st.layout, key, w, h);
                        changed = true;
                    }
                    self.last_panel_style
                        .insert((*key).to_string(), style.to_string());
                } else if style == "elegant" {
                    // Grow if content preferred size increased (avoids clipped/overlapping text).
                    let (pw, ph) = crate::config::preferred_panel_size(st.config.as_ref(), key);
                    let lay = st.layout.get(*key);
                    let too_small = lay.map(|l| l.h + 8 < ph || l.w + 8 < pw).unwrap_or(true);
                    if too_small {
                        fit_panel_size(&mut st.layout, key, pw, ph);
                        changed = true;
                    }
                }
            }
            if changed {
                st.save_layout_to_preset();
            }
        }

        let (running, edit_mode, click_through, keys_layout) = {
            let st = self.state.read();
            // Python parity: panels only while overlay running AND
            // (demo / live iRacing / edit layout). Settings stays available.
            let live = self.demo_only || st.frame.connected || st.edit_mode;
            let mut items = Vec::new();
            if st.running && live {
                for key in WIDGET_KEYS {
                    if !st.config.widget_shown(key) {
                        continue;
                    }
                    let lay = st
                        .layout
                        .get(*key)
                        .cloned()
                        .unwrap_or(PanelLayout::default());
                    items.push(((*key).to_string(), lay));
                }
            }
            (
                st.running && live,
                st.edit_mode,
                st.click_through && !st.map.interactive,
                items,
            )
        };

        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
            .show(ctx, |_| {});

        self.paint_settings_viewport(ctx);
        if self.state.read().quit_requested {
            self.perf_maybe_emit();
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        if !running {
            let to_hide: Vec<_> = self.open_viewports.drain().collect();
            for key in to_hide {
                self.queue_hide(key);
            }
            *self.dragging.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *self.resizing.lock().unwrap_or_else(|e| e.into_inner()) = None;
            self.run_pending_hides(ctx);
            self.perf_maybe_emit();
            return;
        }

        let panel_count = keys_layout.len() as u32;
        let map_open = keys_layout.iter().any(|(k, _)| k == "map");
        let demo_only = self.demo_only;
        let mut still_open = HashSet::new();
        let passthrough = click_through && !edit_mode;
        let gl = self.gl.clone();
        // Advance map car ease every host tick (independent of GL paint / present).
        // build_car_sprites re-enters with dt≈0 and must NOT pin (see motion rev 14).
        if keys_layout.iter().any(|(k, _)| k == "map") {
            let mut st = self.state.write();
            let mono_secs = st.mono_secs;
            let cfg = Arc::clone(&st.config);
            let frame = Arc::clone(&st.frame);
            let mut panel_animating = false;
            let mut wctx = WidgetCtx {
                cfg: cfg.as_ref(),
                frame: frame.as_ref(),
                edit_mode,
                demo: demo_only,
                map: &mut st.map,
                mono_secs,
                panel_animating: &mut panel_animating,
                map_paint_mode: MapPaintMode::Full,
            };
            widgets::tick_car_motion(&mut wctx);
        }
        // (hwnd, dst_w, dst_h, src_w, src_h, panel_key, hash)
        let mut pending_presents: Vec<(isize, i32, i32, i32, i32, String, u64)> = Vec::new();
        let dragging_key = self
            .dragging
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|d| d.key.clone());
        let resizing_key = self
            .resizing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|r| r.key.clone());
        for (key, lay) in keys_layout {
            still_open.insert(key.clone());
            let vid = ViewportId::from_hash_of(&key);
            let title = win_click::panel_title(&key);
            let is_dragging = dragging_key.as_deref() == Some(key.as_str());
            let is_resizing = resizing_key.as_deref() == Some(key.as_str());
            let geom = (lay.x, lay.y, lay.w, lay.h);
            let geom_changed = self.last_geom.get(&key) != Some(&geom);

            let map_panel = key == "map";
            let was_animating = self.panel_animating.get(&key).copied().unwrap_or(false);

            // CPU-composite map path: cache static track, present cars without GL readback.
            // Invalidate on *client pixel* size (not layout) to avoid DPI thrash.
            let map_client = if map_panel {
                win_click::find_overlay_hwnd(&title).and_then(layered::client_size)
            } else {
                None
            };
            let map_authoring_busy = if map_panel {
                let m = &self.state.read().map;
                m.pit_edit || m.corner_edit || m.sf_edit
            } else {
                false
            };
            // Rev 18: restore hot composite. Rev 17 GL path spent ~200ms/s in
            // map readback and dropped presents to ~25/s (worse stutter).
            let use_map_composite = map_panel && !edit_mode && !map_authoring_busy;
            if map_panel && edit_mode != self.last_edit_mode {
                // Edit uses live GL paint; leaving it must recapture the static bg
                // so a stale/broken infield mesh is not reused on the hot path.
                self.clear_map_bg();
                self.map_bg_fp = 0;
                self.map_capturing_bg = use_map_composite;
                self.last_edit_mode = edit_mode;
            }
            if use_map_composite {
                let (cw, ch) = map_client.unwrap_or((lay.w, lay.h));
                let fp = {
                    let st = self.state.read();
                    widgets::bg_fingerprint(st.config.as_ref(), &st.map, st.frame.as_ref(), cw, ch)
                };
                if fp != self.map_bg_fp {
                    self.clear_map_bg();
                    self.map_bg_fp = fp;
                    self.map_capturing_bg = true;
                }
                if let Some((bw, bh, _)) = &self.map_bg {
                    if *bw != cw || *bh != ch {
                        self.clear_map_bg();
                        self.map_capturing_bg = true;
                    }
                }
                if self.map_bg.is_none() {
                    self.map_capturing_bg = true;
                }
            } else if map_panel {
                self.map_capturing_bg = false;
            }
            let map_hot = use_map_composite && self.map_bg.is_some() && !self.map_capturing_bg;
            let map_paint_mode = if use_map_composite && self.map_capturing_bg {
                MapPaintMode::StaticOnly
            } else {
                MapPaintMode::Full
            };

            let min_ms = if map_panel {
                if map_hot {
                    MAP_HOT_PRESENT_MS
                } else {
                    MAP_READBACK_MS
                }
            } else if was_animating {
                // Tables, radar, delta, dash blink, etc. need ~60 Hz presents.
                PANEL_ANIM_READBACK_MS
            } else if key == "relative" || key == "standings" {
                // Idle tables refresh often enough to catch reorder starts
                // without staircasey first frames.
                33
            } else if map_hot {
                NON_MAP_WHEN_MAP_HOT_MS
            } else {
                NON_MAP_READBACK_MS
            };
            let due = self
                .last_readback
                .get(&key)
                .map(|t| t.elapsed().as_millis() >= min_ms)
                .unwrap_or(true);
            let live_connected = self.state.read().frame.connected;
            let heavy = self.last_frame_ms > HEAVY_FRAME_MS;
            // Yield to map only when map is open in this layout. Absent
            // last_readback["map"] must not starve siblings (black Relative).
            let map_due_now = map_open
                && self
                    .last_readback
                    .get("map")
                    .map(|t| t.elapsed().as_millis() >= MAP_READBACK_MS)
                    .unwrap_or(false);
            let readback_now = if map_panel {
                // Hot composite: ~60 Hz ULW after cheaper labels (rev 15).
                due || is_dragging || is_resizing
            } else if is_dragging || is_resizing || was_animating {
                due || is_dragging || is_resizing
            } else if map_hot && live_connected && !edit_mode {
                // Protect map ULW budget; refresh siblings ~4 Hz.
                due && !heavy
            } else if heavy || (live_connected && !edit_mode && map_due_now) {
                false
            } else {
                due
            };
            // Hot composite: skip GL paint; cold capture / edit / classic still paint.
            let paint_now = if map_hot {
                false
            } else {
                edit_mode || readback_now
            };
            if self.perf {
                if paint_now {
                    self.perf_acc.paint_on += 1;
                } else {
                    self.perf_acc.paint_off += 1;
                }
            }

            let mut builder = ViewportBuilder::default()
                .with_title(title.clone())
                .with_decorations(false)
                .with_transparent(true)
                .with_visible(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_mouse_passthrough(passthrough);
            // Only push geometry when it changed — cuts WGL recreate/switch churn.
            // While manually dragging/resizing, do not fight OS geometry with the builder.
            if !is_dragging && geom_changed {
                builder = builder.with_position(egui::pos2(lay.x as f32, lay.y as f32));
            }
            if !is_resizing && geom_changed {
                builder = builder.with_inner_size([lay.w as f32, lay.h as f32]);
            }
            if geom_changed && !is_dragging && !is_resizing {
                self.last_geom.insert(key.clone(), geom);
            }

            let state = self.state.clone();
            let key_clone = key.clone();
            let dragging = Arc::clone(&self.dragging);
            let resizing = Arc::clone(&self.resizing);
            let panel_anim_flag = Arc::new(AtomicBool::new(false));
            let panel_anim_flag_paint = Arc::clone(&panel_anim_flag);
            let do_paint = paint_now;
            let paint_mode = map_paint_mode;
            let demo = demo_only;

            // Immediate viewports must be shown every tick or eframe closes them.
            // Skip expensive widget paint when present is not due.
            ctx.show_viewport_immediate(vid, builder, move |vp_ctx, _class| {
                // Undo sticky Visible(false) from a prior hide/flicker.
                vp_ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                if !do_paint {
                    return;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
                    .show(vp_ctx, |ui| {
                        let (cfg, frame, mono_secs) = {
                            let st = state.read();
                            (Arc::clone(&st.config), Arc::clone(&st.frame), st.mono_secs)
                        };
                        // Map: move authoring state in/out (no deep clone of path/pits).
                        // Other panels: empty stub — they never touch map geometry.
                        let mut map = if key_clone == "map" {
                            std::mem::take(&mut state.write().map)
                        } else {
                            crate::state::MapAuthoring::default()
                        };
                        {
                            let mut panel_animating = false;
                            let mut wctx = WidgetCtx {
                                cfg: cfg.as_ref(),
                                frame: frame.as_ref(),
                                edit_mode,
                                demo,
                                map: &mut map,
                                mono_secs,
                                panel_animating: &mut panel_animating,
                                map_paint_mode: paint_mode,
                            };
                            widgets::paint(ui, &key_clone, &mut wctx);
                            panel_anim_flag_paint.store(panel_animating, Ordering::Relaxed);
                        }
                        if key_clone == "map" {
                            state.write().map = map;
                        }

                        if edit_mode {
                            let full = ui.max_rect();
                            let grip = Rect::from_min_max(
                                Pos2::new(full.right() - RESIZE_GRIP, full.bottom() - RESIZE_GRIP),
                                full.max,
                            );
                            let accent = cfg.color(&key_clone, "accent", "#70df7a");

                            // SE resize grip
                            ui.painter().rect_filled(
                                grip,
                                CornerRadius::same(4),
                                color_with_alpha(accent, 40),
                            );
                            // Grip lines
                            for i in 0..3 {
                                let t = 0.35 + i as f32 * 0.18;
                                let a =
                                    Pos2::new(grip.left() + grip.width() * t, grip.bottom() - 4.0);
                                let b =
                                    Pos2::new(grip.right() - 4.0, grip.top() + grip.height() * t);
                                ui.painter().line_segment(
                                    [a, b],
                                    Stroke::new(1.5_f32, accent.gamma_multiply(0.85)),
                                );
                            }

                            let grip_resp =
                                ui.interact(grip, ui.id().with("resize"), Sense::drag());
                            if grip_resp.drag_started() {
                                if let Some(pos) = grip_resp.interact_pointer_pos() {
                                    let st = state.read();
                                    let cur = st
                                        .layout
                                        .get(&key_clone)
                                        .cloned()
                                        .unwrap_or(PanelLayout::default());
                                    *resizing.lock().unwrap_or_else(|e| e.into_inner()) =
                                        Some(ResizeDrag {
                                            key: key_clone.clone(),
                                            origin_w: cur.w,
                                            origin_h: cur.h,
                                            start: pos,
                                        });
                                }
                            }
                            if grip_resp.dragged() {
                                if let Some(pos) = grip_resp.interact_pointer_pos() {
                                    let guard = resizing.lock().unwrap_or_else(|e| e.into_inner());
                                    if let Some(rd) = guard.as_ref() {
                                        if rd.key == key_clone {
                                            let dx = pos.x - rd.start.x;
                                            let dy = pos.y - rd.start.y;
                                            let nw = (rd.origin_w as f32 + dx)
                                                .round()
                                                .max(MIN_PANEL_W as f32)
                                                as i32;
                                            let nh = (rd.origin_h as f32 + dy)
                                                .round()
                                                .max(MIN_PANEL_H as f32)
                                                as i32;
                                            drop(guard);
                                            {
                                                let mut st = state.write();
                                                let lay =
                                                    st.layout.entry(key_clone.clone()).or_default();
                                                lay.w = nw;
                                                lay.h = nh;
                                            }
                                            vp_ctx.send_viewport_cmd(ViewportCommand::InnerSize(
                                                egui::vec2(nw as f32, nh as f32),
                                            ));
                                        }
                                    }
                                }
                            }
                            if grip_resp.drag_stopped() {
                                *resizing.lock().unwrap_or_else(|e| e.into_inner()) = None;
                                state.write().save_layout_to_preset();
                            }

                            // Move: full panel except when interacting with grip
                            let ptr_in_grip = ui
                                .input(|i| i.pointer.interact_pos())
                                .map(|p| grip.contains(p))
                                .unwrap_or(false);
                            let resizing_this = resizing
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .as_ref()
                                .map(|r| r.key == key_clone)
                                .unwrap_or(false);
                            if !ptr_in_grip && !resizing_this {
                                let resp = ui.interact(full, ui.id().with("drag"), Sense::drag());
                                if resp.drag_started() {
                                    let outer = vp_ctx.input(|i| i.viewport().outer_rect);
                                    let local = vp_ctx.input(|i| i.pointer.latest_pos());
                                    if let (Some(outer), Some(local)) = (outer, local) {
                                        let screen =
                                            Pos2::new(outer.min.x + local.x, outer.min.y + local.y);
                                        let mut st = state.write();
                                        if let Some(lay) = st.layout.get_mut(&key_clone) {
                                            lay.x = outer.min.x.round() as i32;
                                            lay.y = outer.min.y.round() as i32;
                                            *dragging.lock().unwrap_or_else(|e| e.into_inner()) =
                                                Some(PanelDrag {
                                                    key: key_clone.clone(),
                                                    origin_x: lay.x as f32,
                                                    origin_y: lay.y as f32,
                                                    pointer_origin: screen,
                                                });
                                        }
                                    }
                                }
                                if resp.dragged() {
                                    let drag_snap =
                                        dragging.lock().unwrap_or_else(|e| e.into_inner()).clone();
                                    if let Some(pd) = drag_snap {
                                        if pd.key == key_clone {
                                            let outer = vp_ctx.input(|i| i.viewport().outer_rect);
                                            let local = vp_ctx.input(|i| i.pointer.latest_pos());
                                            if let (Some(outer), Some(local)) = (outer, local) {
                                                let screen = Pos2::new(
                                                    outer.min.x + local.x,
                                                    outer.min.y + local.y,
                                                );
                                                let nx =
                                                    pd.origin_x + (screen.x - pd.pointer_origin.x);
                                                let ny =
                                                    pd.origin_y + (screen.y - pd.pointer_origin.y);
                                                let mut st = state.write();
                                                if let Some(lay) = st.layout.get_mut(&key_clone) {
                                                    lay.x = nx.round() as i32;
                                                    lay.y = ny.round() as i32;
                                                }
                                                drop(st);
                                                vp_ctx.send_viewport_cmd(
                                                    ViewportCommand::OuterPosition(egui::pos2(
                                                        nx, ny,
                                                    )),
                                                );
                                            }
                                        }
                                    }
                                }
                                if resp.drag_stopped() {
                                    if let Some(outer) = vp_ctx.input(|i| i.viewport().outer_rect) {
                                        let mut st = state.write();
                                        if let Some(lay) = st.layout.get_mut(&key_clone) {
                                            lay.x = outer.min.x.round() as i32;
                                            lay.y = outer.min.y.round() as i32;
                                        }
                                        st.save_layout_to_preset();
                                    } else {
                                        state.write().save_layout_to_preset();
                                    }
                                    *dragging.lock().unwrap_or_else(|e| e.into_inner()) = None;
                                }
                            }

                            // Light edit border
                            ui.painter().rect_stroke(
                                full.shrink(0.5),
                                CornerRadius::same(4),
                                Stroke::new(1.0_f32, accent.gamma_multiply(0.55)),
                                egui::StrokeKind::Inside,
                            );
                        }
                    });

                if vp_ctx.input(|i| i.viewport().close_requested()) {
                    // Ignore close — overlay panels aren't user-dismissible.
                }
            });

            if paint_now {
                self.panel_animating
                    .insert(key.clone(), panel_anim_flag.load(Ordering::Relaxed));
            }

            // Read pixels while this viewport's GL context is still current.
            // Defer UpdateLayeredWindow until after all viewports (GDI poisons
            // the next eframe make_current on Windows).
            if let Some(hwnd) = win_click::find_overlay_hwnd(&title) {
                let ppp = ctx.pixels_per_point();
                if map_hot && readback_now {
                    // Hot path: full-res composite + dirty-rect ULW (rev 20).
                    // Half-res scaled ULW still uploaded a full HWND (rev 19 miss).
                    let dims = self.map_bg.as_ref().map(|(bw, bh, _)| (*bw, *bh));
                    if let Some((bw, bh)) = dims {
                        let sprite_ppp = if self.map_bg_ppp > 0.25 {
                            self.map_bg_ppp
                        } else {
                            ppp
                        };
                        let (sprites, map_animating) = {
                            let mut st = self.state.write();
                            let mono_secs = st.mono_secs;
                            let cfg = Arc::clone(&st.config);
                            let frame = Arc::clone(&st.frame);
                            let mut panel_animating = false;
                            let mut wctx = WidgetCtx {
                                cfg: cfg.as_ref(),
                                frame: frame.as_ref(),
                                edit_mode,
                                demo: demo_only,
                                map: &mut st.map,
                                mono_secs,
                                panel_animating: &mut panel_animating,
                                map_paint_mode: MapPaintMode::Full,
                            };
                            let sprites = widgets::build_car_sprites(
                                &mut wctx, bw as f32, bh as f32, sprite_ppp,
                            );
                            (sprites, panel_animating)
                        };
                        self.panel_animating.insert(key.clone(), map_animating);
                        self.map_ulw_dirty = layered::car_sprites_dirty_rects(
                            &self.map_prev_sprites,
                            &sprites,
                            bw,
                            bh,
                        );
                        self.map_prev_sprites = sprites.clone();
                        let scratch = self.readback.entry(key.clone()).or_default();
                        if let Some((_, _, bg)) = self.map_bg.as_ref() {
                            let out = scratch.bgra_mut();
                            layered::composite_map_cars(bg, bw, bh, &sprites, out, None);
                        }
                        let hash = self
                            .last_present_hash
                            .get(&hwnd)
                            .copied()
                            .unwrap_or(0)
                            .wrapping_add(1);
                        pending_presents.push((hwnd, bw, bh, bw, bh, key.clone(), hash));
                        self.last_readback.insert(key.clone(), Instant::now());
                        if self.perf {
                            self.perf_acc.readbacks += 1;
                        }
                    }
                } else if readback_now {

                    if let (Some(gl), Some((w, h))) = (gl.as_ref(), layered::client_size(hwnd)) {
                        let rb_start = Instant::now();
                        if map_panel {
                            let taken_owned = {
                                let scratch = self.readback.entry(key.clone()).or_default();
                                layered::map_take_ready_bgra(gl, &mut self.map_pipe, scratch)
                                    .map(|s| s.to_vec())
                            };
                            if let Some(bgra) = taken_owned {
                                if self.map_capturing_bg && use_map_composite {
                                    // Store static track as bg, then composite cars for present.
                                    self.set_map_bg(w, h, bgra, ppp);
                                    self.map_capturing_bg = false;
                                    let (sw, sh) = self
                                        .map_bg_half
                                        .as_ref()
                                        .map(|(a, b, _)| (*a, *b))
                                        .unwrap_or((w, h));
                                    let scale = if w > 0 { sw as f32 / w as f32 } else { 0.5 };
                                    let sprites = {
                                        let mut st = self.state.write();
                                        let mono_secs = st.mono_secs;
                                        let cfg = Arc::clone(&st.config);
                                        let frame = Arc::clone(&st.frame);
                                        let mut panel_animating = false;
                                        let mut wctx = WidgetCtx {
                                            cfg: cfg.as_ref(),
                                            frame: frame.as_ref(),
                                            edit_mode,
                                            demo: demo_only,
                                            map: &mut st.map,
                                            mono_secs,
                                            panel_animating: &mut panel_animating,
                                            map_paint_mode: MapPaintMode::Full,
                                        };
                                        widgets::build_car_sprites(
                                            &mut wctx,
                                            sw as f32,
                                            sh as f32,
                                            ppp * scale,
                                        )
                                    };
                                    let scratch = self.readback.entry(key.clone()).or_default();
                                    if let Some((bw, bh, bg)) = self.map_bg_half.as_ref() {
                                        let out = scratch.bgra_mut();
                                        layered::composite_map_cars(
                                            bg, *bw, *bh, &sprites, out, None,
                                        );
                                        let hash = layered::hash_bgra(scratch.bgra());
                                        pending_presents
                                            .push((hwnd, w, h, *bw, *bh, key.clone(), hash));
                                    } else if let Some((bw, bh, bg)) = self.map_bg.as_ref() {
                                        let out = scratch.bgra_mut();
                                        layered::composite_map_cars(
                                            bg, *bw, *bh, &sprites, out, None,
                                        );
                                        let hash = layered::hash_bgra(scratch.bgra());
                                        pending_presents
                                            .push((hwnd, w, h, *bw, *bh, key.clone(), hash));
                                    }
                                } else {
                                    let scratch = self.readback.entry(key.clone()).or_default();
                                    *scratch.bgra_mut() = bgra;
                                    let hash = layered::hash_bgra(scratch.bgra());
                                    let (sw, sh) = self.map_pipe.last_taken_dims.unwrap_or((w, h));
                                    pending_presents.push((hwnd, w, h, sw, sh, key.clone(), hash));
                                }
                            }
                            let kicked = layered::map_kick_readback(gl, w, h, &mut self.map_pipe);
                            if kicked {
                                self.last_readback.insert(key.clone(), Instant::now());
                            } else if layered::map_pipe_disabled(&self.map_pipe) {
                                // Driver rejected the PBO path; keep the map visible
                                // on the old sync path rather than dropping presents.
                                let bgra_owned = {
                                    let scratch = self.readback.entry(key.clone()).or_default();
                                    layered::read_gl_to_bgra(gl, w, h, scratch).map(|s| s.to_vec())
                                };
                                if let Some(bgra) = bgra_owned {
                                    if self.map_capturing_bg && use_map_composite {
                                        self.set_map_bg(w, h, bgra, ppp);
                                        self.map_capturing_bg = false;
                                        let (sw, sh) = self
                                            .map_bg_half
                                            .as_ref()
                                            .map(|(a, b, _)| (*a, *b))
                                            .unwrap_or((w, h));
                                        let scale = if w > 0 { sw as f32 / w as f32 } else { 0.5 };
                                        let sprites = {
                                            let mut st = self.state.write();
                                            let mono_secs = st.mono_secs;
                                            let cfg = Arc::clone(&st.config);
                                            let frame = Arc::clone(&st.frame);
                                            let mut panel_animating = false;
                                            let mut wctx = WidgetCtx {
                                                cfg: cfg.as_ref(),
                                                frame: frame.as_ref(),
                                                edit_mode,
                                                demo: demo_only,
                                                map: &mut st.map,
                                                mono_secs,
                                                panel_animating: &mut panel_animating,
                                                map_paint_mode: MapPaintMode::Full,
                                            };
                                            widgets::build_car_sprites(
                                                &mut wctx,
                                                sw as f32,
                                                sh as f32,
                                                ppp * scale,
                                            )
                                        };
                                        let scratch = self.readback.entry(key.clone()).or_default();
                                        if let Some((bw, bh, bg)) = self.map_bg_half.as_ref() {
                                            let out = scratch.bgra_mut();
                                            layered::composite_map_cars(
                                                bg, *bw, *bh, &sprites, out, None,
                                            );
                                            let hash = layered::hash_bgra(scratch.bgra());
                                            pending_presents
                                                .push((hwnd, w, h, *bw, *bh, key.clone(), hash));
                                        } else if let Some((bw, bh, bg)) = self.map_bg.as_ref() {
                                            let out = scratch.bgra_mut();
                                            layered::composite_map_cars(
                                                bg, *bw, *bh, &sprites, out, None,
                                            );
                                            let hash = layered::hash_bgra(scratch.bgra());
                                            pending_presents
                                                .push((hwnd, w, h, *bw, *bh, key.clone(), hash));
                                        }
                                    }
                                    self.last_readback.insert(key.clone(), Instant::now());
                                }
                            }
                            if self.perf {
                                self.perf_acc.readbacks += 1;
                                self.perf_acc.rb_map_ms +=
                                    rb_start.elapsed().as_secs_f64() * 1000.0;
                            }
                        } else {
                            let scratch = self.readback.entry(key.clone()).or_default();
                            if let Some(bgra) = layered::read_gl_to_bgra(gl, w, h, scratch) {
                                let rb_ms = rb_start.elapsed().as_secs_f64() * 1000.0;
                                if self.perf {
                                    self.perf_acc.readbacks += 1;
                                    self.perf_acc.rb_other_ms += rb_ms;
                                }
                                self.last_readback.insert(key.clone(), Instant::now());
                                let hash = layered::hash_bgra(bgra);
                                if self.last_present_hash.get(&hwnd) != Some(&hash) {
                                    pending_presents.push((hwnd, w, h, w, h, key.clone(), hash));
                                }
                            }
                        }
                    }
                }
                if self.last_click_through.get(&key) != Some(&passthrough) {
                    win_click::set_click_through(hwnd, passthrough);
                    self.last_click_through.insert(key.clone(), passthrough);
                }
            }

            if key == "map" && readback_now {
                self.perf_record_map_dt();
            }

            self.open_viewports.insert(key);
        }

        for (hwnd, dst_w, dst_h, src_w, src_h, key, hash) in pending_presents {
            if let Some(scratch) = self.readback.get(&key) {
                let ulw_start = Instant::now();
                let map_dirty = if key == "map" {
                    std::mem::take(&mut self.map_ulw_dirty)
                } else {
                    Vec::new()
                };
                if src_w == dst_w && src_h == dst_h {
                    if key == "map" {
                        layered::present_bgra_dirty(
                            &mut self.present_cache,
                            hwnd,
                            dst_w,
                            dst_h,
                            scratch.bgra(),
                            &map_dirty,
                        );
                    } else {
                        layered::present_bgra(
                            &mut self.present_cache,
                            hwnd,
                            dst_w,
                            dst_h,
                            scratch.bgra(),
                        );
                    }
                } else {
                    layered::present_bgra_scaled(
                        &mut self.present_cache,
                        hwnd,
                        dst_w,
                        dst_h,
                        src_w,
                        src_h,
                        scratch.bgra(),
                    );
                }
                if self.perf {
                    self.perf_acc.presents += 1;
                    self.perf_acc.ulw_ms += ulw_start.elapsed().as_secs_f64() * 1000.0;
                }
                self.last_present_hash.insert(hwnd, hash);
            }
        }

        // Drop scratch/hashes for panels that closed.
        self.readback.retain(|k, _| still_open.contains(k));
        self.last_readback.retain(|k, _| still_open.contains(k));
        self.panel_animating.retain(|k, _| still_open.contains(k));
        let mut open_hwnds = HashSet::new();
        for key in &still_open {
            let title = win_click::panel_title(key);
            if let Some(hwnd) = win_click::find_overlay_hwnd(&title) {
                open_hwnds.insert(hwnd);
            }
        }
        self.last_present_hash
            .retain(|hwnd, _| open_hwnds.contains(hwnd));
        self.present_cache.retain_hwnds(&open_hwnds);

        // Re-enabled widgets leave the hide queue; disabled ones get farewell frames.
        for key in &still_open {
            self.pending_hide.remove(key);
            self.missing_frames.remove(key);
        }
        let candidates: Vec<_> = self
            .open_viewports
            .difference(&still_open)
            .cloned()
            .collect();
        // Drop missing counters for keys no longer tracked as open.
        self.missing_frames
            .retain(|k, _| self.open_viewports.contains(k) || candidates.contains(k));
        for key in candidates {
            let n = self.missing_frames.entry(key.clone()).or_insert(0);
            *n = n.saturating_add(1);
            if *n >= HIDE_DEBOUNCE_FRAMES {
                self.queue_hide(key);
            }
        }
        self.open_viewports = still_open;
        self.run_pending_hides(ctx);

        let ms = frame_start.elapsed().as_secs_f64() * 1000.0;
        self.last_frame_ms = ms;
        if self.perf {
            self.perf_acc.frames += 1;
            self.perf_acc.frame_ms_sum += ms;
            self.perf_acc.panel_slots += panel_count;
            self.perf_maybe_emit();
        }
    }
}
