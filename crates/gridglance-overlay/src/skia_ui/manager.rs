//! Owns Skia panel HWNDs, paint cadence, and ULW present.

use super::anim::AnimStore;
use super::canvas::Canvas;
use super::dash;
use super::hwnd::{self, PanelHwnd};
use super::map as map_panel;
use super::panels::{self, PanelPaintCtx};
use super::table;
use crate::layered::{self, PresentCache};
use crate::state::{MapAuthoring, PanelLayout, StateHandle, TrackPathStatus};
use crate::widgets;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

const TABLE_MS: u128 = 33;
const PEDAL_MS: u128 = 16;
const RADIO_MS: u128 = 50;
const DEFAULT_MS: u128 = 50;
/// Map hot path paints every host sync (0 = no cadence gate).
const MAP_MS: u128 = 0;
const MAX_PRESENT_PER_FRAME: u32 = 4;

/// All overlay widget keys are Skia-backed (egui only keeps Settings).
pub fn is_skia_panel(key: &str) -> bool {
    crate::config::WIDGET_KEYS.contains(&key)
}

pub fn skia_overlay_enabled() -> bool {
    // Default on. Set GRIDGLANCE_EGUI_PANELS=1 to force legacy egui overlay panels.
    std::env::var("GRIDGLANCE_EGUI_PANELS")
        .map(|v| v != "1" && !v.eq_ignore_ascii_case("true"))
        .unwrap_or(true)
}

struct PanelState {
    hwnd: PanelHwnd,
    last_geom: (i32, i32, i32, i32),
    last_present: Option<Instant>,
    animating: bool,
}

pub struct SkiaPanelHost {
    panels: HashMap<String, PanelState>,
    anim: AnimStore,
    present_cache: PresentCache,
    /// Cached static track BGRA `(w, h, pixels)` for hot car composite.
    map_bg: Option<(i32, i32, Vec<u8>)>,
    map_bg_fp: u64,
    /// Reused hot-path surface — `Canvas::new` reloads three typefaces.
    map_canvas: Option<Canvas>,
    map_scratch: Vec<u8>,
    /// Edit-mode drag
    drag: Option<DragState>,
    /// Pit-edit pointer edges (Skia HWND has no egui input).
    pit_prev_left: bool,
    pit_prev_right: bool,
    pit_last_cursor: Option<(i32, i32)>,
    /// Primary press started on a handle (suppress click-append).
    pit_press_on_handle: bool,
    /// Cursor moved past drag threshold during this primary press.
    pit_press_moved: bool,
}

struct DragState {
    key: String,
    kind: DragKind,
    origin_layout: PanelLayout,
    origin_cursor: (i32, i32),
}

enum DragKind {
    Move,
    Resize,
}

impl Default for SkiaPanelHost {
    fn default() -> Self {
        Self::new()
    }
}

impl SkiaPanelHost {
    pub fn new() -> Self {
        Self {
            panels: HashMap::new(),
            anim: AnimStore::default(),
            present_cache: PresentCache::default(),
            map_bg: None,
            map_bg_fp: 0,
            map_canvas: None,
            map_scratch: Vec::new(),
            drag: None,
            pit_prev_left: false,
            pit_prev_right: false,
            pit_last_cursor: None,
            pit_press_on_handle: false,
            pit_press_moved: false,
        }
    }

    pub fn shutdown(&mut self) {
        for (_, p) in self.panels.drain() {
            hwnd::destroy(p.hwnd.hwnd);
        }
        self.drag = None;
        self.pit_prev_left = false;
        self.pit_prev_right = false;
        self.pit_last_cursor = None;
        self.pit_press_on_handle = false;
        self.pit_press_moved = false;
        self.map_bg = None;
        self.map_bg_fp = 0;
        self.map_canvas = None;
        self.map_scratch.clear();
    }

    /// True while the user is moving/resizing this panel in edit mode.
    pub fn is_interacting(&self, key: &str) -> bool {
        self.drag.as_ref().is_some_and(|d| d.key == key)
    }

