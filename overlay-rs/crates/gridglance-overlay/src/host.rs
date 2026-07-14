//! Multi-viewport egui host for overlay panels.

use crate::config::WIDGET_KEYS;
use crate::chrome::color_with_alpha;
use crate::layered;
use crate::state::{PanelLayout, StateHandle};
use crate::sysstats::SysStats;
use crate::telemetry::{
    demo::DemoFeed, finalize_frame, FuelBurnTracker, IrsdkReader, LapCompareState, LapExtras,
    LapLogAccum, SectorTimer,
};
use crate::widgets::{self, WidgetCtx};
use crate::win_click;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, ViewportBuilder, ViewportCommand, ViewportId};
use eframe::glow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const MIN_PANEL_W: i32 = 90;
const MIN_PANEL_H: i32 = 44;
const RESIZE_GRIP: f32 = 28.0;

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
    /// Active panel move in edit mode.
    dragging: Arc<Mutex<Option<PanelDrag>>>,
    /// Active SE-corner resize in edit mode.
    resizing: Arc<Mutex<Option<ResizeDrag>>>,
    last_click_through: HashMap<String, bool>,
    gl: Option<Arc<glow::Context>>,
    sector_timer: SectorTimer,
    lap_compare: LapCompareState,
    lap_log: LapLogAccum,
    fuel_burn: FuelBurnTracker,
}

