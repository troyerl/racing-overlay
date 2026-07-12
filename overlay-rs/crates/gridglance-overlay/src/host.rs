//! Multi-viewport egui host for overlay panels.

use crate::config::WIDGET_KEYS;
use crate::state::{PanelLayout, StateHandle};
use crate::telemetry::{demo::DemoFeed, IrsdkReader};
use crate::widgets::{self, WidgetCtx};
use crate::win_click::{self, PanelShape};
use eframe::egui::{self, Sense, ViewportBuilder, ViewportId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

pub struct OverlayApp {
    state: StateHandle,
    demo: Option<DemoFeed>,
    irsdk: Option<IrsdkReader>,
    last_tick: Instant,
    open_viewports: HashSet<String>,
    /// Last applied Win32 shape signature per panel key.
    applied_shape: HashMap<String, (i32, i32, i32, bool)>,
    last_click_through: HashMap<String, bool>,
}

impl OverlayApp {
    pub fn new(state: StateHandle, demo: bool) -> Self {
        Self {
            state,
            demo: if demo { Some(DemoFeed::new()) } else { None },
            irsdk: if demo { None } else { Some(IrsdkReader::new()) },
            last_tick: Instant::now(),
            open_viewports: HashSet::new(),
            applied_shape: HashMap::new(),
            last_click_through: HashMap::new(),
        }
    }

    fn tick_telemetry(&mut self) {
        if self.last_tick.elapsed().as_millis() < 16 {
            return;
        }
        self.last_tick = Instant::now();
        let frame = if let Some(demo) = &self.demo {
            demo.tick()
        } else if let Some(ir) = &mut self.irsdk {
            ir.tick()
        } else {
            return;
        };
        self.state.write().frame = Arc::new(frame);
    }

    fn panel_shape_for(
        key: &str,
        lay: &PanelLayout,
        show_panel: bool,
        radius_frac: f32,
        ppp: f32,
    ) -> (PanelShape, bool, (i32, i32, i32, bool)) {
        let w = (lay.w as f32 * ppp).round().max(1.0) as i32;
        let h = (lay.h as f32 * ppp).round().max(1.0) as i32;
        let no_panel = matches!(key, "radar" | "map") && !show_panel;
        let radius = ((lay.h as f32 * radius_frac).max(8.0) * ppp).round() as i32;
        let shape = if no_panel && key == "radar" {
            PanelShape::Ellipse { w, h }
        } else if no_panel {
            PanelShape::RoundRect {
                w,
                h,
                radius: radius.max(12),
            }
        } else {
            PanelShape::RoundRect { w, h, radius }
        };
        let sig = (w, h, if no_panel { -1 } else { radius }, no_panel);
        (shape, no_panel, sig)
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_telemetry();
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

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
                    let show_panel = st.config.bool_key(
                        key,
                        "show_panel",
                        !matches!(*key, "radar" | "map"),
                    );
                    let radius_frac = st.config.f64_key(key, "corner_radius_frac", 0.08) as f32;
                    items.push(((*key).to_string(), lay, show_panel, radius_frac));
                }
            }
            (
                st.running,
                st.edit_mode,
                st.click_through && !st.map.interactive,
                items,
            )
        };

        // Root stays off-screen / empty.
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
            .show(ctx, |_| {});

        if !running {
            let to_close: Vec<_> = self.open_viewports.iter().cloned().collect();
            for key in to_close {
                ctx.send_viewport_cmd_to(
                    ViewportId::from_hash_of(&key),
                    egui::ViewportCommand::Close,
                );
                self.open_viewports.remove(&key);
                self.applied_shape.remove(&key);
                self.last_click_through.remove(&key);
            }
            return;
        }

        let ppp = ctx.pixels_per_point();
        let mut still_open = HashSet::new();
        let passthrough = click_through && !edit_mode;

        for (key, lay, show_panel, radius_frac) in keys_layout {
            still_open.insert(key.clone());
            let vid = ViewportId::from_hash_of(&key);
            let title = win_click::panel_title(&key);
            let (shape, chroma, sig) =
                Self::panel_shape_for(&key, &lay, show_panel, radius_frac, ppp);

            let builder = ViewportBuilder::default()
                .with_title(title.clone())
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_mouse_passthrough(passthrough)
                .with_position(egui::pos2(lay.x as f32, lay.y as f32))
                .with_inner_size([lay.w as f32, lay.h as f32]);

            let state = self.state.clone();
            let key_clone = key.clone();
            let clear = if chroma {
                let (r, g, b) = win_click::CHROMA_RGB;
                egui::Color32::from_rgb(r, g, b)
            } else {
                egui::Color32::TRANSPARENT
            };

            ctx.show_viewport_immediate(vid, builder, move |vp_ctx, _class| {
                // Match clear to chroma or transparent for this panel.
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(clear))
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
                            let resp = ui.interact(
                                ui.max_rect(),
                                ui.id().with("drag"),
                                Sense::drag(),
                            );
                            if resp.dragged() {
                                let delta = resp.drag_delta();
                                let mut st = state.write();
                                if let Some(lay) = st.layout.get_mut(&key_clone) {
                                    lay.x += delta.x as i32;
                                    lay.y += delta.y as i32;
                                }
                            }
                            if resp.drag_stopped() {
                                state.write().save_layout();
                            }
                        }
                    });

                if vp_ctx.input(|i| i.viewport().close_requested()) {
                    // Ignore close — overlay panels aren't user-dismissible.
                }
            });

            // Win32 transparency / region (when HWND is available).
            if self.applied_shape.get(&key) != Some(&sig) {
                if let Some(hwnd) = win_click::find_overlay_hwnd(&title) {
                    win_click::apply_panel_transparency(hwnd, shape, chroma);
                    self.applied_shape.insert(key.clone(), sig);
                }
            }
            if self.last_click_through.get(&key) != Some(&passthrough) {
                if let Some(hwnd) = win_click::find_overlay_hwnd(&title) {
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
                egui::ViewportCommand::Close,
            );
            self.open_viewports.remove(&key);
            self.applied_shape.remove(&key);
            self.last_click_through.remove(&key);
        }
        self.open_viewports = still_open;
    }
}
