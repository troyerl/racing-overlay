//! In-overlay Settings UI (egui). Schema-driven pages matching Python ConfigEditor.

mod scan;
mod schema;
mod theme;
mod widgets;

pub use schema::{
    allows_free_text_setting, choice_label, default_table_col_width, help_text, is_skipped,
    matches_search, nav_for_tab, pretty_key, setting_groups, string_choices, tab_color,
    table_slot_options, top_tab_for, LAPLOG_COLUMNS, TABLE_DATA_COLUMNS, UNITS_CHOICES, TopTab,
};

use crate::config::{parse_color_str, ConfigContext};
use crate::state::{AlignH, AlignV, StateHandle};
use egui::{self, Color32, RichText, ScrollArea, Ui, UiBuilder, Vec2b};
use schema::group_default_open;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::{paint_background, MUTED, NAV_SECTION, NAV_WIDTH, TITLE};
use widgets::{
    accordion, button_kind, color_button, enable_card, enable_card_row, icon_button, nav_item,
    snap_preview_button,
    number_row, preset_button, search_field, setting_row, status_line, styled_choice_combo,
    styled_combo, text_field, toggle_switch, top_tab_button, top_tabs_frame, ButtonKind,
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
    /// Background cloud admin fetch in flight.
    pub admin_loading: bool,
    pub update_url: Option<String>,
    pub dg_sel: Option<String>,
    pub dg_name: String,
    pub dg_icon: String,
    pub dg_color: String,
    /// Comma-separated member names (source of truth for the open group).
    pub dg_members: String,
    /// Single-name field: filled by selecting a member, or typed to add/rename.
    pub dg_member_edit: String,
    /// Member currently selected in the list (display name).
    pub dg_member_sel: Option<String>,
    /// True while composing a new group (do not auto-select an existing one).
    pub dg_new: bool,
    /// Reveal full LAN ipc_token in App settings (off by default).
    pub show_lan_token: bool,
    /// Cached widget section values (invalidated on edit / section change).
    pub section_cache_id: String,
    pub section_cache: Option<std::sync::Arc<std::collections::HashMap<String, Value>>>,
}

impl SettingsUi {
    pub fn flash(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_deadline = Some(Instant::now() + Duration::from_millis(2500));
    }

    pub fn invalidate_section_cache(&mut self) {
        self.section_cache = None;
        self.section_cache_id.clear();
    }

    pub fn tick_status(&mut self) {
        if let Some(deadline) = self.status_deadline {
            if Instant::now() > deadline {
                self.status.clear();
                self.status_deadline = None;
            }
        }
    }

    /// 1 -> 0 over the last 400 ms of the status flash.
    pub fn status_opacity(&self) -> f32 {
        match self.status_deadline {
            Some(deadline) if !self.status.is_empty() => {
                let left = deadline.saturating_duration_since(Instant::now());
                let ms = left.as_millis() as f32;
                if ms >= 400.0 {
                    1.0
                } else {
                    (ms / 400.0).clamp(0.0, 1.0)
                }
            }
            _ => {
                if self.status.is_empty() {
                    0.0
                } else {
                    1.0
                }
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
    scan::poll_cloud_jobs(ui_state);
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

                status_line(ui, &ui_state.status, ui_state.status_opacity());
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
                        ui_state.flash(format!("Switched to \"{name}\""));
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
                        ui_state.flash(format!("Created \"{name}\""));
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
                        ui_state.flash(format!("Duplicated as \"{to}\""));
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
    accordion(
        ui,
        "preset_advanced",
        "Advanced",
        theme::MUTED,
        false,
        |ui| {
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
                                ui_state.flash(format!("Renamed to \"{to}\""));
                            }
                            Err(e) => ui_state.flash(e.to_string()),
                        }
                    }
                }
            });
            if *dirty {
                ui.label(RichText::new("unsaved").color(theme::YELLOW));
            }
        },
    );
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
                let live = if st.frame.connected && (st.frame.in_garage || !st.frame.in_car) {
                    ConfigContext::Garage
                } else {
                    ConfigContext::Race
                };
                if context == live {
                    // Selecting the live sim profile clears the pin and follows telemetry.
                    st.set_preview_context(None);
                    if st.config_context != live {
                        st.set_config_context(live);
                    } else {
                        st.apply_effective_context();
                    }
                    ui_state.flash(format!("Following {}", live.label()));
                } else {
                    st.set_preview_context(Some(context));
                    ui_state.flash(format!("Editing {}", context.label()));
                }
            }
        }
    });
    ui.label(
        RichText::new(
            "On track = seated in-car. In garage = garage, menus, or spectating. Live preview follows telemetry.",
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
    if matches!(
        key,
        "__general__" | "__app__" | "__drivers__" | "__lan__" | "__scan__" | "__widgets__"
    ) {
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
        "__drivers__" => paint_drivers(ui, state, ui_state, dirty, accent),
        "__lan__" => paint_lan(ui, state, ui_state, dirty, accent),
        "__scan__" => scan::paint_track_scan(ui, state, ui_state, accent),
        other => paint_widget_section(ui, state, ui_state, other, dirty, accent),
    }
}

fn paint_general(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
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
            if let Some(next) = styled_choice_combo(ui, "units", &units, UNITS_CHOICES, 150.0) {
                units = next;
                set_global(state, "units", json!(units), dirty, ui_state);
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
            set_global(
                state,
                "text_scale",
                json!(text_scale as f64),
                dirty,
                ui_state,
            );
        }
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
                        ui_state,
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
                    set_global(state, "start_at_login", json!(start_login), dirty, ui_state);
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
                    set_global(
                        state,
                        "check_updates_on_launch",
                        json!(check_upd),
                        dirty,
                        ui_state,
                    );
                }
            },
        );
        let mut close_to_tray = state
            .read()
            .config
            .cfg
            .get("close_settings_to_tray")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        setting_row(
            ui,
            "Close settings to tray",
            help_text("__app__", "close_settings_to_tray"),
            |ui| {
                if toggle_switch(
                    ui,
                    &mut close_to_tray,
                    accent,
                    ui.id().with("app_close_to_tray"),
                )
                .changed()
                {
                    set_global(
                        state,
                        "close_settings_to_tray",
                        json!(close_to_tray),
                        dirty,
                        ui_state,
                    );
                }
            },
        );
    });
}

