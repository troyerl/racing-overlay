//! In-overlay Settings UI (egui). Schema-driven pages matching Python ConfigEditor.

mod scan;
mod schema;
mod theme;
mod widgets;

pub use schema::{
    help_text, is_skipped, matches_search, nav_for_tab, pretty_key, setting_groups, tab_color,
    top_tab_for, TopTab,
};

use crate::config::{parse_color_str, ConfigContext};
use crate::state::StateHandle;
use egui::{self, Color32, RichText, ScrollArea, Ui, UiBuilder, Vec2b};
use schema::group_default_open;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::{paint_background, MUTED, NAV_SECTION, NAV_WIDTH, TITLE};
use widgets::{
    accordion, button_kind, color_button, enable_card, enable_card_row, nav_item,
    number_row, preset_button, search_field, setting_row, status_line, styled_combo, text_field,
    toggle_switch, top_tab_button, top_tabs_frame, ButtonKind,
};

const WINDOW_TITLE: &str = "GridGlance Settings";
/// Status line + footer + spacing reserved below the nav/content body.
const FOOTER_RESERVE: f32 = 72.0;

/// Ephemeral Settings UI state (search, tabs, status flash).
#[derive(Debug, Clone, Default)]
pub struct SettingsUi {
    pub search: String,
    pub top_tab: TopTab,
    pub status: String,
    pub status_deadline: Option<Instant>,
    pub new_preset_input: String,
    pub file_path_input: String,
    pub html_path: String,
    pub demo_track_id: i64,
    pub demo_track_status: String,
    pub pro_drivers: Vec<Value>,
    pub pro_name: String,
    pub pro_aliases: String,
    pub pro_sel: Option<String>,
    pub admin_loaded: bool,
    pub update_url: Option<String>,
    pub dg_sel: Option<String>,
    pub dg_name: String,
    pub dg_icon: String,
    pub dg_color: String,
    pub dg_members: String,
}

impl SettingsUi {
    pub fn flash(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_deadline = Some(Instant::now() + Duration::from_millis(2500));
    }

    pub fn tick_status(&mut self) {
        if let Some(deadline) = self.status_deadline {
            if Instant::now() > deadline {
                self.status.clear();
                self.status_deadline = None;
            }
        }
    }
}

/// Paint the Settings window contents.
pub fn paint(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    section: &mut String,
    dirty: &mut bool,
) {
    ui_state.tick_status();
    paint_background(ui);

    egui::Frame::new()
        .fill(Color32::TRANSPARENT)
        .inner_margin(egui::Margin {
            left: 18,
            right: 18,
            top: 16,
            bottom: 14,
        })
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.label(
                    RichText::new("Overlay Settings")
                        .size(21.0)
                        .strong()
                        .color(TITLE),
                );
                ui.label(
                    RichText::new("Customize every widget - changes apply live")
                        .size(11.0)
                        .color(MUTED),
                );

                paint_preset_bar(ui, state, ui_state, dirty);

                paint_profile_row(ui, state, ui_state);

                let mut search = ui_state.search.clone();
                let search_resp = search_field(ui, &mut search, "Search settings...");
                if search_resp.changed() {
                    ui_state.search = search;
                }

                top_tabs_frame(ui, |ui| {
                    ui.horizontal(|ui| {
                        if top_tab_button(
                            ui,
                            TopTab::Widgets.label(),
                            ui_state.top_tab == TopTab::Widgets,
                        )
                        .clicked()
                        {
                            ui_state.top_tab = TopTab::Widgets;
                            if top_tab_for(section) == TopTab::Settings {
                                *section = "relative".into();
                            }
                        }
                        if top_tab_button(
                            ui,
                            TopTab::Settings.label(),
                            ui_state.top_tab == TopTab::Settings,
                        )
                        .clicked()
                        {
                            ui_state.top_tab = TopTab::Settings;
                            if top_tab_for(section) == TopTab::Widgets {
                                *section = "__general__".into();
                            }
                        }
                    });
                });

                // Bound the body so nav/content ScrollAreas fill remaining height
                // (Python: root.addLayout(body, 1) + QScrollArea on the nav rail).
                let avail = ui.available_rect_before_wrap();
                let body_h = (avail.height() - FOOTER_RESERVE).max(200.0);
                let body_rect =
                    egui::Rect::from_min_size(avail.min, egui::vec2(avail.width(), body_h));
                ui.allocate_new_ui(UiBuilder::new().max_rect(body_rect), |ui| {
                    ui.set_min_size(body_rect.size());
                    ui.horizontal_top(|ui| {
                        egui::Frame::new()
                            .fill(theme::rail_bg())
                            .stroke(egui::Stroke::new(1.0_f32, theme::NAV_BORDER))
                            .corner_radius(14.0)
                            .inner_margin(egui::Margin::symmetric(8, 12))
                            .show(ui, |ui| {
                                // Break out of horizontal_top so nav items stack vertically.
                                ui.vertical(|ui| {
                                    ui.set_width(NAV_WIDTH);
                                    ui.set_height(ui.available_height());
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    ScrollArea::vertical()
                                        .id_salt("settings_nav")
                                        .auto_shrink(Vec2b::new(true, false))
                                        .show(ui, |ui| {
                                            paint_nav(ui, state, ui_state, section);
                                        });
                                });
                            });

                        ui.add_space(12.0);
                        egui::Frame::new()
                            .fill(Color32::TRANSPARENT)
                            .inner_margin(egui::Margin {
                                left: 6,
                                right: 10,
                                top: 4,
                                bottom: 8,
                            })
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.set_height(ui.available_height());
                                    ScrollArea::vertical()
                                        .id_salt("settings_content")
                                        .auto_shrink(Vec2b::new(false, false))
                                        .show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            paint_page(ui, state, ui_state, section, dirty);
                                        });
                                });
                            });
                    });
                });
                // Advance past the allocated body so status/footer sit below it.
                ui.advance_cursor_after_rect(body_rect);

                status_line(ui, &ui_state.status);
                paint_footer(ui, state, ui_state, dirty);
            });
        });
}