    /// Paint due Skia panels. Returns `(presents, ulw_ms, map_painted)`.
    pub fn sync_and_paint(
        &mut self,
        state: &StateHandle,
        keys_layout: &[(String, PanelLayout)],
        edit_mode: bool,
        click_through: bool,
        demo: bool,
        mono_secs: f64,
    ) -> (u32, f64, bool) {
        let want: HashSet<String> = keys_layout.iter().map(|(k, _)| k.clone()).collect();
        let stale: Vec<String> = self
            .panels
            .keys()
            .filter(|k| !want.contains(*k))
            .cloned()
            .collect();
        for k in stale {
            if let Some(p) = self.panels.remove(&k) {
                hwnd::destroy(p.hwnd.hwnd);
            }
            if k == "map" {
                self.map_bg = None;
                self.map_bg_fp = 0;
                self.map_canvas = None;
            }
        }

        self.handle_edit_input(state, keys_layout, edit_mode);

        let passthrough = click_through && !edit_mode;
        let mut budget = MAX_PRESENT_PER_FRAME;
        let mut presents = 0u32;
        let mut ulw_ms = 0.0_f64;
        let mut map_painted = false;

        // Map first every sync, outside the shared present budget (motion-critical).
        if let Some((_, map_lay)) = keys_layout.iter().find(|(k, _)| k == "map") {
            if let Some((ms, _)) =
                self.paint_map(state, map_lay, edit_mode, passthrough, demo, mono_secs)
            {
                presents += 1;
                ulw_ms += ms;
                map_painted = true;
            }
        }

        // Overdue-first among remaining due panels (budgeted).
        let mut due: Vec<(String, PanelLayout, u128)> = Vec::new();
        for (key, lay) in keys_layout {
            if !is_skia_panel(key) || key == "map" {
                continue;
            }
            let elapsed = self
                .panels
                .get(key)
                .and_then(|p| p.last_present.map(|t| t.elapsed().as_millis()))
                .unwrap_or(u128::MAX);
            let min_ms = cadence(
                key,
                self.panels.get(key).map(|p| p.animating).unwrap_or(false),
            );
            if elapsed >= min_ms || !self.panels.contains_key(key) {
                due.push((key.clone(), lay.clone(), elapsed));
            } else if let Some(p) = self.panels.get(key) {
                hwnd::set_click_through(p.hwnd.hwnd, passthrough);
            }
        }
        due.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));

        for (key, lay, _) in due {
            if budget == 0 {
                break;
            }
            if let Some((ms, is_map)) =
                self.paint_one(state, &key, &lay, edit_mode, passthrough, demo, mono_secs)
            {
                presents += 1;
                ulw_ms += ms;
                map_painted |= is_map;
                budget -= 1;
            }
        }
        (presents, ulw_ms, map_painted)
    }

    /// Map: cached static BG + CPU car composite + dirty ULW (rev 29).
    /// Falls back to full Skia paint while editing / path loading / authoring.
    fn paint_map(
        &mut self,
        state: &StateHandle,
        lay: &PanelLayout,
        edit_mode: bool,
        passthrough: bool,
        demo: bool,
        mono_secs: f64,
    ) -> Option<(f64, bool)> {
        let geom = (lay.x, lay.y, lay.w.max(1), lay.h.max(1));
        if !self.panels.contains_key("map") {
            let hwnd = hwnd::create_panel("map", geom.0, geom.1, geom.2, geom.3)?;
            self.panels.insert(
                "map".to_string(),
                PanelState {
                    hwnd,
                    last_geom: geom,
                    last_present: None,
                    animating: false,
                },
            );
        }
        let panel = self.panels.get_mut("map").unwrap();
        if panel.last_geom != geom {
            hwnd::set_bounds(panel.hwnd.hwnd, geom.0, geom.1, geom.2, geom.3);
            panel.last_geom = geom;
            self.map_bg = None;
        }
        hwnd::set_click_through(panel.hwnd.hwnd, passthrough);

        let w = geom.2;
        let h = geom.3;

        // Pit-edit input (Skia HWND — egui never sees map pointer events).
        self.handle_pit_edit_input(state, geom, w, h);

        let (force_full, path_ready, fp) = {
            let mut st = state.write();
            let cfg = st.config.clone();
            let frame = st.frame.clone();
            let authoring = st.map.pit_edit || st.map.corner_edit || st.map.sf_edit;
            let force_full = edit_mode || authoring;
            let mut flag = false;
            let mut wctx = widgets::WidgetCtx {
                cfg: cfg.as_ref(),
                frame: frame.as_ref(),
                edit_mode,
                demo,
                map: &mut st.map,
                mono_secs,
                panel_animating: &mut flag,
                map_paint_mode: widgets::MapPaintMode::Full,
            };
            widgets::ensure_path_cached(&mut wctx);
            let path_ready =
                wctx.map.path_status == TrackPathStatus::Ready && wctx.map.cached_path.len() >= 3;
            let fp = widgets::bg_fingerprint(cfg.as_ref(), wctx.map, frame.as_ref(), w, h);
            (force_full, path_ready, fp)
        };

        if force_full || !path_ready {
            self.map_bg = None;
            return self.paint_map_full_skia(state, w, h, edit_mode, demo, mono_secs);
        }

        let need_recapture = self
            .map_bg
            .as_ref()
            .map(|(bw, bh, _)| *bw != w || *bh != h)
            .unwrap_or(true)
            || self.map_bg_fp != fp;

        if need_recapture {
            let mut canvas = Canvas::new(w, h)?;
            {
                let mut st = state.write();
                let cfg = st.config.clone();
                let frame = st.frame.clone();
                let _ = map_panel::paint_static(
                    &mut canvas,
                    cfg.as_ref(),
                    frame.as_ref(),
                    &mut st.map,
                    mono_secs,
                    demo,
                );
            }
            let bgra = canvas.to_bgra();
            self.map_bg = Some((w, h, bgra));
            self.map_bg_fp = fp;
        }

        match self.paint_map_hot(state, w, h, demo, mono_secs) {
            Some(res) => Some(res),
            None => self.paint_map_full_skia(state, w, h, edit_mode, demo, mono_secs),
        }
    }

    /// Seed a reused surface with the cached static track, then run the same car
    /// renderer as the full paint. Keeps number labels, the player ring,
    /// leader/ahead/behind marker rings and status badges on the hot path.
    /// `None` means the caller should fall back to a full repaint.
    fn paint_map_hot(
        &mut self,
        state: &StateHandle,
        w: i32,
        h: i32,
        demo: bool,
        mono_secs: f64,
    ) -> Option<(f64, bool)> {
        let stale = self
            .map_canvas
            .as_ref()
            .map(|c| c.width() != w || c.height() != h)
            .unwrap_or(true);
        if stale {
            self.map_canvas = Canvas::new(w, h);
        }
        let canvas = self.map_canvas.as_mut()?;
        let (_, _, bg) = self.map_bg.as_ref()?;
        if !canvas.write_bgra(bg) {
            return None;
        }

        let animating = {
            let mut st = state.write();
            let cfg = st.config.clone();
            let frame = st.frame.clone();
            map_panel::paint_dynamic(
                canvas,
                cfg.as_ref(),
                frame.as_ref(),
                &mut st.map,
                mono_secs,
                demo,
            )
        };
        let mut scratch = std::mem::take(&mut self.map_scratch);
        canvas.read_bgra_into(&mut scratch);

        let hwnd = self.panels.get("map").map(|p| p.hwnd.hwnd).unwrap_or(0);
        let ulw_start = Instant::now();
        layered::present_bgra(&mut self.present_cache, hwnd, w, h, &scratch);
        let ulw_ms = ulw_start.elapsed().as_secs_f64() * 1000.0;
        self.map_scratch = scratch;
        if let Some(p) = self.panels.get_mut("map") {
            p.last_present = Some(Instant::now());
            p.animating = animating;
        }
        Some((ulw_ms, true))
    }

    fn paint_map_full_skia(
        &mut self,
        state: &StateHandle,
        w: i32,
        h: i32,
        edit_mode: bool,
        demo: bool,
        mono_secs: f64,
    ) -> Option<(f64, bool)> {
        let mut canvas = Canvas::new(w, h)?;
        let animating = {
            let mut st = state.write();
            let cfg = st.config.clone();
            let frame = st.frame.clone();
            map_panel::paint(
                &mut canvas,
                cfg.as_ref(),
                frame.as_ref(),
                &mut st.map,
                mono_secs,
                demo,
                edit_mode,
            )
        };

        if edit_mode {
            let grip = 18.0_f32;
            canvas.fill_rect(
                super::types::Rect::from_xywh((w as f32) - grip, (h as f32) - grip, grip, grip),
                super::types::Rgba::rgb(112, 223, 122).with_alpha(90),
                4.0,
            );
        }

        let bgra = canvas.to_bgra();
        let hwnd = self.panels.get("map").map(|p| p.hwnd.hwnd).unwrap_or(0);
        let ulw_start = Instant::now();
        layered::present_bgra(&mut self.present_cache, hwnd, w, h, &bgra);
        let ulw_ms = ulw_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(p) = self.panels.get_mut("map") {
            p.last_present = Some(Instant::now());
            p.animating = animating;
        }
        Some((ulw_ms, true))
    }

    /// Paint one non-map panel. Returns `Some((ulw_ms, is_map))` on success.
    fn paint_one(
        &mut self,
        state: &StateHandle,
        key: &str,
        lay: &PanelLayout,
        edit_mode: bool,
        passthrough: bool,
        demo: bool,
        mono_secs: f64,
    ) -> Option<(f64, bool)> {
        if key == "map" {
            return self.paint_map(state, lay, edit_mode, passthrough, demo, mono_secs);
        }

        let geom = (lay.x, lay.y, lay.w.max(1), lay.h.max(1));
        if !self.panels.contains_key(key) {
            let hwnd = hwnd::create_panel(key, geom.0, geom.1, geom.2, geom.3)?;
            self.panels.insert(
                key.to_string(),
                PanelState {
                    hwnd,
                    last_geom: geom,
                    last_present: None,
                    animating: false,
                },
            );
        }
        let panel = self.panels.get_mut(key).unwrap();
        if panel.last_geom != geom {
            hwnd::set_bounds(panel.hwnd.hwnd, geom.0, geom.1, geom.2, geom.3);
            panel.last_geom = geom;
        }
        hwnd::set_click_through(panel.hwnd.hwnd, passthrough);

        let w = geom.2;
        let h = geom.3;
        let mut canvas = Canvas::new(w, h)?;

        let animating = {
            let mut st = state.write();
            let cfg = st.config.clone();
            let frame = st.frame.clone();
            if key == "relative" || key == "standings" {
                let anim = self.anim.table_mut(key);
                let (rows, slots) = if key == "relative" {
                    (frame.relative_cars.as_slice(), &frame.relative_slots)
                } else {
                    (frame.standings_cars.as_slice(), &frame.standings_slots)
                };
                table::paint_table(
                    &mut canvas,
                    cfg.as_ref(),
                    key,
                    rows,
                    slots,
                    anim,
                    mono_secs,
                    frame.as_ref(),
                )
            } else if key == "dash" {
                let anim = self.anim.dash_mut();
                dash::paint(&mut canvas, cfg.as_ref(), frame.as_ref(), anim, mono_secs)
            } else {
                let cfg_p = st.config.clone();
                let frame_p = st.frame.clone();
                let mut pctx = PanelPaintCtx {
                    cfg: cfg_p.as_ref(),
                    frame: frame_p.as_ref(),
                    edit_mode,
                    demo,
                    map: &mut st.map,
                    mono_secs,
                    anim: &mut self.anim,
                };
                panels::paint_panel(&mut canvas, key, &mut pctx)
            }
        };

        if edit_mode {
            let grip = 18.0_f32;
            canvas.fill_rect(
                super::types::Rect::from_xywh((w as f32) - grip, (h as f32) - grip, grip, grip),
                super::types::Rgba::rgb(112, 223, 122).with_alpha(90),
                4.0,
            );
        }

        let bgra = canvas.to_bgra();
        let hwnd = self.panels.get(key).map(|p| p.hwnd.hwnd).unwrap_or(0);
        let ulw_start = Instant::now();
        layered::present_bgra(&mut self.present_cache, hwnd, w, h, &bgra);
        let ulw_ms = ulw_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(p) = self.panels.get_mut(key) {
            p.last_present = Some(Instant::now());
            p.animating = animating;
        }
        Some((ulw_ms, false))
    }

    fn handle_pit_edit_input(
        &mut self,
        state: &StateHandle,
        geom: (i32, i32, i32, i32),
        w: i32,
        h: i32,
    ) {
        let left = hwnd::left_button_down();
        let right = hwnd::right_button_down();
        let middle = hwnd::middle_button_down();
        let shift = hwnd::shift_down();
        let cursor = hwnd::cursor_pos();
        let primary_pressed = left && !self.pit_prev_left;
        let primary_released = !left && self.pit_prev_left;
        let secondary_pressed = right && !self.pit_prev_right;

        let cursor_delta = match (cursor, self.pit_last_cursor) {
            (Some((cx, cy)), Some((px, py))) => ((cx - px) as f32, (cy - py) as f32),
            _ => (0.0, 0.0),
        };

        // Ctrl+scroll zoom.
        let wheel = hwnd::take_ctrl_wheel_delta();

        let pit_edit = state.read().map.pit_edit;
        if !pit_edit {
            self.pit_prev_left = left;
            self.pit_prev_right = right;
            self.pit_last_cursor = cursor;
            self.pit_press_on_handle = false;
            self.pit_press_moved = false;
            if state.read().map.pit_drag.is_some() {
                state.write().map.pit_drag = None;
            }
            return;
        }

        let plot = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(w as f32, h as f32),
        );
        let local_pos = cursor.map(|(cx, cy)| {
            egui::pos2((cx - geom.0) as f32, (cy - geom.1) as f32)
        });

        if primary_pressed {
            self.pit_press_on_handle = false;
            self.pit_press_moved = false;
        }
        if left && (cursor_delta.0.abs() > 3.0 || cursor_delta.1.abs() > 3.0) {
            self.pit_press_moved = true;
        }

        {
            let mut st = state.write();
            let cfg = st.config.clone();
            let (base, view) = widgets::map::pit_edit_view_xforms(cfg.as_ref(), &st.map, plot);
            let mirror = cfg.bool_key("map", "mirror", false);
            let rot = cfg.f64_key("map", "rotation", 0.0) as i32;
            let asphalt_w = cfg.f64_key("map", "asphalt_width", 12.0) as f32;
            let handle_r =
                ((asphalt_w * 0.35).max(4.0) * st.map.pit_edit_zoom.max(1.0).sqrt()).max(8.0);

            if wheel != 0 {
                if let Some(pos) = local_pos.filter(|p| plot.contains(*p)) {
                    let scroll_y = wheel as f32 * (40.0 / 120.0);
                    widgets::map::apply_pit_edit_wheel_zoom(
                        &mut st.map, &base, &view, pos, scroll_y,
                    );
                }
            }

            let buttons = widgets::map::PitEditButtons {
                primary_down: left,
                primary_pressed,
                primary_released,
                secondary_pressed,
                middle_down: middle,
                shift,
            };
            let allow_append =
                primary_released && !self.pit_press_on_handle && !self.pit_press_moved;
            let pressed_handle = widgets::map::tick_pit_edit_pointer(
                &mut st.map,
                &view,
                mirror,
                rot,
                handle_r,
                plot,
                local_pos,
                cursor_delta,
                &buttons,
                allow_append,
            );
            if pressed_handle {
                self.pit_press_on_handle = true;
            }
        }

        self.pit_prev_left = left;
        self.pit_prev_right = right;
        self.pit_last_cursor = cursor;
    }

    fn handle_edit_input(
        &mut self,
        state: &StateHandle,
        keys_layout: &[(String, PanelLayout)],
        edit_mode: bool,
    ) {
        if !edit_mode {
            self.drag = None;
            return;
        }
        let Some((cx, cy)) = hwnd::cursor_pos() else {
            return;
        };
        let down = hwnd::left_button_down();
        if !down {
            if self.drag.is_some() {
                self.drag = None;
                state.write().save_layout_to_preset();
            }
            return;
        }
        if let Some(drag) = &self.drag {
            let dx = cx - drag.origin_cursor.0;
            let dy = cy - drag.origin_cursor.1;
            let mut st = state.write();
            let lay = st.layout.entry(drag.key.clone()).or_default();
            match drag.kind {
                DragKind::Move => {
                    lay.x = drag.origin_layout.x + dx;
                    lay.y = drag.origin_layout.y + dy;
                }
                DragKind::Resize => {
                    lay.w = (drag.origin_layout.w + dx).max(90);
                    lay.h = (drag.origin_layout.h + dy).max(44);
                }
            }
            return;
        }
        for (key, lay) in keys_layout.iter().rev() {
            let grip = 22;
            let in_resize = cx >= lay.x + lay.w - grip
                && cy >= lay.y + lay.h - grip
                && cx <= lay.x + lay.w
                && cy <= lay.y + lay.h;
            let in_panel = cx >= lay.x && cy >= lay.y && cx <= lay.x + lay.w && cy <= lay.y + lay.h;
            if in_resize || in_panel {
                self.drag = Some(DragState {
                    key: key.clone(),
                    kind: if in_resize {
                        DragKind::Resize
                    } else {
                        DragKind::Move
                    },
                    origin_layout: lay.clone(),
                    origin_cursor: (cx, cy),
                });
                break;
            }
        }
    }
}

fn cadence(key: &str, animating: bool) -> u128 {
    if animating {
        return 16;
    }
    match key {
        "dash" | "inputs" | "radar" | "delta_bar" | "ers_hybrid" | "flags" => PEDAL_MS,
        "relative" | "standings" => TABLE_MS,
        "radio_tower" => RADIO_MS,
        "map" => MAP_MS,
        _ => DEFAULT_MS,
    }
}

#[allow(dead_code)]
fn _map_authoring(_: &MapAuthoring) {}