fn paint_lan(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    dirty: &mut bool,
    accent: Color32,
) {
    ui.label(
        RichText::new("LAN telemetry")
            .size(16.0)
            .strong()
            .color(TITLE),
    );
    ui.label(
        RichText::new("Read-only API for other apps on your Wi‑Fi/LAN.")
            .size(11.0)
            .color(MUTED),
    );
    ui.add_space(8.0);
    enable_card(ui, "LAN telemetry API", accent, |ui| {
        ui.label(
            RichText::new(
                "Listens on all interfaces when enabled. Clients must send your ipc_token.",
            )
            .size(11.0)
            .color(MUTED),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "Token file: {}",
                crate::paths::ipc_token_path().display()
            ))
            .size(10.0)
            .color(MUTED),
        );
        ui.add_space(6.0);
        let mut port = state
            .read()
            .config
            .cfg
            .get("lan_telemetry_port")
            .and_then(|v| v.as_f64())
            .unwrap_or(f64::from(gridglance_ipc::DEFAULT_LAN_TELEMETRY_PORT))
            as f32;
        let connect_endpoint = crate::telemetry_lan::lan_connect_endpoint(port.round() as u16);
        let lan_ip = crate::telemetry_lan::preferred_lan_ipv4();
        setting_row(
            ui,
            "Connect address",
            Some(
                "Host:port for other devices on the same Wi‑Fi. Use this IP — not 127.0.0.1 — from a phone or second PC.",
            ),
            |ui| {
                ui.label(
                    RichText::new(&connect_endpoint)
                        .size(13.0)
                        .monospace()
                        .color(TITLE),
                );
                ui.add_space(6.0);
                if button_kind(ui, "Copy address", ButtonKind::Primary).clicked() {
                    if lan_ip.is_none() {
                        ui_state.flash("Could not detect LAN IP — check Wi‑Fi or run ipconfig");
                    } else {
                        ui.ctx().copy_text(connect_endpoint.clone());
                        ui_state.flash(format!("Copied {connect_endpoint}"));
                    }
                }
            },
        );
        ui.add_space(4.0);
        let lan_token = crate::ipc::ensure_ipc_token().unwrap_or_default();
        setting_row(
            ui,
            "IPC token",
            Some("Paste this token into the client on your phone or other PC. Same secret as localhost control IPC."),
            |ui| {
                let display = if ui_state.show_lan_token {
                    lan_token.clone()
                } else if lan_token.len() <= 8 {
                    "••••••••".into()
                } else {
                    format!("{}…{}", &lan_token[..4], &lan_token[lan_token.len() - 4..])
                };
                ui.label(RichText::new(display).size(12.0).monospace().color(TITLE));
                ui.add_space(6.0);
                if button_kind(
                    ui,
                    if ui_state.show_lan_token {
                        "Hide"
                    } else {
                        "Show"
                    },
                    ButtonKind::GhostAccent,
                )
                .clicked()
                {
                    ui_state.show_lan_token = !ui_state.show_lan_token;
                }
                if button_kind(ui, "Copy token", ButtonKind::Primary).clicked() {
                    if lan_token.is_empty() {
                        ui_state.flash("Could not read ipc_token");
                    } else {
                        ui.ctx().copy_text(lan_token.clone());
                        ui_state.flash("Token copied — paste it into your other device’s app");
                    }
                }
            },
        );
        ui.add_space(4.0);
        let mut lan_on = state
            .read()
            .config
            .cfg
            .get("lan_telemetry_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        setting_row(
            ui,
            "Enable LAN telemetry",
            help_text("__lan__", "lan_telemetry_enabled"),
            |ui| {
                if toggle_switch(ui, &mut lan_on, accent, ui.id().with("lan_telem_on")).changed() {
                    set_global(
                        state,
                        "lan_telemetry_enabled",
                        json!(lan_on),
                        dirty,
                        ui_state,
                    );
                }
            },
        );
        if number_row(
            ui,
            "Port",
            &mut port,
            1024.0..=65535.0,
            1.0,
            accent,
            help_text("__lan__", "lan_telemetry_port"),
        ) {
            set_global(
                state,
                "lan_telemetry_port",
                json!(port.round() as u64),
                dirty,
                ui_state,
            );
        }
        let mut hz = state
            .read()
            .config
            .cfg
            .get("lan_telemetry_hz")
            .and_then(|v| v.as_f64())
            .unwrap_or(15.0) as f32;
        if number_row(
            ui,
            "Push rate (Hz)",
            &mut hz,
            5.0..=30.0,
            1.0,
            accent,
            help_text("__lan__", "lan_telemetry_hz"),
        ) {
            set_global(
                state,
                "lan_telemetry_hz",
                json!(hz.round() as u64),
                dirty,
                ui_state,
            );
        }
        ui.add_space(8.0);
        let mut upload_laps = state
            .read()
            .config
            .cfg
            .get("upload_race_laps")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        setting_row(
            ui,
            "Upload race laps after finish",
            help_text("__lan__", "upload_race_laps"),
            |ui| {
                if toggle_switch(ui, &mut upload_laps, accent, ui.id().with("upload_race_laps"))
                    .changed()
                {
                    set_global(
                        state,
                        "upload_race_laps",
                        json!(upload_laps),
                        dirty,
                        ui_state,
                    );
                }
            },
        );
    });
}

fn paint_drivers(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    dirty: &mut bool,
    accent: Color32,
) {
    ui.label(RichText::new("Drivers").size(16.0).strong().color(TITLE));
    ui.label(
        RichText::new("App-wide groups and professional-driver badges.")
            .size(11.0)
            .color(MUTED),
    );
    ui.add_space(8.0);
    scan::ensure_admin_loaded(ui_state);
    paint_driver_groups(ui, state, ui_state, dirty, accent);
    scan::paint_pro_drivers_admin(ui, ui_state, accent);
}