fn paint_preset_bar(ui: &mut Ui, state: &StateHandle, ui_state: &mut SettingsUi, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (presets, active, default_preset) = {
            let st = state.read();
            (
                st.config.preset_names(),
                st.config.active_preset.clone(),
                st.config.default_preset(),
            )
        };
        ui.label(RichText::new("Preset").size(12.0).strong().color(MUTED));
        if let Some(name) = styled_combo(ui, "settings_preset", &active, &presets, 160.0) {
            if let Some(mut st) = state.try_write() {
                match Arc::make_mut(&mut st.config).set_active_preset(&name) {
                    Ok(()) => {
                        st.apply_effective_context();
                        *dirty = false;
                        ui_state.flash(format!("Switched to “{name}”"));
                    }
                    Err(e) => ui_state.flash(e.to_string()),
                }
            }
        }

        if preset_button(ui, "New").clicked() {
            let name = if ui_state.new_preset_input.trim().is_empty() {
                format!("Preset {}", presets.len() + 1)
            } else {
                ui_state.new_preset_input.clone()
            };
            if let Some(mut st) = state.try_write() {
                match Arc::make_mut(&mut st.config).create_preset(&name) {
                    Ok(()) => {
                        ui_state.new_preset_input.clear();
                        st.apply_effective_context();
                        *dirty = false;
                        ui_state.flash(format!("Created “{name}”"));
                    }
                    Err(e) => ui_state.flash(e.to_string()),
                }
            }
        }
        if preset_button(ui, "Duplicate").clicked() {
            let to = format!("{active} copy");
            if let Some(mut st) = state.try_write() {
                match Arc::make_mut(&mut st.config).duplicate_preset(&active, &to) {
                    Ok(()) => {
                        st.apply_effective_context();
                        *dirty = false;
                        ui_state.flash(format!("Duplicated as “{to}”"));
                    }
                    Err(e) => ui_state.flash(e.to_string()),
                }
            }
        }
        if button_kind(ui, "Delete", ButtonKind::Danger).clicked() {
            if let Some(mut st) = state.try_write() {
                match Arc::make_mut(&mut st.config).delete_preset(&active) {
                    Ok(()) => {
                        st.apply_effective_context();
                        *dirty = false;
                        ui_state.flash("Preset deleted");
                    }
                    Err(e) => ui_state.flash(e.to_string()),
                }
            }
        }

        // Stretch then Default toggle (label left of switch).
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut is_default = default_preset.as_deref() == Some(active.as_str());
            if toggle_switch(
                ui,
                &mut is_default,
                theme::ACCENT,
                ui.id().with("default_preset"),
            )
            .changed()
                && is_default
            {
                if let Some(mut st) = state.try_write() {
                    match Arc::make_mut(&mut st.config).set_default_preset(&active) {
                        Ok(()) => ui_state.flash("Default preset updated"),
                        Err(e) => ui_state.flash(e.to_string()),
                    }
                }
            }
            ui.label(RichText::new("Default preset").color(theme::ROW_LABEL));
        });
    });

    let active = state.read().config.active_preset.clone();
    accordion(ui, "Advanced", theme::MUTED, false, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Preset name").size(11.0).color(MUTED));
            text_field(
                ui,
                &mut ui_state.new_preset_input,
                "for New / Rename / Import",
                ui.available_width().min(280.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("File path").size(11.0).color(MUTED));
            text_field(
                ui,
                &mut ui_state.file_path_input,
                ".ggprofile.json path",
                ui.available_width().min(360.0),
            );
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if preset_button(ui, "Export").clicked() {
                let path = ui_state.file_path_input.trim();
                if path.is_empty() {
                    ui_state.flash("Type an export path above");
                } else {
                    let payload = state.read().config.export_preset_value(&active);
                    match payload.and_then(|v| {
                        std::fs::write(path, serde_json::to_string_pretty(&v)?)
                            .map_err(anyhow::Error::from)
                    }) {
                        Ok(()) => ui_state.flash("Preset exported"),
                        Err(e) => ui_state.flash(e.to_string()),
                    }
                }
            }
            if preset_button(ui, "Import").clicked() {
                let path = ui_state.file_path_input.trim();
                let name = ui_state.new_preset_input.trim();
                if path.is_empty() || name.is_empty() {
                    ui_state.flash("Type import path + name above");
                } else {
                    match std::fs::read_to_string(path)
                        .map_err(anyhow::Error::from)
                        .and_then(|text| {
                            serde_json::from_str::<Value>(&text).map_err(anyhow::Error::from)
                        }) {
                        Ok(payload) => {
                            if let Some(mut st) = state.try_write() {
                                match Arc::make_mut(&mut st.config)
                                    .import_preset_value(name, &payload, false)
                                {
                                    Ok(()) => {
                                        st.apply_effective_context();
                                        *dirty = false;
                                        ui_state.flash("Preset imported");
                                    }
                                    Err(e) => ui_state.flash(e.to_string()),
                                }
                            }
                        }
                        Err(e) => ui_state.flash(e.to_string()),
                    }
                }
            }
            if preset_button(ui, "Rename").clicked() {
                let to = ui_state.new_preset_input.trim().to_string();
                if to.is_empty() {
                    ui_state.flash("Type a new name above");
                } else if let Some(mut st) = state.try_write() {
                    match Arc::make_mut(&mut st.config).rename_preset(&active, &to) {
                        Ok(()) => {
                            ui_state.new_preset_input.clear();
                            ui_state.flash(format!("Renamed to “{to}”"));
                        }
                        Err(e) => ui_state.flash(e.to_string()),
                    }
                }
            }
        });
        if *dirty {
            ui.label(RichText::new("unsaved").color(theme::YELLOW));
        }
    });
}

