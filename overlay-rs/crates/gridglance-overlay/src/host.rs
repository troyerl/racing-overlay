//! Multi-viewport egui host for overlay panels.

use crate::chrome::color_with_alpha;
use crate::config::{ConfigContext, WIDGET_KEYS};
use crate::layered;
use crate::settings;
use crate::state::{PanelLayout, StateHandle};
use crate::sysstats::SysStats;
use crate::telemetry::{
    demo::DemoFeed, finalize_frame, FuelBurnTracker, IrsdkReader, LapCompareState, LapExtras,
    LapLogAccum, PitStopTracker, SectorTimer, TelemetryFrame,
};
use crate::widgets::{self, WidgetCtx};
use crate::win_click;
use eframe::egui::{
    self, CornerRadius, Pos2, Rect, Sense, Stroke, ViewportBuilder, ViewportCommand, ViewportId,
};
use eframe::glow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const MIN_PANEL_W: i32 = 90;
const MIN_PANEL_H: i32 = 44;
const RESIZE_GRIP: f32 = 28.0;
/// Frames to keep applying Visible(false) so macOS/eframe actually drops the window.
const HIDE_RETRY_FRAMES: u8 = 8;
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
    tray_rx: Option<std::sync::mpsc::Receiver<crate::shell::TrayCommand>>,
    activate_peer: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Last car/league seen for preset auto-switch (avoid thrashing).
    last_switch_car: Option<String>,
    last_switch_league: Option<i32>,
}

impl OverlayApp {
    pub fn new(
        state: StateHandle,
        demo: bool,
        gl: Option<Arc<glow::Context>>,
        tray_rx: Option<std::sync::mpsc::Receiver<crate::shell::TrayCommand>>,
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
            tray_rx,
            activate_peer,
            last_switch_car: None,
            last_switch_league: None,
        }
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
        if let Some(rx) = &self.tray_rx {
            for cmd in crate::shell::poll_events(rx) {
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
        if self.last_tick.elapsed().as_millis() < 8 {
            return;
        }
        self.last_tick = Instant::now();
        let edit_mode = self.state.read().edit_mode;
        let mut frame = if self.demo_only {
            self.demo.tick()
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
                self.demo.tick()
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

        finalize_frame(&mut frame, cfg.as_ref());
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
        frame.lap_compare = self.lap_compare.view(frame.session_time, &ref_mode);

        self.lap_log.observe(
            frame.lap,
            frame.last_lap_s,
            frame.track_temp,
            LapExtras {
                fuel_l: Some(frame.fuel_l),
                incidents: Some(frame.incidents),
                personal_best: frame.best_lap_s,
                ..Default::default()
            },
        );
        if !self.lap_log.laps.is_empty() {
            frame.lap_log = self.lap_log.build_rows(cfg.as_ref());
        }

        self.sysstats.sample_into(&mut frame);
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
                        }
                    }
                }
            }
        }
        {
            let mut st = self.state.write();
            let next_context = if frame.in_garage {
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
        ctx.request_repaint_after(std::time::Duration::from_millis(8));

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

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
            .show(ctx, |_| {});

        self.paint_settings_viewport(ctx);
        if self.state.read().quit_requested {
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
            return;
        }

        let mut still_open = HashSet::new();
        let passthrough = click_through && !edit_mode;
        let gl = self.gl.clone();
        let mut pending_presents: Vec<(isize, i32, i32, Vec<u8>)> = Vec::new();
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

            ctx.show_viewport_immediate(vid, builder, move |vp_ctx, _class| {
                // Undo sticky Visible(false) from a prior hide/flicker.
                vp_ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
                    .show(vp_ctx, |ui| {
                        let (cfg, frame, mut map) = {
                            let st = state.read();
                            (
                                Arc::clone(&st.config),
                                Arc::clone(&st.frame),
                                st.map.clone(),
                            )
                        };
                        {
                            let mut wctx = WidgetCtx {
                                cfg: cfg.as_ref(),
                                frame: frame.as_ref(),
                                edit_mode,
                                map: &mut map,
                            };
                            widgets::paint(ui, &key_clone, &mut wctx);
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
                                                let lay = st
                                                    .layout
                                                    .entry(key_clone.clone())
                                                    .or_insert_with(PanelLayout::default);
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

            // Read pixels while this viewport's GL context is still current.
            // Defer UpdateLayeredWindow until after all viewports (GDI poisons
            // the next eframe make_current on Windows).
            if let Some(hwnd) = win_click::find_overlay_hwnd(&title) {
                if let (Some(gl), Some((w, h))) = (gl.as_ref(), layered::client_size(hwnd)) {
                    if let Some(bgra) = layered::read_gl_to_bgra(gl, w, h) {
                        pending_presents.push((hwnd, w, h, bgra));
                    }
                }
                if self.last_click_through.get(&key) != Some(&passthrough) {
                    win_click::set_click_through(hwnd, passthrough);
                    self.last_click_through.insert(key.clone(), passthrough);
                }
            }

            self.open_viewports.insert(key);
        }

        for (hwnd, w, h, bgra) in pending_presents {
            layered::present_bgra(hwnd, w, h, &bgra);
        }

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
    }
}