fn paint_driver_groups(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    dirty: &mut bool,
    accent: Color32,
) {
    enable_card(ui, "Driver groups", accent, |ui| {
        ui.label(
            RichText::new(
                "App-wide (all presets, On track and In garage). Icons show on Relative, Standings, and Radio.",
            )
            .size(11.0)
            .color(MUTED),
        );

        let mut groups = {
            let st = state.read();
            crate::driver_groups::normalize_driver_groups(&st.config.driver_groups_value())
        };

        let names: Vec<String> = groups
            .iter()
            .filter_map(|g| {
                g.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        let selected = if ui_state.dg_new {
            String::new()
        } else {
            ui_state
                .dg_sel
                .clone()
                .filter(|s| names.contains(s))
                .or_else(|| names.first().cloned())
                .unwrap_or_default()
        };

        // Keep the editor filled whenever the selection changes (including first paint).
        if !ui_state.dg_new
            && !selected.is_empty()
            && (ui_state.dg_sel.as_deref() != Some(selected.as_str())
                || ui_state.dg_name.trim().is_empty())
        {
            load_driver_group_into_ui(ui_state, &groups, &selected);
        }

        if names.is_empty() {
            ui.label(
                RichText::new("No groups yet — add a name and members below.")
                    .size(11.0)
                    .color(MUTED),
            );
        } else {
            let combo_sel = if selected.is_empty() {
                names.first().cloned().unwrap_or_default()
            } else {
                selected.clone()
            };
            setting_row(ui, "Group", None, |ui| {
                if let Some(next) = styled_combo(ui, "dg_list", &combo_sel, &names, 220.0) {
                    load_driver_group_into_ui(ui_state, &groups, &next);
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("{} group(s)", names.len()))
                    .size(11.0)
                    .color(MUTED),
            );
            egui::ScrollArea::vertical()
                .id_salt("dg_group_list")
                .max_height(120.0)
                .show(ui, |ui| {
                    for g in &groups {
                        let name = g
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        if name.is_empty() {
                            continue;
                        }
                        let n_members = g
                            .get("members")
                            .and_then(|m| m.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        let is_sel = ui_state.dg_sel.as_deref() == Some(name.as_str());
                        let label = format!("{name}  ·  {n_members} drivers");
                        let resp = ui.selectable_label(is_sel, RichText::new(label).size(12.0));
                        if resp.clicked() {
                            load_driver_group_into_ui(ui_state, &groups, &name);
                        }
                    }
                });
        }

        ui.add_space(8.0);
        setting_row(ui, "Group name", None, |ui| {
            let _ = text_field(ui, &mut ui_state.dg_name, "League mates", 180.0);
        });
        setting_row(ui, "Icon", None, |ui| {
            if ui_state.dg_icon.is_empty()
                || !crate::driver_groups::DRIVER_GROUP_ICONS.contains(&ui_state.dg_icon.as_str())
            {
                ui_state.dg_icon = "league".into();
            }
            if ui_state.dg_color.trim().is_empty() {
                ui_state.dg_color = "#5bb8ff".into();
            }
            let icon_col = parse_color_str(&ui_state.dg_color);
            if let Some(next) =
                widgets::icon_combo(ui, "dg_icon", &ui_state.dg_icon, 180.0, icon_col)
            {
                ui_state.dg_icon = next;
                if !ui_state.dg_new {
                    let _ = persist_driver_group_members(state, ui_state, &mut groups, dirty);
                }
            }
        });
        setting_row(ui, "Color", None, |ui| {
            if ui_state.dg_color.trim().is_empty() {
                ui_state.dg_color = "#5bb8ff".into();
            }
            let col = parse_color_str(&ui_state.dg_color);
            let mut rgba = [
                col.r() as f32 / 255.0,
                col.g() as f32 / 255.0,
                col.b() as f32 / 255.0,
                col.a() as f32 / 255.0,
            ];
            if color_button(ui, "dg_color", &ui_state.dg_color, &mut rgba) {
                ui_state.dg_color = rgba_to_hex(rgba);
                if !ui_state.dg_new {
                    let _ = persist_driver_group_members(state, ui_state, &mut groups, dirty);
                }
            }
        });

        let member_names: Vec<String> = crate::driver_groups::members_from_csv(&ui_state.dg_members)
            .iter()
            .filter_map(|e| e.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
            .collect();
        if ui_state
            .dg_member_sel
            .as_ref()
            .is_some_and(|s| !member_names.iter().any(|n| n == s))
        {
            ui_state.dg_member_sel = None;
        }
        ui.label(
            RichText::new(format!("Members ({})", member_names.len()))
                .size(12.0)
                .strong()
                .color(TITLE),
        );
        if member_names.is_empty() {
            ui.label(
                RichText::new("No drivers in this group yet.")
                    .size(11.0)
                    .color(MUTED),
            );
        } else {
            egui::ScrollArea::vertical()
                .id_salt("dg_member_list")
                .max_height(160.0)
                .show(ui, |ui| {
                    for (i, name) in member_names.iter().enumerate() {
                        let is_sel = ui_state.dg_member_sel.as_deref() == Some(name.as_str());
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(is_sel, RichText::new(name).size(12.0))
                                .clicked()
                            {
                                ui_state.dg_member_sel = Some(name.clone());
                                ui_state.dg_member_edit = name.clone();
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if button_kind(ui, "Remove", ButtonKind::GhostAccent).clicked()
                                    {
                                        let mut next = member_names.clone();
                                        next.remove(i);
                                        if ui_state.dg_member_sel.as_deref() == Some(name.as_str())
                                        {
                                            ui_state.dg_member_sel = None;
                                            ui_state.dg_member_edit.clear();
                                        }
                                        ui_state.dg_members = next.join(", ");
                                        match persist_driver_group_members(
                                            state,
                                            ui_state,
                                            &mut groups,
                                            dirty,
                                        ) {
                                            Ok(()) => ui_state.flash("Member removed"),
                                            Err(msg) => ui_state.flash(msg),
                                        }
                                    }
                                },
                            );
                        });
                    }
                });
        }
        setting_row(
            ui,
            "Edit member",
            Some("Click a member above to load their name, then rename or add."),
            |ui| {
                let _ = text_field(ui, &mut ui_state.dg_member_edit, "Driver name", 220.0);
            },
        );
        ui.horizontal(|ui| {
            if button_kind(ui, "Add member", ButtonKind::GhostAccent).clicked() {
                let name = ui_state.dg_member_edit.trim().to_string();
                if !name.is_empty() {
                    let mut next = member_names.clone();
                    let exists = next
                        .iter()
                        .any(|n| n.eq_ignore_ascii_case(name.as_str()));
                    if !exists {
                        next.push(name.clone());
                        ui_state.dg_members = next.join(", ");
                        ui_state.dg_member_sel = Some(name);
                        match persist_driver_group_members(
                            state,
                            ui_state,
                            &mut groups,
                            dirty,
                        ) {
                            Ok(()) => ui_state.flash("Member added"),
                            Err(msg) => ui_state.flash(msg),
                        }
                    } else {
                        ui_state.flash("Already in this group");
                    }
                }
            }
            if button_kind(ui, "Rename selected", ButtonKind::Default).clicked() {
                let new_name = ui_state.dg_member_edit.trim().to_string();
                if let Some(old) = ui_state.dg_member_sel.clone() {
                    if !new_name.is_empty() {
                        let mut next = member_names.clone();
                        if let Some(pos) = next.iter().position(|n| n == &old) {
                            let clash = next.iter().enumerate().any(|(i, n)| {
                                i != pos && n.eq_ignore_ascii_case(new_name.as_str())
                            });
                            if clash {
                                ui_state.flash("Another member already has that name");
                            } else {
                                next[pos] = new_name.clone();
                                ui_state.dg_members = next.join(", ");
                                ui_state.dg_member_sel = Some(new_name.clone());
                                ui_state.dg_member_edit = new_name;
                                match persist_driver_group_members(
                                    state,
                                    ui_state,
                                    &mut groups,
                                    dirty,
                                ) {
                                    Ok(()) => ui_state.flash("Member renamed"),
                                    Err(msg) => ui_state.flash(msg),
                                }
                            }
                        }
                    }
                } else {
                    ui_state.flash("Select a member first");
                }
            }
        });

        ui.horizontal(|ui| {
            if button_kind(ui, "Add / Update", ButtonKind::GhostAccent).clicked() {
                let name = ui_state.dg_name.trim().to_string();
                if !name.is_empty() {
                    let members = crate::driver_groups::members_from_csv(&ui_state.dg_members);
                    ui_state.dg_members = crate::driver_groups::members_to_csv(&members);
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
                    ui_state.dg_new = false;
                    set_driver_groups(state, json!(groups), dirty, ui_state);
                    ui_state.flash("Driver group saved");
                }
            }
            if button_kind(ui, "Remove group", ButtonKind::Warn).clicked() {
                if let Some(sel) = ui_state.dg_sel.clone() {
                    groups.retain(|g| g.get("name").and_then(|n| n.as_str()) != Some(sel.as_str()));
                    set_driver_groups(state, json!(groups), dirty, ui_state);
                    ui_state.dg_sel = None;
                    ui_state.dg_new = false;
                    ui_state.dg_name.clear();
                    ui_state.dg_members.clear();
                    ui_state.dg_member_edit.clear();
                    ui_state.dg_member_sel = None;
                    ui_state.flash("Driver group removed");
                }
            }
            if button_kind(ui, "New group", ButtonKind::Default).clicked() {
                ui_state.dg_sel = None;
                ui_state.dg_new = true;
                ui_state.dg_name.clear();
                ui_state.dg_icon = "league".into();
                ui_state.dg_color = "#5bb8ff".into();
                ui_state.dg_members.clear();
                ui_state.dg_member_edit.clear();
                ui_state.dg_member_sel = None;
            }
            if button_kind(ui, "Import from results...", ButtonKind::Default).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file()
                {
                    match std::fs::read_to_string(&path)
                        .map_err(|e| e.into())
                        .and_then(|t| crate::driver_groups::parse_event_result_names(&t))
                    {
                        Ok(names) => {
                            // Prefer saved group members so a stale/empty UI field
                            // cannot re-import drivers that are already in the group.
                            let mut existing = crate::driver_groups::members_from_csv(
                                &ui_state.dg_members,
                            );
                            if let Some(sel) = ui_state.dg_sel.as_deref() {
                                if let Some(g) = groups.iter().find(|g| {
                                    g.get("name").and_then(|n| n.as_str()) == Some(sel)
                                }) {
                                    if let Some(mem) = g.get("members").and_then(|m| m.as_array())
                                    {
                                        let (merged_existing, _, _) =
                                            crate::driver_groups::merge_names_into_members(
                                                &existing,
                                                &mem.iter()
                                                    .filter_map(|e| {
                                                        e.get("name")
                                                            .and_then(|n| n.as_str())
                                                            .map(|s| s.to_string())
                                                    })
                                                    .collect::<Vec<_>>(),
                                            );
                                        existing = merged_existing;
                                    }
                                }
                            }
                            let (merged, added, skipped) =
                                crate::driver_groups::merge_names_into_members(&existing, &names);
                            ui_state.dg_members =
                                crate::driver_groups::members_to_csv(&merged);
                            // Persist immediately when editing a named group.
                            let gname = ui_state.dg_name.trim().to_string();
                            if !gname.is_empty() {
                                let entry = json!({
                                    "name": gname,
                                    "icon": ui_state.dg_icon,
                                    "color": ui_state.dg_color,
                                    "members": merged,
                                });
                                if let Some(pos) = groups.iter().position(|g| {
                                    g.get("name").and_then(|n| n.as_str()) == Some(gname.as_str())
                                }) {
                                    groups[pos] = entry;
                                } else {
                                    groups.push(entry);
                                    ui_state.dg_sel = Some(gname.clone());
                                    ui_state.dg_new = false;
                                }
                                set_driver_groups(state, json!(groups), dirty, ui_state);
                            }
                            ui_state.flash(format!(
                                "Imported +{added}, skipped {skipped} duplicate{}",
                                if skipped == 1 { "" } else { "s" }
                            ));
                        }
                        Err(e) => ui_state.flash(e.to_string()),
                    }
                }
            }
        });
    });
}

/// Write the editor's member list into the selected/named group and persist.
fn persist_driver_group_members(
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    groups: &mut Vec<Value>,
    dirty: &mut bool,
) -> Result<(), String> {
    let gname = ui_state
        .dg_sel
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ui_state.dg_name.trim());
    if gname.is_empty() {
        return Err("Enter a group name first".into());
    }
    let gname = gname.to_string();
    if ui_state.dg_name.trim().is_empty() {
        ui_state.dg_name = gname.clone();
    }
    let members = crate::driver_groups::members_from_csv(&ui_state.dg_members);
    ui_state.dg_members = crate::driver_groups::members_to_csv(&members);
    let entry = json!({
        "name": gname,
        "icon": ui_state.dg_icon,
        "color": ui_state.dg_color,
        "members": members,
    });
    if let Some(pos) = groups
        .iter()
        .position(|g| g.get("name").and_then(|n| n.as_str()) == Some(gname.as_str()))
    {
        groups[pos] = entry;
    } else {
        groups.push(entry);
    }
    ui_state.dg_sel = Some(gname);
    ui_state.dg_new = false;
    set_driver_groups(state, json!(groups.clone()), dirty, ui_state);
    Ok(())
}