fn paint_profile_row(ui: &mut Ui, state: &StateHandle, ui_state: &mut SettingsUi) {
    ui.horizontal(|ui| {
        let current = state.read().effective_context();
        let selected = current.label().to_string();
        let options = vec!["On track".to_string(), "In garage".to_string()];
        ui.label(RichText::new("Profile").size(12.0).strong().color(MUTED));
        if let Some(next) = styled_combo(ui, "settings_profile", &selected, &options, 150.0) {
            let context = if next == "In garage" {
                ConfigContext::Garage
            } else {
                ConfigContext::Race
            };
            if let Some(mut st) = state.try_write() {
                st.set_preview_context(Some(context));
                ui_state.flash(format!("Editing {}", context.label()));
            }
        }
    });
    ui.label(
        RichText::new(
            "On-track and garage overrides are edited separately; live preview follows telemetry.",
        )
        .size(11.0)
        .color(MUTED),
    );
}

fn paint_nav(ui: &mut Ui, state: &StateHandle, ui_state: &SettingsUi, section: &mut String) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        let q = ui_state.search.to_ascii_lowercase();
        let mut last_group = String::new();
        for (key, title, group) in nav_for_tab(ui_state.top_tab) {
            if !q.is_empty() {
                let hay = format!("{key} {title} {group}").to_ascii_lowercase();
                if !hay.contains(&q) && *section != key {
                    continue;
                }
            }
            if !group.is_empty() && group != last_group {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(theme::spaced_upper(&group))
                        .size(10.0)
                        .strong()
                        .color(NAV_SECTION),
                );
                ui.add_space(2.0);
                last_group = group.clone();
            }
            let accent = theme::parse_hex(tab_color(&key));
            let dot_on = widget_shown(state, &key);
            let selected = *section == key;
            if nav_item(
                ui,
                &title,
                selected,
                accent,
                dot_on,
                ui.id().with(("nav", key.as_str())),
            )
            .clicked()
            {
                *section = key.clone();
            }
        }
    });
}

fn widget_shown(state: &StateHandle, key: &str) -> bool {
    if key == "__general__" || key == "__app__" || key == "__widgets__" {
        return true;
    }
    let st = state.read();
    st.config.widget_shown(key)
}

fn paint_page(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    section: &str,
    dirty: &mut bool,
) {
    let accent = theme::parse_hex(tab_color(section));
    match section {
        "__general__" => paint_general(ui, state, ui_state, dirty, accent),
        "__app__" => paint_app(ui, state, ui_state, dirty, accent),
        "__scan__" => scan::paint_track_scan(ui, state, ui_state, accent),
        other => paint_widget_section(ui, state, ui_state, other, dirty, accent),
    }
}

