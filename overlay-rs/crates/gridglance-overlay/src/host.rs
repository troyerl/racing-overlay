//! Multi-viewport egui host for overlay panels.

use crate::config::WIDGET_KEYS;
use crate::state::{PanelLayout, StateHandle};
use crate::telemetry::{demo::DemoFeed, IrsdkReader};
use crate::widgets::{self, WidgetCtx};
use crate::win_click;
use eframe::egui::{self, ViewportBuilder, ViewportId};
use std::collections::HashSet;
use std::time::Instant;

pub struct OverlayApp {
    state: StateHandle,
    demo: Option<DemoFeed>,
    irsdk: Option<IrsdkReader>,
    last_tick: Instant,
    open_viewports: HashSet<String>,
}

impl OverlayApp {
    pub fn new(state: StateHandle, demo: bool) -> Self {
        Self {
            state,
            demo: if demo { Some(DemoFeed::new()) } else { None },
            irsdk: if demo { None } else { Some(IrsdkReader::new()) },
            last_tick: Instant::now(),
            open_viewports: HashSet::new(),
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
        self.state.write().frame = frame;
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_telemetry();
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        // Snapshot visibility + layout under lock, then paint.
        let (running, edit_mode, click_through, keys_layout, generation) = {
            let st = self.state.read();
            let mut items = Vec::new();
            for key in WIDGET_KEYS {
                if !st.running {
                    break;
                }
                if !st.config.widget_shown(key) && !st.edit_mode {
                    continue;
                }
                let lay = st
                    .layout
                    .get(*key)
                    .cloned()
                    .unwrap_or(PanelLayout::default());
                items.push(((*key).to_string(), lay));
            }
            (
                st.running,
                st.edit_mode,
                st.click_through && !st.map.interactive,
                items,
                st.config.generation,
            )
        };
        let _ = generation;

        // Root viewport stays hidden / tiny — panels are immediate viewports.
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                if !running {
                    ui.label("Overlay stopped (IPC overlay.start to resume)");
                } else {
                    ui.label("GridGlance overlay (Rust)");
                }
            });

        let mut still_open = HashSet::new();
        for (key, lay) in keys_layout {
            still_open.insert(key.clone());
            let vid = ViewportId::from_hash_of(&key);
            let builder = ViewportBuilder::default()
                .with_title(format!("GridGlance — {key}"))
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_position(egui::pos2(lay.x as f32, lay.y as f32))
                .with_inner_size([lay.w as f32, lay.h as f32]);

            let state = self.state.clone();
            let key_clone = key.clone();
            ctx.show_viewport_immediate(vid, builder, move |vp_ctx, _class| {
                // Click-through when not editing
                if let Some(viewport) = vp_ctx.input(|i| {
                    i.viewport()
                        .native_pixels_per_point
                        .map(|_| ()) // force read
                }) {
                    let _ = viewport;
                }
                #[cfg(windows)]
                {
                    // Best-effort: egui doesn't expose HWND directly in all backends;
                    // skip if unavailable.
                    let _ = click_through;
                    let _ = win_click::set_click_through;
                }
                #[cfg(not(windows))]
                {
                    let _ = (click_through, win_click::set_click_through as fn(isize, bool));
                }

                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
                    .show(vp_ctx, |ui| {
                        let (cfg, frame) = {
                            let st = state.read();
                            (st.config.clone(), st.frame.clone())
                        };
                        {
                            let mut st = state.write();
                            let mut wctx = WidgetCtx {
                                cfg: &cfg,
                                frame: &frame,
                                edit_mode,
                                map: &mut st.map,
                            };
                            widgets::paint(ui, &key_clone, &mut wctx);
                        }

                        // Drag to move in edit mode
                        if edit_mode {
                            let resp = ui.interact(
                                ui.max_rect(),
                                ui.id().with("drag"),
                                egui::Sense::drag(),
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
            self.open_viewports.insert(key);
        }

        // Close viewports that are no longer shown
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
        }
        self.open_viewports = still_open;
    }
}