impl OverlayApp {
    pub fn new(state: StateHandle, demo: bool, gl: Option<Arc<glow::Context>>) -> Self {
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
            last_click_through: HashMap::new(),
            dragging: Arc::new(Mutex::new(None)),
            resizing: Arc::new(Mutex::new(None)),
            gl,
            sector_timer: SectorTimer::new(),
            lap_compare: LapCompareState::new(),
            lap_log,
            fuel_burn: FuelBurnTracker::default(),
        }
    }

    fn tick_telemetry(&mut self) {
        if self.last_tick.elapsed().as_millis() < 8 {
            return;
        }
        self.last_tick = Instant::now();
        let mut frame = if self.demo_only {
            self.demo.tick()
        } else if let Some(ir) = &mut self.irsdk {
            let live = ir.tick();
            if live.connected {
                live
            } else {
                self.demo.tick()
            }
        } else {
            self.demo.tick()
        };
        let cfg = Arc::clone(&self.state.read().config);

        let hist_n = cfg.f64_key("fuel_calc", "history_laps", 10.0).round().max(1.0) as usize;
        let cap = if frame.fuel_max_l > 0.0 {
            frame.fuel_max_l
        } else if frame.fuel_pct > 0.01 {
            frame.fuel_l / frame.fuel_pct
        } else {
            0.0
        };
        self.fuel_burn
            .observe(frame.lap, frame.fuel_l, cap, hist_n);
        frame.fuel_use_history = self.fuel_burn.uses.clone();

        finalize_frame(&mut frame, cfg.as_ref());

        {
            let n = cfg.f64_key("sector_timing", "sectors", 3.0).round().max(1.0) as usize;
            let starts = SectorTimer::equal_starts(n);
            self.sector_timer.set_boundaries(&starts);
        }
        self.sector_timer.update(
            frame.player_lap_dist_pct,
            frame.cur_lap_s,
            frame.last_lap_s,
        );
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
        self.state.write().frame = Arc::new(frame);
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_telemetry();
        ctx.request_repaint_after(std::time::Duration::from_millis(8));

        let (running, edit_mode, click_through, keys_layout) = {
            let st = self.state.read();
            let mut items = Vec::new();
            if st.running {
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
                st.running,
                st.edit_mode,
                st.click_through && !st.map.interactive,
                items,
            )
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
            .show(ctx, |_| {});

        if !running {
            let to_close: Vec<_> = self.open_viewports.iter().cloned().collect();
            for key in to_close {
                ctx.send_viewport_cmd_to(
                    ViewportId::from_hash_of(&key),
                    ViewportCommand::Close,
                );
                self.open_viewports.remove(&key);
                self.last_click_through.remove(&key);
            }
            *self.dragging.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *self.resizing.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return;
        }

        let mut still_open = HashSet::new();
        let passthrough = click_through && !edit_mode;
        let gl = self.gl.clone();
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

            let mut builder = ViewportBuilder::default()
                .with_title(title.clone())
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_mouse_passthrough(passthrough);
            // While manually dragging/resizing, do not fight OS geometry with the builder.
            if !is_dragging {
                builder = builder.with_position(egui::pos2(lay.x as f32, lay.y as f32));
            }
            if !is_resizing {
                builder = builder.with_inner_size([lay.w as f32, lay.h as f32]);
            }

            let state = self.state.clone();
            let key_clone = key.clone();
            let dragging = Arc::clone(&self.dragging);
            let resizing = Arc::clone(&self.resizing);

            ctx.show_viewport_immediate(vid, builder, move |vp_ctx, _class| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
                    .show(vp_ctx, |ui| {
                        let (cfg, frame, mut map) = {
                            let st = state.read();
                            (Arc::clone(&st.config), Arc::clone(&st.frame), st.map.clone())
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
                                let a = Pos2::new(
                                    grip.left() + grip.width() * t,
                                    grip.bottom() - 4.0,
                                );
                                let b = Pos2::new(
                                    grip.right() - 4.0,
                                    grip.top() + grip.height() * t,
                                );
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
                                    let guard =
                                        resizing.lock().unwrap_or_else(|e| e.into_inner());
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
                                state.write().save_layout();
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
                                let resp = ui.interact(
                                    full,
                                    ui.id().with("drag"),
                                    Sense::drag(),
                                );
                                if resp.drag_started() {
                                    let outer = vp_ctx.input(|i| i.viewport().outer_rect);
                                    let local = vp_ctx.input(|i| i.pointer.latest_pos());
                                    if let (Some(outer), Some(local)) = (outer, local) {
                                        let screen = Pos2::new(
                                            outer.min.x + local.x,
                                            outer.min.y + local.y,
                                        );
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
                                    let drag_snap = dragging
                                        .lock()
                                        .unwrap_or_else(|e| e.into_inner())
                                        .clone();
                                    if let Some(pd) = drag_snap {
                                        if pd.key == key_clone {
                                            let outer =
                                                vp_ctx.input(|i| i.viewport().outer_rect);
                                            let local =
                                                vp_ctx.input(|i| i.pointer.latest_pos());
                                            if let (Some(outer), Some(local)) = (outer, local)
                                            {
                                                let screen = Pos2::new(
                                                    outer.min.x + local.x,
                                                    outer.min.y + local.y,
                                                );
                                                let nx = pd.origin_x
                                                    + (screen.x - pd.pointer_origin.x);
                                                let ny = pd.origin_y
                                                    + (screen.y - pd.pointer_origin.y);
                                                let mut st = state.write();
                                                if let Some(lay) =
                                                    st.layout.get_mut(&key_clone)
                                                {
                                                    lay.x = nx.round() as i32;
                                                    lay.y = ny.round() as i32;
                                                }
                                                drop(st);
                                                vp_ctx.send_viewport_cmd(
                                                    ViewportCommand::OuterPosition(
                                                        egui::pos2(nx, ny),
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                                if resp.drag_stopped() {
                                    if let Some(outer) =
                                        vp_ctx.input(|i| i.viewport().outer_rect)
                                    {
                                        let mut st = state.write();
                                        if let Some(lay) = st.layout.get_mut(&key_clone) {
                                            lay.x = outer.min.x.round() as i32;
                                            lay.y = outer.min.y.round() as i32;
                                        }
                                        st.save_layout();
                                    } else {
                                        state.write().save_layout();
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

            // Per-pixel alpha composite (Windows): read GL backbuffer → UpdateLayeredWindow.
            if let Some(hwnd) = win_click::find_overlay_hwnd(&title) {
                if let (Some(gl), Some((w, h))) = (gl.as_ref(), layered::client_size(hwnd)) {
                    layered::present_gl_framebuffer(gl, hwnd, w, h);
                }
                if self.last_click_through.get(&key) != Some(&passthrough) {
                    win_click::set_click_through(hwnd, passthrough);
                    self.last_click_through.insert(key.clone(), passthrough);
                }
            }

            self.open_viewports.insert(key);
        }

        let to_close: Vec<_> = self
            .open_viewports
            .difference(&still_open)
            .cloned()
            .collect();
        for key in to_close {
            ctx.send_viewport_cmd_to(
                ViewportId::from_hash_of(&key),
                ViewportCommand::Close,
            );
            self.open_viewports.remove(&key);
            self.last_click_through.remove(&key);
        }
        self.open_viewports = still_open;
    }
}