fn paint_general(
    ui: &mut Ui,
    state: &StateHandle,
    _ui_state: &SettingsUi,
    dirty: &mut bool,
    accent: Color32,
) {
    ui.label(RichText::new("General").size(16.0).strong().color(TITLE));
    ui.label(
        RichText::new("Global overlay preferences.")
            .size(11.0)
            .color(MUTED),
    );
    ui.add_space(8.0);

    enable_card(ui, "Global", accent, |ui| {
        let mut units = {
            let st = state.read();
            st.config.global_str("units", "metric")
        };
        let unit_help = help_text("__general__", "units");
        setting_row(ui, "Units", unit_help, |ui| {
            let unit_options = vec!["metric".to_string(), "imperial".to_string()];
            if let Some(next) = styled_combo(ui, "units", &units, &unit_options, 150.0) {
                units = next;
                set_global(state, "units", json!(units), dirty);
            }
        });

        let mut text_scale = {
            let st = state.read();
            st.config
                .cfg
                .get("text_scale")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0) as f32
        };
        if number_row(
            ui,
            "Text scale",
            &mut text_scale,
            0.7..=1.5,
            0.05,
            accent,
            help_text("__general__", "text_scale"),
        ) {
            set_global(state, "text_scale", json!(text_scale as f64), dirty);
        }

        let mut start_on = {
            let st = state.read();
            st.config
                .cfg
                .get("start_overlay_on_launch")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };
        setting_row(
            ui,
            "Start overlay on launch",
            help_text("__general__", "start_overlay_on_launch"),
            |ui| {
                if toggle_switch(ui, &mut start_on, accent, ui.id().with("start_launch")).changed()
                {
                    set_global(state, "start_overlay_on_launch", json!(start_on), dirty);
                }
            },
        );
    });

    ui.add_space(8.0);
    enable_card(ui, "Preset bindings", accent, |ui| {
        ui.label(
            RichText::new("Cars and leagues that auto-activate the current preset.")
                .size(11.0)
                .color(MUTED),
        );
        let (mut cars_str, mut leagues_str) = {
            let st = state.read();
            (
                st.config.active_preset_cars().join(", "),
                st.config
                    .active_preset_leagues()
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };
        setting_row(ui, "Cars (paths)", None, |ui| {
            if text_field(ui, &mut cars_str, "e.g. gt3/ferrari296", 260.0).changed() {
                let cars: Vec<String> = cars_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if let Some(mut st) = state.try_write() {
                    Arc::make_mut(&mut st.config).set_active_preset_cars(cars);
                    *dirty = true;
                }
            }
        });
        setting_row(ui, "Leagues (IDs)", None, |ui| {
            if text_field(ui, &mut leagues_str, "e.g. 1234, 5678", 260.0).changed() {
                let leagues: Vec<i32> = leagues_str
                    .split(|c: char| !c.is_ascii_digit() && c != '-')
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if let Some(mut st) = state.try_write() {
                    Arc::make_mut(&mut st.config).set_active_preset_leagues(leagues);
                    *dirty = true;
                }
            }
        });
    });

    ui.add_space(8.0);
    enable_card(ui, "Overlay", accent, |ui| {
        let mut running = state.read().running;
        setting_row(ui, "Panels visible", None, |ui| {
            if toggle_switch(ui, &mut running, accent, ui.id().with("running")).changed() {
                if let Some(mut st) = state.try_write() {
                    st.running = running;
                }
            }
        });
    });
}

fn paint_app(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    dirty: &mut bool,
    accent: Color32,
) {
    ui.label(RichText::new("App").size(16.0).strong().color(TITLE));
    ui.add_space(8.0);
    scan::ensure_admin_loaded(ui_state);
    scan::paint_about(ui, ui_state, accent);
    ui.add_space(8.0);
    enable_card(ui, "Launch", accent, |ui| {
        let mut start_overlay = state
            .read()
            .config
            .cfg
            .get("start_overlay_on_launch")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        setting_row(
            ui,
            "Start overlay on launch",
            help_text("__app__", "start_overlay_on_launch")
                .or_else(|| help_text("__general__", "start_overlay_on_launch")),
            |ui| {
                if toggle_switch(
                    ui,
                    &mut start_overlay,
                    accent,
                    ui.id().with("app_start_overlay"),
                )
                .changed()
                {
                    set_global(
                        state,
                        "start_overlay_on_launch",
                        json!(start_overlay),
                        dirty,
                    );
                }
            },
        );
        let mut start_login = state
            .read()
            .config
            .cfg
            .get("start_at_login")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        setting_row(
            ui,
            "Start at login",
            help_text("__app__", "start_at_login"),
            |ui| {
                if toggle_switch(
                    ui,
                    &mut start_login,
                    accent,
                    ui.id().with("app_start_login"),
                )
                .changed()
                {
                    set_global(state, "start_at_login", json!(start_login), dirty);
                    let args = if start_overlay { "--no-settings" } else { "" };
                    if let Err(e) = crate::autostart::set_enabled(start_login, args) {
                        ui_state.flash(e.to_string());
                    }
                }
            },
        );
        let mut check_upd = state
            .read()
            .config
            .cfg
            .get("check_updates_on_launch")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        setting_row(
            ui,
            "Check for updates on launch",
            help_text("__app__", "check_updates_on_launch"),
            |ui| {
                if toggle_switch(
                    ui,
                    &mut check_upd,
                    accent,
                    ui.id().with("app_check_updates"),
                )
                .changed()
                {
                    set_global(state, "check_updates_on_launch", json!(check_upd), dirty);
                }
            },
        );
    });
    ui.add_space(8.0);
    enable_card(ui, "Auto-switch presets", accent, |ui| {
        for (key, label) in [
            ("auto_switch_by_league", "Switch by league"),
            ("auto_switch_by_car", "Switch by car"),
            ("auto_switch_to_default", "Fall back to default preset"),
        ] {
            let mut on = state.read().config.auto_switch_flag(key);
            setting_row(ui, label, None, |ui| {
                if toggle_switch(ui, &mut on, accent, ui.id().with(("auto_switch", key))).changed()
                {
                    if let Some(mut st) = state.try_write() {
                        Arc::make_mut(&mut st.config).set_auto_switch_flag(key, on);
                        *dirty = true;
                    }
                }
            });
        }
    });
    paint_driver_groups(ui, state, ui_state, dirty, accent);
    scan::paint_cloud_admin(ui, ui_state, accent);
}

