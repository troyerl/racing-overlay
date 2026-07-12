//! Single transparent virtual-desktop host; widgets as positioned Areas.

use crate::config::WIDGET_KEYS;
use crate::state::{PanelLayout, StateHandle};
use crate::telemetry::{demo::DemoFeed, IrsdkReader};
use crate::widgets::{self, WidgetCtx};
use crate::win_click;
use eframe::egui::{self, Order, Sense, Vec2};
use std::sync::Arc;
use std::time::Instant;

const WINDOW_TITLE: &str = "GridGlance Overlay";

pub struct OverlayApp {
    state: StateHandle,
    demo: Option<DemoFeed>,
    irsdk: Option<IrsdkReader>,
    last_tick: Instant,
    virtual_origin: (f32, f32),
    last_passthrough: Option<bool>,
    last_hit_rects: Vec<(i32, i32, i32, i32)>,
}

impl OverlayApp {
    pub fn new(state: StateHandle, demo: bool) -> Self {
        let (vx, vy, _, _) = win_click::virtual_desktop_rect();
        Self {
            state,
            demo: if demo { Some(DemoFeed::new()) } else { None },
            irsdk: if demo { None } else { Some(IrsdkReader::new()) },
            last_tick: Instant::now(),
            virtual_origin: (vx as f32, vy as f32),
            last_passthrough: None,
            last_hit_rects: Vec::new(),
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
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_telemetry();
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        let (vx, vy) = self.virtual_origin;

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

        // Locked overlay: mouse passes through the full virtual desktop.
        // Edit mode: capture only over widget hit-regions (see SetWindowRgn below).
        let passthrough = running && click_through && !edit_mode;
        if self.last_passthrough != Some(passthrough) {
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(passthrough));
            if let Some(hwnd) = win_click::find_overlay_hwnd(WINDOW_TITLE) {
                win_click::set_click_through(hwnd, passthrough);
            }
            self.last_passthrough = Some(passthrough);
        }

        // Empty transparent root — no chrome when stopped or running.
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
            .show(ctx, |_| {});

        if !running {
            if !self.last_hit_rects.is_empty() {
                if let Some(hwnd) = win_click::find_overlay_hwnd(WINDOW_TITLE) {
                    win_click::set_hit_region(hwnd, &[]);
                }
                self.last_hit_rects.clear();
            }
            return;
        }

        let ppp = ctx.pixels_per_point();
        let mut hit_rects: Vec<(i32, i32, i32, i32)> = Vec::with_capacity(keys_layout.len());

        for (key, lay) in &keys_layout {
            let pos = egui::pos2(lay.x as f32 - vx, lay.y as f32 - vy);
            let size = Vec2::new(lay.w as f32, lay.h as f32);
            hit_rects.push((
                ((lay.x as f32 - vx) * ppp).round() as i32,
                ((lay.y as f32 - vy) * ppp).round() as i32,
                (lay.w as f32 * ppp).round() as i32,
                (lay.h as f32 * ppp).round() as i32,
            ));

            let state = self.state.clone();
            let key_clone = key.clone();
            egui::Area::new(egui::Id::new(("gg_panel", key.as_str())))
                .fixed_pos(pos)
                .order(Order::Foreground)
                .interactable(edit_mode || key.as_str() == "map")
                .show(ctx, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    ui.scope(|ui| {
                        ui.set_min_size(size);
                        ui.set_max_size(size);
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
                });
        }

        let desired = if edit_mode || !passthrough {
            hit_rects
        } else {
            // Full-window region is fine when everything is mouse-passthrough.
            Vec::new()
        };
        if desired != self.last_hit_rects {
            if let Some(hwnd) = win_click::find_overlay_hwnd(WINDOW_TITLE) {
                win_click::set_hit_region(hwnd, &desired);
            }
            self.last_hit_rects = desired;
        }
    }
}