fn load_driver_group_into_ui(ui_state: &mut SettingsUi, groups: &[Value], name: &str) {
    let Some(g) = groups
        .iter()
        .find(|g| g.get("name").and_then(|n| n.as_str()) == Some(name))
    else {
        return;
    };
    ui_state.dg_new = false;
    ui_state.dg_sel = Some(name.to_string());
    ui_state.dg_name = name.to_string();
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
    ui_state.dg_member_edit.clear();
    ui_state.dg_member_sel = None;
}

fn paint_widget_position(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    section: &str,
    accent: Color32,
) {
    enable_card(ui, "Position on screen", accent, |ui| {
        ui.label(
            RichText::new("Snap to the monitor this widget is on. Size stays the same.")
                .size(11.0)
                .color(MUTED),
        );
        ui.add_space(8.0);
        let gap = 4.0;
        let cell = egui::vec2(36.0, 36.0);
        let grid: [[(&str, Option<usize>, Option<usize>, AlignH, AlignV); 3]; 3] = [
            [
                ("Top left", Some(0), Some(0), AlignH::Left, AlignV::Top),
                ("Top", Some(1), Some(0), AlignH::Center, AlignV::Top),
                ("Top right", Some(2), Some(0), AlignH::Right, AlignV::Top),
            ],
            [
                ("Left", Some(0), Some(1), AlignH::Left, AlignV::Center),
                ("Center", Some(1), Some(1), AlignH::Center, AlignV::Center),
                ("Right", Some(2), Some(1), AlignH::Right, AlignV::Center),
            ],
            [
                ("Bottom left", Some(0), Some(2), AlignH::Left, AlignV::Bottom),
                ("Bottom", Some(1), Some(2), AlignH::Center, AlignV::Bottom),
                ("Bottom right", Some(2), Some(2), AlignH::Right, AlignV::Bottom),
            ],
        ];
        for (ri, row) in grid.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (ci, (tip, col, row, h, v)) in row.iter().enumerate() {
                    let id = ui.id().with((section, "pos", ri, ci));
                    if snap_preview_button(ui, *col, *row, cell, id)
                        .on_hover_text(*tip)
                        .clicked()
                    {
                        snap_widget(state, ui_state, section, *h, *v);
                    }
                }
            });
            ui.add_space(4.0);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            if snap_preview_button(
                ui,
                None,
                Some(1),
                cell,
                ui.id().with((section, "pos_h")),
            )
            .on_hover_text("Center horizontally (keep vertical)")
            .clicked()
            {
                snap_widget(state, ui_state, section, AlignH::Center, AlignV::Keep);
            }
            if snap_preview_button(
                ui,
                Some(1),
                None,
                cell,
                ui.id().with((section, "pos_v")),
            )
            .on_hover_text("Center vertically (keep horizontal)")
            .clicked()
            {
                snap_widget(state, ui_state, section, AlignH::Keep, AlignV::Center);
            }
        });
    });
}

fn snap_widget(
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    section: &str,
    horiz: AlignH,
    vert: AlignV,
) {
    if let Some(mut st) = state.try_write() {
        st.align_widget(section, horiz, vert);
    }
    ui_state.flash("Widget moved");
}

fn paint_widget_section(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
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
                    ui_state.invalidate_section_cache();
                }
            }
        });
    });
    ui.add_space(4.0);

    let values = cached_section_values(state, section, ui_state);
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
        set_section_key(state, section, "show", json!(show), dirty, ui_state);
    }
    ui.add_space(8.0);
    paint_widget_position(ui, state, ui_state, section, accent);
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
        for (k, v) in values.iter() {
            grouped.insert(k.as_str(), v);
        }
        // Every key claimed by a schema group — never also listed under Other.
        let schema_keys: std::collections::HashSet<&str> = groups
            .iter()
            .flat_map(|(_, keys)| keys.iter().copied())
            .collect();
        for (group_title, keys) in &groups {
            let visible_keys: Vec<&str> = keys
                .iter()
                .copied()
                .filter(|k| {
                    *k != "show"
                        && !is_skipped(section, k)
                        && matches_search(section, k, &ui_state.search)
                })
                .collect();
            if visible_keys.is_empty() {
                continue;
            }
            accordion(
                ui,
                (section, "group", *group_title),
                group_title,
                accent,
                group_default_open(group_title),
                |ui| {
                    for key in visible_keys {
                        if let Some(val) = grouped.get(key) {
                            paint_value(ui, state, section, key, val, dirty, accent, ui_state);
                        } else {
                            // Schema key missing from sparse preset — still show a control.
                            let fallback = schema_fallback_value(section, key);
                            paint_value(
                                ui, state, section, key, &fallback, dirty, accent, ui_state,
                            );
                        }
                    }
                },
            );
        }
        // Other = true orphans only (legacy / unknown keys not in any group).
        let mut leftovers: Vec<_> = values
            .iter()
            .filter(|(k, _)| {
                let key = k.as_str();
                key != "show"
                    && !is_skipped(section, key)
                    && !schema_keys.contains(key)
                    && matches_search(section, key, &ui_state.search)
            })
            .collect();
        leftovers.sort_by(|a, b| a.0.cmp(b.0));
        if !leftovers.is_empty() {
            accordion(ui, (section, "other"), "Other", accent, false, |ui| {
                ui.label(
                    RichText::new(
                        "Legacy keys from an older preset. Safe to ignore or clear by resetting this widget.",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
                ui.add_space(4.0);
                for (key, val) in leftovers {
                    paint_value(ui, state, section, key, val, dirty, accent, ui_state);
                }
            });
        }
    } else {
        let mut keys: Vec<_> = values.keys().collect();
        keys.sort();
        for key in keys {
            if key == "show" || is_skipped(section, key) {
                continue;
            }
            if !matches_search(section, key, &ui_state.search) {
                continue;
            }
            if let Some(value) = values.get(key) {
                paint_value(ui, state, section, key, value, dirty, accent, ui_state);
            }
        }
    }
}