fn paint_driver_groups(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    dirty: &mut bool,
    accent: Color32,
) {
    ui.add_space(8.0);
    enable_card(ui, "Driver groups", accent, |ui| {
        ui.label(
            RichText::new("Local name groups with icons for Relative / Standings / Radio.")
                .size(11.0)
                .color(MUTED),
        );

        let mut groups = {
            let st = state.read();
            crate::driver_groups::normalize_driver_groups(
                st.config.cfg.get("driver_groups").unwrap_or(&json!([])),
            )
        };

        let names: Vec<String> = groups
            .iter()
            .filter_map(|g| {
                g.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        let selected = ui_state
            .dg_sel
            .clone()
            .filter(|s| names.contains(s))
            .or_else(|| names.first().cloned())
            .unwrap_or_default();

        if !names.is_empty() {
            if let Some(next) = styled_combo(ui, "dg_list", &selected, &names, 200.0) {
                ui_state.dg_sel = Some(next.clone());
                if let Some(g) = groups
                    .iter()
                    .find(|g| g.get("name").and_then(|n| n.as_str()) == Some(next.as_str()))
                {
                    ui_state.dg_name = next;
                    ui_state.dg_icon = g
                        .get("icon")
                        .and_then(|i| i.as_str())
                        .unwrap_or("league")
                        .to_string();
                    ui_state.dg_color = g
                        .get("color")
                        .and_then(|c| c.as_str())
                        .unwrap_or("#5bb8ff")
                        .to_string();
                    ui_state.dg_members = g
                        .get("members")
                        .and_then(|m| m.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|e| e.get("name").and_then(|n| n.as_str()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                }
            }
        }

        setting_row(ui, "Group name", None, |ui| {
            let _ = text_field(ui, &mut ui_state.dg_name, "League mates", 180.0);
        });
        let icon_opts: Vec<String> = crate::driver_groups::DRIVER_GROUP_ICONS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        setting_row(ui, "Icon", None, |ui| {
            if ui_state.dg_icon.is_empty() {
                ui_state.dg_icon = "league".into();
            }
            if let Some(next) = styled_combo(ui, "dg_icon", &ui_state.dg_icon, &icon_opts, 140.0) {
                ui_state.dg_icon = next;
            }
        });
        setting_row(ui, "Color", None, |ui| {
            let _ = text_field(ui, &mut ui_state.dg_color, "#5bb8ff", 120.0);
        });
        setting_row(ui, "Members", None, |ui| {
            let _ = text_field(ui, &mut ui_state.dg_members, "comma-separated names", 260.0);
        });

        ui.horizontal(|ui| {
            if button_kind(ui, "Add / Update", ButtonKind::GhostAccent).clicked() {
                let name = ui_state.dg_name.trim().to_string();
                if !name.is_empty() {
                    let members: Vec<Value> = ui_state
                        .dg_members
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(|s| json!({ "name": s, "aliases": [] }))
                        .collect();
                    let entry = json!({
                        "name": name,
                        "icon": ui_state.dg_icon,
                        "color": ui_state.dg_color,
                        "members": members,
                    });
                    if let Some(pos) = groups
                        .iter()
                        .position(|g| g.get("name").and_then(|n| n.as_str()) == Some(name.as_str()))
                    {
                        groups[pos] = entry;
                    } else {
                        groups.push(entry);
                    }
                    ui_state.dg_sel = Some(name);
                    set_global(state, "driver_groups", json!(groups), dirty);
                    ui_state.flash("Driver group saved");
                }
            }
            if button_kind(ui, "Remove", ButtonKind::Warn).clicked() {
                if let Some(sel) = ui_state.dg_sel.clone() {
                    groups.retain(|g| g.get("name").and_then(|n| n.as_str()) != Some(sel.as_str()));
                    set_global(state, "driver_groups", json!(groups), dirty);
                    ui_state.dg_sel = None;
                    ui_state.dg_name.clear();
                    ui_state.dg_members.clear();
                    ui_state.flash("Driver group removed");
                }
            }
            if button_kind(ui, "Import from results…", ButtonKind::Default).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file()
                {
                    match std::fs::read_to_string(&path)
                        .map_err(|e| e.into())
                        .and_then(|t| crate::driver_groups::parse_event_result_names(&t))
                    {
                        Ok(names) => {
                            let existing: Vec<Value> = ui_state
                                .dg_members
                                .split(',')
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .map(|s| json!({ "name": s, "aliases": [] }))
                                .collect();
                            let (merged, added, skipped) =
                                crate::driver_groups::merge_names_into_members(&existing, &names);
                            ui_state.dg_members = merged
                                .iter()
                                .filter_map(|e| e.get("name").and_then(|n| n.as_str()))
                                .collect::<Vec<_>>()
                                .join(", ");
                            ui_state.flash(format!("Imported +{added} (skipped {skipped})"));
                        }
                        Err(e) => ui_state.flash(e.to_string()),
                    }
                }
            }
        });
    });
}

fn paint_widget_section(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &SettingsUi,
    section: &str,
    dirty: &mut bool,
    accent: Color32,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(pretty_key(section))
                .size(16.0)
                .strong()
                .color(TITLE),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if button_kind(ui, "Reset to defaults", ButtonKind::GhostAccent).clicked() {
                if let Some(mut st) = state.try_write() {
                    let context = st.effective_context();
                    Arc::make_mut(&mut st.config).reset_section(context, section);
                    *dirty = true;
                }
            }
        });
    });
    ui.add_space(4.0);

    let values = section_values(state, section);
    let mut show = values
        .get("show")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if enable_card_row(
        ui,
        &format!("Enable {}", pretty_key(section)),
        "Show this overlay panel",
        accent,
        &mut show,
        ui.id().with((section, "show")),
    ) {
        set_section_key(state, section, "show", json!(show), dirty);
    }
    ui.add_space(6.0);
    if !show {
        ui.label(
            RichText::new("Enable this widget to edit its settings.")
                .size(11.0)
                .color(MUTED),
        );
        return;
    }

    let groups = setting_groups(section);
    if !groups.is_empty() {
        let mut grouped: HashMap<&str, &Value> = HashMap::new();
        for (k, v) in &values {
            grouped.insert(k.as_str(), v);
        }
        let mut used = std::collections::HashSet::new();
        for (group_title, keys) in groups {
            let visible_keys: Vec<&str> = keys
                .iter()
                .copied()
                .filter(|k| {
                    *k != "show"
                        && !is_skipped(section, k)
                        && grouped.contains_key(k)
                        && matches_search(section, k, &ui_state.search)
                })
                .collect();
            if visible_keys.is_empty() {
                continue;
            }
            accordion(
                ui,
                group_title,
                accent,
                group_default_open(group_title),
                |ui| {
                    for key in visible_keys {
                        used.insert(key);
                        if let Some(val) = grouped.get(key) {
                            paint_value(ui, state, section, key, val, dirty, accent, ui_state);
                        }
                    }
                },
            );
        }
        let leftovers: Vec<_> = values
            .iter()
            .filter(|(k, _)| {
                k.as_str() != "show"
                    && !is_skipped(section, k)
                    && !used.contains(k.as_str())
                    && matches_search(section, k, &ui_state.search)
            })
            .collect();
        if !leftovers.is_empty() {
            accordion(ui, "Other", accent, false, |ui| {
                for (key, val) in leftovers {
                    paint_value(ui, state, section, key, val, dirty, accent, ui_state);
                }
            });
        }
    } else {
        for (key, value) in &values {
            if key == "show" || is_skipped(section, key) {
                continue;
            }
            if !matches_search(section, key, &ui_state.search) {
                continue;
            }
            paint_value(ui, state, section, key, value, dirty, accent, ui_state);
        }
    }
}

fn section_values(state: &StateHandle, section: &str) -> HashMap<String, Value> {
    let st = state.read();
    match st.config.section(section) {
        Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => HashMap::new(),
    }
}

fn paint_value(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    value: &Value,
    dirty: &mut bool,
    accent: Color32,
    _ui_state: &SettingsUi,
) {
    match value {
        Value::Bool(b) => {
            let mut v = *b;
            setting_row(ui, &pretty_key(key), help_text(section, key), |ui| {
                if toggle_switch(ui, &mut v, accent, ui.id().with((section, key))).changed() {
                    set_section_key(state, section, key, json!(v), dirty);
                }
            });
        }
        Value::Number(n) => {
            let mut v = n.as_f64().unwrap_or(0.0) as f32;
            let range = if key.contains("scale") || key.contains("tau") || key.contains("frac") {
                0.0..=4.0
            } else if key.contains("opacity") || key.contains("pct") {
                0.0..=1.0
            } else {
                0.0..=500.0
            };
            if number_row(
                ui,
                &pretty_key(key),
                &mut v,
                range,
                0.05,
                accent,
                help_text(section, key),
            ) {
                set_section_key(state, section, key, json!(v as f64), dirty);
            }
        }
        Value::String(s) => {
            if looks_like_color(s) {
                paint_color_string(ui, state, section, key, s, dirty);
                return;
            }
            let mut text = s.clone();
            setting_row(ui, &pretty_key(key), help_text(section, key), |ui| {
                if text_field(ui, &mut text, "", 220.0).changed() {
                    set_section_key(state, section, key, json!(text), dirty);
                }
            });
        }
        Value::Object(map) if key == "colors" || key == "license_colors" => {
            accordion(ui, &pretty_key(key), accent, false, |ui| {
                let mut entries: Vec<_> = map.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                for (ck, cv) in entries {
                    if let Some(hex) = cv.as_str() {
                        paint_nested_color(ui, state, section, key, ck, hex, dirty);
                    }
                }
            });
        }
        Value::Object(_) | Value::Array(_) => {
            accordion(
                ui,
                &format!("{} (JSON)", pretty_key(key)),
                accent,
                false,
                |ui| {
                    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                    if resp.changed() {
                        if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                            set_section_key(state, section, key, parsed, dirty);
                        }
                    }
                },
            );
        }
        Value::Null => {}
    }
}