fn cached_section_values(
    state: &StateHandle,
    section: &str,
    ui_state: &mut SettingsUi,
) -> std::sync::Arc<std::collections::HashMap<String, Value>> {
    let (gen, ctx) = {
        let st = state.read();
        (st.config.generation, st.effective_context())
    };
    let cache_id = format!("{section}|{ctx:?}|{gen}");
    if ui_state.section_cache_id != cache_id || ui_state.section_cache.is_none() {
        ui_state.section_cache_id = cache_id;
        ui_state.section_cache = Some(std::sync::Arc::new(section_values(state, section)));
    }
    std::sync::Arc::clone(
        ui_state
            .section_cache
            .as_ref()
            .expect("section cache filled"),
    )
}

fn section_values(state: &StateHandle, section: &str) -> HashMap<String, Value> {
    let st = state.read();
    match st.config.section(section) {
        Value::Object(map) => map.clone().into_iter().collect(),
        _ => HashMap::new(),
    }
}

/// Default control value when a schema key is absent from the live section map.
fn schema_fallback_value(section: &str, key: &str) -> Value {
    match key {
        "show_icons" | "show_subtask_music" => Value::Bool(false),
        "title" => Value::String(section.to_uppercase().replace('_', " ")),
        "panel_style" => Value::String("data".into()),
        "row_height_px" => json!(28.0),
        "corner_radius_frac" => json!(0.0),
        "panel_opacity" => json!(1.0),
        "widths" => json!({}),
        "column_order" if matches!(section, "relative" | "standings") => {
            json!(["badge", "position", "name", "license", "irating", "gap"])
        }
        k if k.starts_with("show_") => Value::Bool(true),
        _ => Value::Bool(true),
    }
}

/// Slider/spin bounds for a numeric setting key (min..=max, step).
fn number_setting_bounds(key: &str) -> (std::ops::RangeInclusive<f32>, f32) {
    match key {
        "rows" | "rows_ahead" | "rows_behind" => return (0.0..=40.0, 1.0),
        "max_turns" => return (1.0..=8.0, 1.0),
        "sectors" => return (2.0..=12.0, 1.0),
        "history_laps" | "green_history_laps" => return (1.0..=50.0, 1.0),
        "fuel_ema_alpha" | "caution_pit_loss_factor" | "caution_fuel_multiplier" => {
            return (0.05..=1.0, 0.05);
        }
        "fuel_mass_laptime_s_per_l" => return (0.0..=0.1, 0.005),
        "fuel_fill_rate_lps" => return (0.5..=5.0, 0.1),
        "tire_change_4t_s" | "tire_change_2t_s" | "opponent_splash_pit_max_s" => {
            return (0.0..=60.0, 0.5);
        }
        "pace_window_laps" | "fcy_horizon_laps" | "opponent_stint_due_laps" => {
            return (1.0..=40.0, 1.0);
        }
        "race_tire_sets_total" | "tire_sets_reserve" => return (0.0..=10.0, 1.0),
        "track_wetness_tire_suppress" => return (0.0..=100.0, 1.0),
        "legal_fuel_buffer_l" | "min_stint_laps" => return (0.0..=20.0, 0.5),
        "shift_segments" | "ring_segments" => return (4.0..=48.0, 1.0),
        "row_height_px" => return (0.0..=80.0, 1.0),
        "asphalt_width" => return (1.0..=40.0, 1.0),
        "outline_width" => return (0.0..=24.0, 1.0),
        "rotation" => return (0.0..=360.0, 90.0),
        "brake_threshold" => return (0.0..=100.0, 1.0),
        "warn_wear_pct" => return (0.0..=100.0, 1.0),
        "line_width" => return (0.5..=8.0, 0.1),
        "delta_bar_range" | "range" => return (0.1..=5.0, 0.05),
        "pit_loss_seconds" => return (0.0..=90.0, 1.0),
        "low_fuel_time_threshold" => return (0.0..=600.0, 5.0),
        "low_fuel_laps_threshold" => return (0.0..=20.0, 0.5),
        "shift_blink_hz" => return (1.0..=20.0, 0.5),
        "history_seconds" | "marker_hold_seconds" | "shift_blink_max_sec" => {
            return (0.0..=30.0, 0.5);
        }
        "undercut_gap_max_s" | "cover_gap_max_s" => return (0.0..=60.0, 0.5),
        "range_pct" => return (0.0..=0.15, 0.005),
        "alongside_zone_pct" | "side_span_pct" => return (0.0..=0.02, 0.0005),
        "dot_radius_frac" | "other_dot_radius_frac" => return (0.01..=0.15, 0.005),
        "lan_telemetry_port" => return (1024.0..=65535.0, 1.0),
        "lan_telemetry_hz" => return (5.0..=30.0, 1.0),
        _ => {}
    }

    if key.contains("tau") {
        // 0 disables easing in some widgets — keep a usable floor.
        return (0.05..=1.0, 0.01);
    }
    if key.ends_with("_frac") || key.contains("opacity") {
        return (0.0..=1.0, 0.01);
    }
    // Fraction-style pct (0–1). Integer-style wear pct handled above.
    if key.ends_with("_pct") || key.contains("pct") {
        return (0.0..=1.0, 0.01);
    }
    if key.contains("scale") {
        return (0.25..=2.5, 0.05);
    }
    if key.ends_with("_seconds") || key.ends_with("_sec") || key.ends_with("_s") {
        return (0.0..=60.0, 0.5);
    }
    if key.ends_with("_px") || key.ends_with("_width") {
        return (0.0..=80.0, 1.0);
    }
    // Unknown numeric — keep modest so the slider stays usable.
    (0.0..=50.0, 0.05)
}