fn paint_footer(ui: &mut Ui, state: &StateHandle, ui_state: &mut SettingsUi, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let (running, edit_mode, apply_live, auto_save) = {
            let st = state.read();
            (
                st.running,
                st.edit_mode,
                st.settings_apply_live,
                st.settings_auto_save,
            )
        };
        let run_label = if running {
            "Stop overlay"
        } else {
            "Start overlay"
        };
        let run_kind = if running {
            ButtonKind::Stop
        } else {
            ButtonKind::Go
        };
        if button_kind(ui, run_label, run_kind).clicked() {
            if let Some(mut st) = state.try_write() {
                st.running = !running;
                ui_state.flash(if running {
                    "Overlay stopped"
                } else {
                    "Overlay started"
                });
            }
        }
        let mut edit = edit_mode;
        ui.label(RichText::new("Edit layout").color(theme::ROW_LABEL));
        if toggle_switch(ui, &mut edit, theme::ACCENT, ui.id().with("footer_edit")).changed() {
            if let Some(mut st) = state.try_write() {
                st.edit_mode = edit;
            }
        }

        // Stretch, then primary cluster + More overflow.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let more_id = ui.make_persistent_id("settings_footer_more");
                let more_resp = button_kind(ui, "More…", ButtonKind::GhostAccent);
                if more_resp.clicked() {
                    ui.memory_mut(|mem| mem.toggle_popup(more_id));
                }
                egui::popup::popup_below_widget(
                    ui,
                    more_id,
                    &more_resp,
                    egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        egui::Frame::new()
                            .fill(theme::POPUP_BG)
                            .stroke(egui::Stroke::new(1.0_f32, theme::INPUT_BORDER))
                            .corner_radius(theme::FIELD_RADIUS)
                            .inner_margin(egui::Margin::symmetric(10, 10))
                            .show(ui, |ui| {
                                ui.set_min_width(200.0);
                                ui.spacing_mut().item_spacing.y = 8.0;
                                let mut live = apply_live;
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Apply live").color(theme::ROW_LABEL));
                                    if toggle_switch(
                                        ui,
                                        &mut live,
                                        theme::ACCENT,
                                        ui.id().with("footer_apply_live"),
                                    )
                                    .changed()
                                    {
                                        if let Some(mut st) = state.try_write() {
                                            st.settings_apply_live = live;
                                        }
                                    }
                                });
                                let mut autosave = auto_save;
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Auto-save").color(theme::ROW_LABEL));
                                    if toggle_switch(
                                        ui,
                                        &mut autosave,
                                        theme::ACCENT,
                                        ui.id().with("footer_auto_save"),
                                    )
                                    .changed()
                                    {
                                        if let Some(mut st) = state.try_write() {
                                            st.settings_auto_save = autosave;
                                        }
                                    }
                                });
                                ui.separator();
                                if button_kind(ui, "Reload from file", ButtonKind::Default)
                                    .clicked()
                                {
                                    match crate::config::OverlayConfig::load() {
                                        Ok(next) => {
                                            if let Some(mut st) = state.try_write() {
                                                st.config = Arc::new(next);
                                                st.apply_effective_context();
                                                *dirty = false;
                                                ui_state.flash("Reloaded from file");
                                            }
                                        }
                                        Err(e) => ui_state.flash(e.to_string()),
                                    }
                                    ui.memory_mut(|mem| mem.close_popup());
                                }
                                if button_kind(ui, "Reset to defaults", ButtonKind::Danger)
                                    .clicked()
                                {
                                    if let Some(mut st) = state.try_write() {
                                        let context = st.effective_context();
                                        Arc::make_mut(&mut st.config).reset_context(context);
                                        *dirty = true;
                                        ui_state.flash(if context == ConfigContext::Garage {
                                            "Garage overrides cleared"
                                        } else {
                                            "Reset to defaults"
                                        });
                                    }
                                    ui.memory_mut(|mem| mem.close_popup());
                                }
                            });
                    },
                );
                if button_kind(ui, "Apply", ButtonKind::Warn).clicked() {
                    if let Some(mut st) = state.try_write() {
                        let context = st.effective_context();
                        Arc::make_mut(&mut st.config).sync_active_preset_for_context(context);
                        ui_state.flash("Applied");
                    }
                }
                if button_kind(ui, "Save", ButtonKind::Primary).clicked() {
                    if let Some(mut st) = state.try_write() {
                        let context = st.effective_context();
                        match Arc::make_mut(&mut st.config).save_for_context(context) {
                            Ok(()) => {
                                *dirty = false;
                                ui_state.flash("Saved");
                            }
                            Err(e) => ui_state.flash(e.to_string()),
                        }
                    }
                }
                if button_kind(ui, "Quit", ButtonKind::Danger).clicked() {
                    if let Some(mut st) = state.try_write() {
                        if st.settings_auto_save {
                            let context = st.effective_context();
                            let _ = Arc::make_mut(&mut st.config).save_for_context(context);
                        }
                        st.running = false;
                        st.settings_open = false;
                        st.quit_requested = true;
                    }
                }
            });
        });
    });
}

fn paint_color_string(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    hex: &str,
    dirty: &mut bool,
) {
    let col = parse_color_str(hex);
    let mut rgba = [
        col.r() as f32 / 255.0,
        col.g() as f32 / 255.0,
        col.b() as f32 / 255.0,
        col.a() as f32 / 255.0,
    ];
    setting_row(ui, &pretty_key(key), help_text(section, key), |ui| {
        if color_button(ui, (section, key), hex, &mut rgba) {
            let hex = rgba_to_hex(rgba);
            set_section_key(state, section, key, json!(hex), dirty);
        }
    });
}

fn paint_nested_color(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    group: &str,
    key: &str,
    hex: &str,
    dirty: &mut bool,
) {
    let col = parse_color_str(hex);
    let mut rgba = [
        col.r() as f32 / 255.0,
        col.g() as f32 / 255.0,
        col.b() as f32 / 255.0,
        col.a() as f32 / 255.0,
    ];
    setting_row(ui, &pretty_key(key), None, |ui| {
        if color_button(ui, (section, group, key), hex, &mut rgba) {
            let hex = rgba_to_hex(rgba);
            set_nested(state, section, group, key, json!(hex), dirty);
        }
    });
}

fn rgba_to_hex(rgba: [f32; 4]) -> String {
    let r = (rgba[0] * 255.0) as u8;
    let g = (rgba[1] * 255.0) as u8;
    let b = (rgba[2] * 255.0) as u8;
    let a = (rgba[3] * 255.0) as u8;
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

fn looks_like_color(s: &str) -> bool {
    s.starts_with('#') || s.starts_with("rgba(")
}

fn set_global(state: &StateHandle, key: &str, val: Value, dirty: &mut bool) {
    if let Some(mut st) = state.try_write() {
        let cfg = Arc::make_mut(&mut st.config);
        cfg.apply_cfg_patch(&json!({ key: val }));
        *dirty = true;
    }
}

fn set_section_key(state: &StateHandle, section: &str, key: &str, val: Value, dirty: &mut bool) {
    if let Some(mut st) = state.try_write() {
        let cfg = Arc::make_mut(&mut st.config);
        cfg.apply_cfg_patch(&json!({ section: { key: val } }));
        *dirty = true;
    }
}

fn set_nested(
    state: &StateHandle,
    section: &str,
    group: &str,
    key: &str,
    val: Value,
    dirty: &mut bool,
) {
    if let Some(mut st) = state.try_write() {
        let cfg = Arc::make_mut(&mut st.config);
        cfg.apply_cfg_patch(&json!({ section: { group: { key: val } } }));
        *dirty = true;
    }
}

pub fn default_size() -> egui::Vec2 {
    egui::vec2(880.0, 820.0)
}

pub fn window_title() -> &'static str {
    WINDOW_TITLE
}

pub fn apply_viewport_theme(ctx: &egui::Context) {
    theme::apply_settings_visuals(ctx);
}