fn paint_value(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    value: &Value,
    dirty: &mut bool,
    accent: Color32,
    ui_state: &mut SettingsUi,
) {
    if key == "column_order" {
        if matches!(section, "relative" | "standings") {
            paint_ordered_columns(
                ui,
                state,
                section,
                key,
                TABLE_DATA_COLUMNS,
                &["name"],
                dirty,
                accent,
                ui_state,
            );
            return;
        }
        if section == "laptime_log" {
            paint_ordered_columns(
                ui,
                state,
                section,
                key,
                LAPLOG_COLUMNS,
                &["lap", "time"],
                dirty,
                accent,
                ui_state,
            );
            return;
        }
    }
    if key == "widths" && matches!(section, "relative" | "standings") {
        paint_table_widths(ui, state, section, dirty, accent, ui_state);
        return;
    }
    if key == "columns" && matches!(section, "relative" | "standings") {
        paint_table_column_flags(ui, state, section, value, dirty, accent, ui_state);
        return;
    }
    if key == "sizes" && section == "radar" {
        paint_numeric_object(ui, state, section, key, value, dirty, accent, ui_state);
        return;
    }
    match value {
        Value::Bool(b) => {
            let mut v = *b;
            setting_row(ui, &pretty_key(key), help_text(section, key), |ui| {
                if toggle_switch(ui, &mut v, accent, ui.id().with((section, key))).changed() {
                    set_section_key(state, section, key, json!(v), dirty, ui_state);
                }
            });
        }
        Value::Number(n) => {
            if matches!(key, "rows" | "rows_ahead" | "rows_behind")
                && matches!(section, "relative" | "standings")
            {
                paint_linked_row_counts(ui, state, section, key, dirty, accent, ui_state);
                return;
            }
            let mut v = n.as_f64().unwrap_or(0.0) as f32;
            let (range, step) = number_setting_bounds(key);
            v = v.clamp(*range.start(), *range.end());
            if number_row(
                ui,
                &pretty_key(key),
                &mut v,
                range,
                step,
                accent,
                help_text(section, key),
            ) {
                set_section_key(state, section, key, json!(v as f64), dirty, ui_state);
            }
        }
        Value::String(s) => {
            if looks_like_color(s) {
                paint_color_string(ui, state, section, key, s, dirty, ui_state);
                return;
            }
            if let Some(choices) = string_choices(section, key) {
                let selected = if choices.iter().any(|(v, _)| *v == s) {
                    s.as_str()
                } else {
                    choices.first().map(|(v, _)| *v).unwrap_or(s.as_str())
                };
                setting_row(ui, &pretty_key(key), help_text(section, key), |ui| {
                    if let Some(next) =
                        styled_choice_combo(ui, (section, key), selected, choices, 180.0)
                    {
                        set_section_key(state, section, key, json!(next), dirty, ui_state);
                    }
                });
                return;
            }
            // Titles / labels only — every other string is a dropdown (or read-only).
            if allows_free_text_setting(key) {
                let mut text = s.clone();
                setting_row(ui, &pretty_key(key), help_text(section, key), |ui| {
                    if text_field(ui, &mut text, "", 220.0).changed() {
                        set_section_key(state, section, key, json!(text), dirty, ui_state);
                    }
                });
                return;
            }
            setting_row(ui, &pretty_key(key), help_text(section, key), |ui| {
                ui.label(RichText::new(choice_label(s)).color(MUTED));
            });
        }
        Value::Object(map) if key == "colors" || key == "license_colors" => {
            accordion(
                ui,
                (section, "nested", key),
                &pretty_key(key),
                accent,
                false,
                |ui| {
                    let mut entries: Vec<_> = map.iter().collect();
                    entries.sort_by(|a, b| a.0.cmp(b.0));
                    for (ck, cv) in entries {
                        if let Some(hex) = cv.as_str() {
                            paint_nested_color(ui, state, section, key, ck, hex, dirty, ui_state);
                        }
                    }
                },
            );
        }
        Value::Object(map) if key == "header" || key == "footer" => {
            paint_table_slots(ui, state, section, key, map, dirty, accent, ui_state);
        }
        Value::Object(map) if key == "header_icons" || key == "footer_icons" => {
            paint_table_slot_icons(ui, state, section, key, map, dirty, accent, ui_state);
        }
        Value::Object(_) => {
            // Structured controls only — never raw JSON editors in widget settings.
            paint_structured_object(ui, state, section, key, value, dirty, accent, ui_state);
        }
        Value::Array(arr) => {
            paint_structured_array(ui, state, section, key, arr, dirty, accent, ui_state);
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
                let more_resp = button_kind(ui, "More...", ButtonKind::GhostAccent);
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

const TABLE_SLOT_SIDES: &[&str] = &["left", "center", "right"];

fn paint_table_slots(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    map: &serde_json::Map<String, Value>,
    dirty: &mut bool,
    _accent: Color32,
    ui_state: &mut SettingsUi,
) {
    let slots = table_slot_options(section);
    let labels: Vec<String> = slots.iter().map(|s| choice_label(s)).collect();
    let choices: Vec<(&str, &str)> = slots
        .iter()
        .zip(labels.iter())
        .map(|(v, l)| (*v, l.as_str()))
        .collect();
    ui.add_space(4.0);
    ui.label(
        RichText::new(pretty_key(key))
            .size(12.0)
            .color(theme::ROW_LABEL),
    );
    ui.add_space(2.0);
    for side in TABLE_SLOT_SIDES {
        let current = map
            .get(*side)
            .and_then(|v| v.as_str())
            .unwrap_or("none")
            .to_string();
        let selected = if choices.iter().any(|(v, _)| *v == current) {
            current
        } else {
            "none".into()
        };
        setting_row(ui, &pretty_key(side), None, |ui| {
            if let Some(next) =
                styled_choice_combo(ui, (section, key, *side), &selected, &choices, 200.0)
            {
                set_nested(state, section, key, side, json!(next), dirty, ui_state);
            }
        });
    }
}

fn paint_table_slot_icons(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    map: &serde_json::Map<String, Value>,
    dirty: &mut bool,
    accent: Color32,
    ui_state: &mut SettingsUi,
) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(pretty_key(key))
            .size(12.0)
            .color(theme::ROW_LABEL),
    );
    ui.add_space(2.0);
    for side in TABLE_SLOT_SIDES {
        let mut on = map.get(*side).and_then(|v| v.as_bool()).unwrap_or(false);
        setting_row(ui, &pretty_key(side), None, |ui| {
            if toggle_switch(ui, &mut on, accent, ui.id().with((section, key, *side))).changed() {
                set_nested(state, section, key, side, json!(on), dirty, ui_state);
            }
        });
    }
}

fn paint_color_string(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    hex: &str,
    dirty: &mut bool,
    ui_state: &mut SettingsUi,
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
            set_section_key(state, section, key, json!(hex), dirty, ui_state);
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
    ui_state: &mut SettingsUi,
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
            set_nested(state, section, group, key, json!(hex), dirty, ui_state);
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

fn set_global(
    state: &StateHandle,
    key: &str,
    val: Value,
    dirty: &mut bool,
    ui_state: &mut SettingsUi,
) {
    if let Some(mut st) = state.try_write() {
        let cfg = Arc::make_mut(&mut st.config);
        cfg.apply_cfg_patch(&json!({ key: val }));
        *dirty = true;
        ui_state.invalidate_section_cache();
    }
}

fn set_driver_groups(
    state: &StateHandle,
    val: Value,
    dirty: &mut bool,
    ui_state: &mut SettingsUi,
) {
    if let Some(mut st) = state.try_write() {
        let cfg = Arc::make_mut(&mut st.config);
        cfg.set_driver_groups(val);
        *dirty = true;
        ui_state.invalidate_section_cache();
    }
}

fn set_section_key(
    state: &StateHandle,
    section: &str,
    key: &str,
    val: Value,
    dirty: &mut bool,
    ui_state: &mut SettingsUi,
) {
    if let Some(mut st) = state.try_write() {
        let cfg = Arc::make_mut(&mut st.config);
        cfg.apply_cfg_patch(&json!({ section: { key: val.clone() } }));
        // Dot number (and similar) must land on the race base so a garage-profile
        // edit is not invisible once you go on track.
        cfg.write_shared_section_key(section, key, val.clone());
        if key == "panel_style" {
            let (w, h) = crate::config::preferred_panel_size(st.config.as_ref(), section);
            crate::state::fit_panel_size(&mut st.layout, section, w, h);
            st.save_layout_to_preset();
        }
        *dirty = true;
        ui_state.invalidate_section_cache();
    }
}

/// Relative / Standings linked row counts.
/// Relative: `rows` is the on-screen total including you → ahead + behind = rows − 1.
/// Standings: `rows` stays ahead + behind (window size helpers).
fn paint_linked_row_counts(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    dirty: &mut bool,
    accent: Color32,
    ui_state: &mut SettingsUi,
) {
    let include_self = section == "relative";
    let (total, ahead, behind) = {
        let st = state.read();
        let sec = st.config.section(section);
        let ahead = sec
            .get("rows_ahead")
            .and_then(|v| v.as_f64())
            .unwrap_or(3.0)
            .max(0.0)
            .round() as i32;
        let behind = sec
            .get("rows_behind")
            .and_then(|v| v.as_f64())
            .unwrap_or(3.0)
            .max(0.0)
            .round() as i32;
        let neighbors = ahead.max(0) + behind.max(0);
        let min_total = if include_self {
            neighbors + 1
        } else {
            neighbors
        };
        let total = sec
            .get("rows")
            .and_then(|v| v.as_f64())
            .map(|v| v.max(0.0).round() as i32)
            .unwrap_or(min_total);
        (total.max(min_total), ahead.max(0), behind.max(0))
    };
    let neighbor_budget = if include_self {
        (total - 1).max(0)
    } else {
        total.max(0)
    };

    let label = match key {
        "rows" => "Total rows",
        "rows_ahead" => "Rows ahead",
        "rows_behind" => "Rows behind",
        _ => return,
    };
    let mut v = match key {
        "rows" => total as f32,
        "rows_ahead" => ahead as f32,
        "rows_behind" => behind as f32,
        _ => return,
    };
    let min_v = if key == "rows" && include_self {
        1.0
    } else {
        0.0
    };
    let max = if key == "rows" {
        40.0
    } else {
        neighbor_budget as f32
    };
    if number_row(
        ui,
        label,
        &mut v,
        min_v..=max,
        1.0,
        accent,
        help_text(section, key),
    ) {
        let n = v.round().clamp(min_v, 40.0) as i32;
        let (new_total, new_ahead, new_behind) = match key {
            "rows" => {
                let t = if include_self { n.max(1) } else { n.max(0) };
                let budget = if include_self { t - 1 } else { t };
                let a = ahead.min(budget).max(0);
                (t, a, budget - a)
            }
            "rows_ahead" => {
                let a = n.clamp(0, neighbor_budget);
                (total, a, neighbor_budget - a)
            }
            "rows_behind" => {
                let b = n.clamp(0, neighbor_budget);
                (total, neighbor_budget - b, b)
            }
            _ => return,
        };
        if let Some(mut st) = state.try_write() {
            let cfg = Arc::make_mut(&mut st.config);
            cfg.apply_cfg_patch(&json!({
                section: {
                    "rows": new_total,
                    "rows_ahead": new_ahead,
                    "rows_behind": new_behind,
                }
            }));
            *dirty = true;
            ui_state.invalidate_section_cache();
        }
    }
}

fn column_order_from(
    values: &HashMap<String, Value>,
    catalog: &[&str],
    defaults: &[&str],
) -> Vec<String> {
    if let Some(arr) = values.get("column_order").and_then(|v| v.as_array()) {
        let cols: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .filter(|c| catalog.contains(&c.as_str()))
            .collect();
        if !cols.is_empty() {
            return cols;
        }
    }
    defaults.iter().map(|s| (*s).to_string()).collect()
}

fn ensure_required_columns(order: &mut Vec<String>, required: &[&str], catalog: &[&str]) {
    for req in required {
        if order.iter().any(|c| c == *req) {
            continue;
        }
        insert_catalog_column(order, req, catalog);
    }
}

fn insert_catalog_column(order: &mut Vec<String>, col: &str, catalog: &[&str]) {
    if order.iter().any(|c| c == col) {
        return;
    }
    let master_idx = catalog
        .iter()
        .position(|c| *c == col)
        .unwrap_or(catalog.len());
    let mut insert_at = order.len();
    for (i, existing) in order.iter().enumerate() {
        let ei = catalog
            .iter()
            .position(|c| c == existing)
            .unwrap_or(catalog.len());
        if ei > master_idx {
            insert_at = i;
            break;
        }
    }
    order.insert(insert_at, col.to_string());
}

fn paint_ordered_columns(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    catalog: &[&str],
    required: &[&str],
    dirty: &mut bool,
    accent: Color32,
    ui_state: &mut SettingsUi,
) {
    let values = cached_section_values(state, section, ui_state);
    let defaults: Vec<&str> = if section == "laptime_log" {
        vec!["lap", "time", "delta", "temp"]
    } else {
        vec!["badge", "position", "name", "license", "irating", "gap"]
    };
    let mut order = column_order_from(&values, catalog, &defaults);
    ensure_required_columns(&mut order, required, catalog);

    ui.label(
        RichText::new(help_text(section, key).unwrap_or("Visible columns"))
            .size(11.0)
            .color(MUTED),
    );
    ui.add_space(4.0);

    let mut next_order: Option<Vec<String>> = None;
    for i in 0..order.len() {
        let col = order[i].clone();
        let locked = required.contains(&col.as_str());
        ui.horizontal(|ui| {
            ui.scope(|ui| {
                ui.set_min_width(120.0);
                ui.label(RichText::new(choice_label(&col)).color(TITLE));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !locked {
                    let mut on = true;
                    if toggle_switch(
                        ui,
                        &mut on,
                        accent,
                        ui.id().with((section, "col_vis", col.as_str())),
                    )
                    .changed()
                        && !on
                    {
                        let mut o = order.clone();
                        o.retain(|c| c != &col);
                        ensure_required_columns(&mut o, required, catalog);
                        next_order = Some(o);
                    }
                } else {
                    ui.label(RichText::new("Required").size(11.0).color(MUTED));
                }
                let can_down = i + 1 < order.len() && next_order.is_none();
                if icon_button(
                    ui,
                    "chevron_down",
                    can_down,
                    ui.id().with((section, "col_dn", i)),
                )
                .clicked()
                    && can_down
                {
                    let mut o = order.clone();
                    o.swap(i, i + 1);
                    next_order = Some(o);
                }
                let can_up = i > 0 && next_order.is_none();
                if icon_button(
                    ui,
                    "chevron_up",
                    can_up,
                    ui.id().with((section, "col_up", i)),
                )
                .clicked()
                    && can_up
                {
                    let mut o = order.clone();
                    o.swap(i, i - 1);
                    next_order = Some(o);
                }
            });
        });
        ui.add_space(2.0);
    }

    let mut hidden: Vec<&str> = catalog
        .iter()
        .copied()
        .filter(|c| !order.iter().any(|o| o == *c))
        .collect();
    // List by friendly name so the add picker is easy to scan.
    hidden.sort_by(|a, b| {
        choice_label(a)
            .to_ascii_lowercase()
            .cmp(&choice_label(b).to_ascii_lowercase())
    });
    if !hidden.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new("Add column").size(11.0).color(MUTED));
        for col in hidden {
            let mut on = false;
            setting_row(ui, &choice_label(col), None, |ui| {
                if toggle_switch(
                    ui,
                    &mut on,
                    accent,
                    ui.id().with((section, "col_add", col)),
                )
                .changed()
                    && on
                    && next_order.is_none()
                {
                    let mut o = order.clone();
                    insert_catalog_column(&mut o, col, catalog);
                    ensure_required_columns(&mut o, required, catalog);
                    next_order = Some(o);
                }
            });
        }
    }

    if let Some(o) = next_order {
        if matches!(section, "relative" | "standings") {
            if let Some(widths) = values.get("widths").and_then(|v| v.as_object()) {
                let mut pruned = serde_json::Map::new();
                for (k, v) in widths {
                    if o.iter().any(|c| c == k) && k != "name" {
                        pruned.insert(k.clone(), v.clone());
                    }
                }
                set_section_key(state, section, "widths", Value::Object(pruned), dirty, ui_state);
            }
        }
        set_section_key(state, section, key, json!(o), dirty, ui_state);
    }
}

fn paint_table_column_flags(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    value: &Value,
    dirty: &mut bool,
    accent: Color32,
    ui_state: &mut SettingsUi,
) {
    let map = value.as_object();
    let mut stripe = map
        .and_then(|m| m.get("stripe"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    setting_row(
        ui,
        "Class stripe",
        Some("Colored bar beside position from the driver’s class color."),
        |ui| {
            if toggle_switch(
                ui,
                &mut stripe,
                accent,
                ui.id().with((section, "columns", "stripe")),
            )
            .changed()
            {
                set_nested(
                    state,
                    section,
                    "columns",
                    "stripe",
                    json!(stripe),
                    dirty,
                    ui_state,
                );
            }
        },
    );
}

fn paint_numeric_object(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    group: &str,
    value: &Value,
    dirty: &mut bool,
    accent: Color32,
    ui_state: &mut SettingsUi,
) {
    let Some(map) = value.as_object() else {
        return;
    };
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (ck, cv) in entries {
        let Some(n) = cv.as_f64() else {
            continue;
        };
        let mut v = n as f32;
        let (range, step) = if ck.contains("tau") {
            (0.05..=1.0, 0.01)
        } else {
            (0.01..=1.5, 0.01)
        };
        v = v.clamp(*range.start(), *range.end());
        if number_row(
            ui,
            &pretty_key(ck),
            &mut v,
            range,
            step,
            accent,
            help_text(section, group),
        ) {
            set_nested(
                state,
                section,
                group,
                ck,
                json!((v as f64 * 1000.0).round() / 1000.0),
                dirty,
                ui_state,
            );
        }
    }
}

fn paint_structured_object(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    value: &Value,
    dirty: &mut bool,
    accent: Color32,
    ui_state: &mut SettingsUi,
) {
    let Some(map) = value.as_object() else {
        return;
    };
    if map.is_empty() {
        ui.label(
            RichText::new("No options for this setting.")
                .size(11.0)
                .color(MUTED),
        );
        return;
    }
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (ck, cv) in entries {
        match cv {
            Value::Bool(b) => {
                let mut v = *b;
                setting_row(ui, &pretty_key(ck), None, |ui| {
                    if toggle_switch(
                        ui,
                        &mut v,
                        accent,
                        ui.id().with((section, key, ck.as_str())),
                    )
                    .changed()
                    {
                        set_nested(state, section, key, ck, json!(v), dirty, ui_state);
                    }
                });
            }
            Value::Number(n) => {
                let mut v = n.as_f64().unwrap_or(0.0) as f32;
                let (range, step) = number_setting_bounds(ck);
                v = v.clamp(*range.start(), *range.end());
                if number_row(ui, &pretty_key(ck), &mut v, range, step, accent, None) {
                    set_nested(
                        state,
                        section,
                        key,
                        ck,
                        json!(v as f64),
                        dirty,
                        ui_state,
                    );
                }
            }
            Value::String(s) if looks_like_color(s) => {
                paint_nested_color(ui, state, section, key, ck, s, dirty, ui_state);
            }
            Value::String(s) => {
                if let Some(choices) = string_choices(section, ck)
                    .or_else(|| string_choices(key, ck))
                {
                    let selected = if choices.iter().any(|(v, _)| *v == s) {
                        s.as_str()
                    } else {
                        choices.first().map(|(v, _)| *v).unwrap_or(s.as_str())
                    };
                    setting_row(ui, &pretty_key(ck), None, |ui| {
                        if let Some(next) = styled_choice_combo(
                            ui,
                            (section, key, ck.as_str()),
                            selected,
                            choices,
                            180.0,
                        ) {
                            set_nested(state, section, key, ck, json!(next), dirty, ui_state);
                        }
                    });
                } else if allows_free_text_setting(ck) {
                    let mut text = s.clone();
                    setting_row(ui, &pretty_key(ck), None, |ui| {
                        if text_field(ui, &mut text, "", 180.0).changed() {
                            set_nested(state, section, key, ck, json!(text), dirty, ui_state);
                        }
                    });
                } else {
                    setting_row(ui, &pretty_key(ck), None, |ui| {
                        ui.label(RichText::new(choice_label(s)).color(MUTED));
                    });
                }
            }
            _ => {
                // Nested structures: skip rather than dump JSON.
            }
        }
    }
}

fn paint_structured_array(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    key: &str,
    arr: &[Value],
    dirty: &mut bool,
    accent: Color32,
    ui_state: &mut SettingsUi,
) {
    if arr.iter().all(|v| v.as_str().is_some()) {
        // Generic string-list editor: reorder/remove only (no free-text add).
        ui.label(
            RichText::new(pretty_key(key)).size(12.0).color(TITLE),
        );
        let mut items: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        enum Edit {
            Swap(usize, usize),
            Remove(usize),
        }
        let mut edit: Option<Edit> = None;
        for i in 0..items.len() {
            let item = choice_label(&items[i]);
            ui.horizontal(|ui| {
                ui.label(RichText::new(item).color(TITLE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(
                        ui,
                        "chevron_down",
                        i + 1 < items.len() && edit.is_none(),
                        ui.id().with((section, key, "dn", i)),
                    )
                    .clicked()
                        && i + 1 < items.len()
                    {
                        edit = Some(Edit::Swap(i, i + 1));
                    }
                    if icon_button(
                        ui,
                        "chevron_up",
                        i > 0 && edit.is_none(),
                        ui.id().with((section, key, "up", i)),
                    )
                    .clicked()
                        && i > 0
                    {
                        edit = Some(Edit::Swap(i, i - 1));
                    }
                    if button_kind(ui, "Remove", ButtonKind::GhostAccent).clicked() {
                        edit = Some(Edit::Remove(i));
                    }
                });
            });
        }
        let _ = accent;
        if let Some(e) = edit {
            match e {
                Edit::Swap(a, b) => items.swap(a, b),
                Edit::Remove(i) => {
                    items.remove(i);
                }
            }
            set_section_key(state, section, key, json!(items), dirty, ui_state);
        }
        return;
    }
    ui.label(
        RichText::new("This list can’t be edited here.")
            .size(11.0)
            .color(MUTED),
    );
}

fn paint_table_widths(
    ui: &mut Ui,
    state: &StateHandle,
    section: &str,
    dirty: &mut bool,
    accent: Color32,
    ui_state: &mut SettingsUi,
) {
    let values = cached_section_values(state, section, ui_state);
    let order = column_order_from(
        &values,
        TABLE_DATA_COLUMNS,
        &["badge", "position", "name", "license", "irating", "gap"],
    );
    let width_map = values.get("widths").and_then(|v| v.as_object());

    // Name is flex (fills leftover space); only fixed columns are resizable.
    let resizeable: Vec<&str> = order
        .iter()
        .map(|s| s.as_str())
        .filter(|c| *c != "name")
        .collect();

    if resizeable.is_empty() {
        ui.label(
            RichText::new("No fixed-width columns are visible. Enable columns under Columns.")
                .size(11.0)
                .color(MUTED),
        );
        return;
    }

    ui.label(
        RichText::new(
            help_text(section, "widths")
                .unwrap_or("Column width as a multiple of row height."),
        )
        .size(11.0)
        .color(MUTED),
    );
    ui.add_space(4.0);

    for col in resizeable {
        let default = default_table_col_width(col);
        let mut v = width_map
            .and_then(|m| m.get(col))
            .and_then(|x| x.as_f64())
            .map(|x| x as f32)
            .unwrap_or(default)
            .clamp(0.4, 4.0);
        if number_row(
            ui,
            &choice_label(col),
            &mut v,
            0.4..=4.0,
            0.05,
            accent,
            Some("Width × row height."),
        ) {
            set_nested(
                state,
                section,
                "widths",
                col,
                json!((v as f64 * 100.0).round() / 100.0),
                dirty,
                ui_state,
            );
        }
    }
}

fn set_nested(
    state: &StateHandle,
    section: &str,
    group: &str,
    key: &str,
    val: Value,
    dirty: &mut bool,
    ui_state: &mut SettingsUi,
) {
    if let Some(mut st) = state.try_write() {
        let cfg = Arc::make_mut(&mut st.config);
        cfg.apply_cfg_patch(&json!({ section: { group: { key: val } } }));
        *dirty = true;
        ui_state.invalidate_section_cache();
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
