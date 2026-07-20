//! Custom-painted controls matching Python ConfigEditor widgets.

use super::theme::{
    self, spaced_upper, ACCENT, ACCENT_DIM, BLUE, BTN_BORDER, BTN_RADIUS, CARD_BORDER, CARD_RADIUS,
    FIELD_RADIUS, GROOVE, INPUT_BORDER, LABEL_WIDTH, MUTED, NAV_BORDER, NAV_IDLE, NAV_ITEM_H,
    ORANGE, POPUP_BG, ROW_LABEL, SEARCH_RADIUS, TITLE, YELLOW,
};
use egui::{
    self, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind,
    Ui, Vec2,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Default,
    Primary,
    Danger,
    Go,
    Stop,
    Warn,
    GhostAccent,
}

/// Pill toggle (Python `ToggleSwitch`).
pub fn toggle_switch(ui: &mut Ui, on: &mut bool, accent: Color32, id: egui::Id) -> Response {
    let desired = Vec2::new(46.0, 26.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let mut resp = ui.interact(rect, id, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(id.with("knob"), *on, 0.14);
    let p = ui.painter();
    let off = Color32::from_rgb(0x3a, 0x41, 0x50);
    let track = Color32::from_rgb(
        (off.r() as f32 + (accent.r() as f32 - off.r() as f32) * t) as u8,
        (off.g() as f32 + (accent.g() as f32 - off.g() as f32) * t) as u8,
        (off.b() as f32 + (accent.b() as f32 - off.b() as f32) * t) as u8,
    );
    p.rect_filled(rect, 13.0, track);
    let knob_r = 10.0;
    let pad = 3.0;
    let cx_off = rect.left() + pad + knob_r;
    let cx_on = rect.right() - pad - knob_r;
    let cx = cx_off + (cx_on - cx_off) * t;
    p.circle_filled(
        Pos2::new(cx, rect.center().y),
        knob_r,
        Color32::from_rgb(0xf6, 0xf8, 0xfb),
    );
    resp
}

/// Left nav item with accent bar + optional status dot.
pub fn nav_item(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    accent: Color32,
    dot_on: bool,
    id: egui::Id,
) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, NAV_ITEM_H), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(id.with("sel"), selected, 0.14);
    let p = ui.painter();
    let pill = rect.shrink2(Vec2::new(6.0, 3.0));
    if t > 0.01 {
        let a = (34.0 * t) as u8;
        p.rect_filled(
            pill,
            10.0,
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), a),
        );
        p.rect_stroke(
            pill,
            10.0,
            Stroke::new(1.3_f32, Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), (255.0 * t) as u8)),
            StrokeKind::Inside,
        );
        let bar = Rect::from_min_max(pill.min, Pos2::new(pill.left() + 3.0, pill.bottom()));
        p.rect_filled(
            bar,
            2.0,
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), (255.0 * t) as u8),
        );
    }
    let text_col = Color32::from_rgb(
        (NAV_IDLE.r() as f32 + (TITLE.r() as f32 - NAV_IDLE.r() as f32) * t) as u8,
        (NAV_IDLE.g() as f32 + (TITLE.g() as f32 - NAV_IDLE.g() as f32) * t) as u8,
        (NAV_IDLE.b() as f32 + (TITLE.b() as f32 - NAV_IDLE.b() as f32) * t) as u8,
    );
    p.text(
        rect.left_center() + Vec2::new(18.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        text_col,
    );
    // Hide off-dots so the rail isn't sparkly.
    if dot_on {
        p.circle_filled(rect.right_center() + Vec2::new(-16.0, 0.0), 4.0, accent);
    } else {
        p.circle_filled(
            rect.right_center() + Vec2::new(-16.0, 0.0),
            3.0,
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 40),
        );
    }
    resp
}

/// Setting row: 170px label, optional help, stretch, then control.
pub fn setting_row(ui: &mut Ui, label: &str, help: Option<&str>, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        let (lab_rect, _) = ui.allocate_exact_size(Vec2::new(LABEL_WIDTH, 22.0), Sense::hover());
        ui.painter().text(
            lab_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            ROW_LABEL,
        );
        if let Some(h) = help {
            help_pill(ui, h);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            control(ui);
        });
    });
}

fn help_pill(ui: &mut Ui, tip: &str) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::hover());
    let hovered = resp.hovered();
    let border = if hovered { ACCENT_DIM } else { INPUT_BORDER };
    let text = if hovered { TITLE } else { MUTED };
    ui.painter().rect_stroke(
        rect,
        FIELD_RADIUS,
        Stroke::new(1.0_f32, border),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "?",
        FontId::proportional(11.0),
        text,
    );
    resp.on_hover_text(tip);
}

/// Horizontal enable card: title+hint left, toggle right.
pub fn enable_card_row(
    ui: &mut Ui,
    title: &str,
    hint: &str,
    accent: Color32,
    on: &mut bool,
    id: egui::Id,
) -> bool {
    let mut changed = false;
    egui::Frame::new()
        .fill(theme::card_bg())
        .stroke(Stroke::new(1.0_f32, CARD_BORDER))
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin {
            left: 15,
            right: 15,
            top: 11,
            bottom: 11,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(13.0).strong().color(TITLE));
                    if !hint.is_empty() {
                        ui.label(RichText::new(hint).size(11.0).color(theme::MUTED));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    changed = toggle_switch(ui, on, accent, id).changed();
                });
            });
        });
    changed
}

/// Generic card wrapper.
pub fn enable_card(ui: &mut Ui, title: &str, _accent: Color32, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(theme::card_bg())
        .stroke(Stroke::new(1.0_f32, CARD_BORDER))
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin {
            left: 15,
            right: 15,
            top: 11,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(13.0).strong().color(TITLE));
            ui.add_space(6.0);
            body(ui);
        });
}

/// Slider + spin aligned after a 170px label.
pub fn number_row(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    accent: Color32,
    help: Option<&str>,
) -> bool {
    let mut changed = false;
    setting_row(ui, label, help, |ui| {
        // RTL parent: spin first → rightmost, slider left of it.
        changed |= number_spin(ui, value, range.clone(), step);
        ui.add_space(8.0);
        changed |= number_slider(ui, value, range, step, accent);
    });
    changed
}

fn number_slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    accent: Color32,
) -> bool {
    let width = 180.0;
    let height = 22.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let min = *range.start();
    let max = *range.end();
    let span = (max - min).max(f32::EPSILON);
    let mut changed = false;
    if (resp.dragged() || resp.clicked()) && ui.input(|i| i.pointer.interact_pos()).is_some() {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let raw = min + span * t;
            let stepped = if step > 0.0 {
                (raw / step).round() * step
            } else {
                raw
            };
            let next = stepped.clamp(min, max);
            if (*value - next).abs() > f32::EPSILON {
                *value = next;
                changed = true;
            }
        }
    }

    let t = ((*value - min) / span).clamp(0.0, 1.0);
    let groove = Rect::from_center_size(rect.center(), Vec2::new(width, 6.0));
    let p = ui.painter();
    // Continuous solid track (no segmented / dashed fill).
    p.rect_filled(groove, 3.0, GROOVE);
    let filled_w = groove.width() * t;
    if filled_w > 0.5 {
        p.rect_filled(
            Rect::from_min_max(
                groove.left_top(),
                Pos2::new(groove.left() + filled_w, groove.bottom()),
            ),
            3.0,
            accent,
        );
    }
    let knob = Pos2::new(groove.left() + groove.width() * t, groove.center().y);
    let handle = if resp.hovered() || resp.dragged() {
        Color32::WHITE
    } else {
        Color32::from_rgb(0xf6, 0xf8, 0xfb)
    };
    p.circle_filled(knob, 7.0, handle);
    p.circle_stroke(knob, 7.0, Stroke::new(2.0_f32, accent));
    changed
}

fn number_spin(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
) -> bool {
    let mut changed = false;
    egui::Frame::new()
        .fill(theme::input_bg())
        .stroke(Stroke::new(1.0_f32, INPUT_BORDER))
        .corner_radius(FIELD_RADIUS)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.set_width(94.0);
            // Suppress DragValue's own frame so only the outer chrome shows.
            ui.scope(|ui| {
                let v = ui.visuals_mut();
                v.widgets.inactive.bg_stroke = Stroke::NONE;
                v.widgets.inactive.bg_fill = Color32::TRANSPARENT;
                v.widgets.hovered.bg_stroke = Stroke::NONE;
                v.widgets.hovered.bg_fill = Color32::TRANSPARENT;
                v.widgets.active.bg_stroke = Stroke::NONE;
                v.widgets.active.bg_fill = Color32::TRANSPARENT;
                if ui
                    .add(
                        egui::DragValue::new(value)
                            .range(range)
                            .speed(step.max(0.01) as f64)
                            .fixed_decimals(2),
                    )
                    .changed()
                {
                    changed = true;
                }
            });
        });
    changed
}

/// Top tabs chrome wrapper (Python `#topTabs`).
pub fn top_tabs_frame(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(13, 15, 19, 199))
        .stroke(Stroke::new(1.0_f32, NAV_BORDER))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            body(ui);
        });
}

/// Top tab pill (Widgets / Settings).
pub fn top_tab_button(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(12.0),
        if selected { TITLE } else { MUTED },
    );
    let w = galley.size().x + 40.0;
    let h = galley.size().y + 18.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let fill = if selected {
        Color32::from_rgba_unmultiplied(70, 223, 122, 41)
    } else if resp.hovered() {
        Color32::from_rgba_unmultiplied(255, 255, 255, 10)
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if selected {
        Stroke::new(1.0_f32, ACCENT_DIM)
    } else {
        Stroke::NONE
    };
    ui.painter().rect_filled(rect, 9.0, fill);
    ui.painter()
        .rect_stroke(rect, 9.0, stroke, StrokeKind::Inside);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.0),
        if selected { TITLE } else { MUTED },
    );
    resp
}

/// Small preset toolbar button.
pub fn preset_button(ui: &mut Ui, label: &str) -> Response {
    button_kind(ui, label, ButtonKind::Default)
}

pub fn button_kind(ui: &mut Ui, label: &str, kind: ButtonKind) -> Response {
    let strong = matches!(kind, ButtonKind::Primary | ButtonKind::Go);
    let font = if strong {
        FontId::new(12.0, egui::FontFamily::Proportional)
    } else {
        FontId::proportional(12.0)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), theme::TEXT);
    let w = (galley.size().x + 32.0).max(64.0);
    let h = galley.size().y + 18.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let (fill, hover_fill, stroke, text) = match kind {
        ButtonKind::Default => (
            theme::button_bg(),
            theme::button_hover_bg(),
            Stroke::new(1.0_f32, BTN_BORDER),
            theme::TEXT,
        ),
        ButtonKind::Primary => (
            BLUE,
            Color32::from_rgb(0x5e, 0xa7, 0xff),
            Stroke::new(1.0_f32, BLUE),
            Color32::from_rgb(0x06, 0x1b, 0x32),
        ),
        ButtonKind::Danger | ButtonKind::Stop => (
            Color32::TRANSPARENT,
            Color32::from_rgba_unmultiplied(255, 148, 22, 31),
            Stroke::new(1.0_f32, ORANGE),
            ORANGE,
        ),
        ButtonKind::Warn => (
            Color32::TRANSPARENT,
            Color32::from_rgba_unmultiplied(255, 210, 58, 31),
            Stroke::new(1.0_f32, YELLOW),
            YELLOW,
        ),
        ButtonKind::GhostAccent => (
            Color32::TRANSPARENT,
            Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), 31),
            Stroke::new(
                1.0_f32,
                Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), 84),
            ),
            ACCENT,
        ),
        ButtonKind::Go => (
            Color32::from_rgb(0x3d, 0xc9, 0x6a),
            Color32::from_rgb(0x61, 0xec, 0x8e),
            Stroke::new(1.0_f32, ACCENT),
            Color32::from_rgb(0x05, 0x1f, 0x0e),
        ),
    };
    let fill = if resp.hovered() { hover_fill } else { fill };
    ui.painter().rect_filled(rect, BTN_RADIUS, fill);
    ui.painter()
        .rect_stroke(rect, BTN_RADIUS, stroke, StrokeKind::Inside);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        text,
    );
    resp
}

pub fn styled_combo(
    ui: &mut Ui,
    id_source: impl std::hash::Hash,
    selected: &str,
    options: &[String],
    width: f32,
) -> Option<String> {
    let id = ui.make_persistent_id(id_source);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());
    let open = ui.memory(|mem| mem.is_popup_open(id));
    let stroke = if open {
        Stroke::new(1.0_f32, ACCENT)
    } else if resp.hovered() {
        Stroke::new(1.0_f32, ACCENT_DIM)
    } else {
        Stroke::new(1.0_f32, INPUT_BORDER)
    };
    ui.painter()
        .rect_filled(rect, FIELD_RADIUS, theme::input_bg());
    ui.painter()
        .rect_stroke(rect, FIELD_RADIUS, stroke, StrokeKind::Inside);
    ui.painter().text(
        rect.left_center() + Vec2::new(11.0, 0.0),
        egui::Align2::LEFT_CENTER,
        selected,
        FontId::proportional(12.0),
        theme::TEXT,
    );
    let cx = rect.right() - 15.0;
    let cy = rect.center().y + 1.0;
    ui.painter().line_segment(
        [Pos2::new(cx - 4.0, cy - 2.0), Pos2::new(cx, cy + 2.0)],
        Stroke::new(1.5_f32, NAV_IDLE),
    );
    ui.painter().line_segment(
        [Pos2::new(cx, cy + 2.0), Pos2::new(cx + 4.0, cy - 2.0)],
        Stroke::new(1.5_f32, NAV_IDLE),
    );
    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(id));
    }

    let mut picked = None;
    egui::popup::popup_below_widget(
        ui,
        id,
        &resp,
        egui::popup::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(width);
            egui::Frame::new()
                .fill(POPUP_BG)
                .stroke(Stroke::new(1.0_f32, INPUT_BORDER))
                .corner_radius(FIELD_RADIUS)
                .inner_margin(egui::Margin::symmetric(6, 6))
                .show(ui, |ui| {
                    for opt in options {
                        let selected_row = opt == selected;
                        let row_resp = nav_like_row(ui, opt, selected_row);
                        if row_resp.clicked() {
                            picked = Some(opt.clone());
                            ui.memory_mut(|mem| mem.close_popup());
                        }
                    }
                });
        },
    );
    picked
}

/// Driver-group icon picker: Font Awesome glyph + friendly label.
pub fn icon_combo(ui: &mut Ui, id_source: &str, selected_key: &str, width: f32) -> Option<String> {
    let options = crate::driver_groups::DRIVER_GROUP_ICONS;
    let id = ui.make_persistent_id(id_source);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());
    let open = ui.memory(|mem| mem.is_popup_open(id));
    let stroke = if open {
        Stroke::new(1.0_f32, ACCENT)
    } else if resp.hovered() {
        Stroke::new(1.0_f32, ACCENT_DIM)
    } else {
        Stroke::new(1.0_f32, INPUT_BORDER)
    };
    ui.painter()
        .rect_filled(rect, FIELD_RADIUS, theme::input_bg());
    ui.painter()
        .rect_stroke(rect, FIELD_RADIUS, stroke, StrokeKind::Inside);
    paint_icon_label(
        ui,
        rect.left_center() + Vec2::new(11.0, 0.0),
        selected_key,
        theme::TEXT,
    );
    let cx = rect.right() - 15.0;
    let cy = rect.center().y + 1.0;
    ui.painter().line_segment(
        [Pos2::new(cx - 4.0, cy - 2.0), Pos2::new(cx, cy + 2.0)],
        Stroke::new(1.5_f32, NAV_IDLE),
    );
    ui.painter().line_segment(
        [Pos2::new(cx, cy + 2.0), Pos2::new(cx + 4.0, cy - 2.0)],
        Stroke::new(1.5_f32, NAV_IDLE),
    );
    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(id));
    }

    let mut picked = None;
    egui::popup::popup_below_widget(
        ui,
        id,
        &resp,
        egui::popup::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(width);
            egui::Frame::new()
                .fill(POPUP_BG)
                .stroke(Stroke::new(1.0_f32, INPUT_BORDER))
                .corner_radius(FIELD_RADIUS)
                .inner_margin(egui::Margin::symmetric(6, 6))
                .show(ui, |ui| {
                    for &key in options {
                        let selected_row = key == selected_key;
                        let row_resp = icon_nav_row(ui, key, selected_row);
                        if row_resp.clicked() {
                            picked = Some(key.to_string());
                            ui.memory_mut(|mem| mem.close_popup());
                        }
                    }
                });
        },
    );
    picked
}

fn paint_icon_label(ui: &mut Ui, origin: Pos2, key: &str, color: Color32) {
    let mut x = origin.x;
    if let Some(g) = crate::icons::glyph(key) {
        ui.painter().text(
            Pos2::new(x, origin.y),
            egui::Align2::LEFT_CENTER,
            g,
            crate::icons::font_id(14.0),
            color,
        );
        x += 22.0;
    }
    ui.painter().text(
        Pos2::new(x, origin.y),
        egui::Align2::LEFT_CENTER,
        crate::icons::label(key),
        FontId::proportional(12.0),
        color,
    );
}

fn icon_nav_row(ui: &mut Ui, key: &str, selected: bool) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
    if selected || resp.hovered() {
        let fill = if selected {
            Color32::from_rgba_unmultiplied(70, 223, 122, 41)
        } else {
            theme::button_hover_bg()
        };
        ui.painter().rect_filled(rect, 7.0, fill);
    }
    let color = if selected { TITLE } else { theme::TEXT };
    paint_icon_label(ui, rect.left_center() + Vec2::new(10.0, 0.0), key, color);
    resp
}

fn nav_like_row(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
    if selected || resp.hovered() {
        let fill = if selected {
            Color32::from_rgba_unmultiplied(70, 223, 122, 41)
        } else {
            theme::button_hover_bg()
        };
        ui.painter().rect_filled(rect, 7.0, fill);
    }
    ui.painter().text(
        rect.left_center() + Vec2::new(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        if selected { TITLE } else { theme::TEXT },
    );
    resp
}

pub fn text_field(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    framed_text_edit(
        ui,
        value,
        hint,
        width,
        FIELD_RADIUS,
        egui::Margin::symmetric(10, 6),
    )
}

pub fn search_field(ui: &mut Ui, value: &mut String, hint: &str) -> Response {
    framed_text_edit(
        ui,
        value,
        hint,
        f32::INFINITY,
        SEARCH_RADIUS,
        egui::Margin {
            left: 14,
            right: 14,
            top: 10,
            bottom: 10,
        },
    )
}

fn framed_text_edit(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    width: f32,
    radius: f32,
    margin: egui::Margin,
) -> Response {
    let id = ui.next_auto_id();
    let focused = ui.memory(|m| m.has_focus(id));
    let stroke = if focused {
        Stroke::new(1.0_f32, ACCENT)
    } else {
        Stroke::new(1.0_f32, INPUT_BORDER)
    };
    egui::Frame::new()
        .fill(theme::input_bg())
        .stroke(stroke)
        .corner_radius(radius)
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .id(id)
                    .hint_text(hint)
                    .desired_width(width)
                    .frame(false),
            )
        })
        .inner
}

/// Accent status line (Python `#status`).
pub fn status_line(ui: &mut Ui, text: &str, opacity: f32) {
    if !text.is_empty() && opacity > 0.01 {
        let a = (255.0 * opacity.clamp(0.0, 1.0)) as u8;
        ui.label(
            RichText::new(text)
                .size(11.0)
                .color(Color32::from_rgba_unmultiplied(
                    ACCENT.r(),
                    ACCENT.g(),
                    ACCENT.b(),
                    a,
                )),
        );
        if opacity < 0.999 {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}

/// Python-style accordion header with joined body frame.
///
/// `id_salt` must be unique per accordion instance (include section + role)
/// so nested titles like "Colors" do not share CollapsingState.
pub fn accordion<R>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    title: &str,
    accent: Color32,
    default_open: bool,
    body: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let id = ui.make_persistent_id(("settings_accordion", id_salt));
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        default_open,
    );
    let prev_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;

    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
    if resp.clicked() {
        state.toggle(ui);
    }
    let open = state.is_open();
    state.store(ui.ctx());
    let t = ui
        .ctx()
        .animate_bool_with_time(id.with("open_t"), open, 0.16);
    let fill = Color32::from_rgba_unmultiplied(20, 23, 29, 217);
    let stroke_t = ((t - 0.0) / 0.15).clamp(0.0, 1.0);
    let stroke_base = if resp.hovered() { ACCENT_DIM } else { CARD_BORDER };
    let stroke_col = Color32::from_rgb(
        (stroke_base.r() as f32 + (ACCENT_DIM.r() as f32 - stroke_base.r() as f32) * stroke_t) as u8,
        (stroke_base.g() as f32 + (ACCENT_DIM.g() as f32 - stroke_base.g() as f32) * stroke_t) as u8,
        (stroke_base.b() as f32 + (ACCENT_DIM.b() as f32 - stroke_base.b() as f32) * stroke_t) as u8,
    );
    // Soften bottom corners as the body opens (no hard pop at 0.15).
    let open_corner = ((1.0 - t) * theme::ACCORDION_RADIUS as f32).round() as u8;
    let header_radius = CornerRadius {
        nw: theme::ACCORDION_RADIUS as u8,
        ne: theme::ACCORDION_RADIUS as u8,
        sw: open_corner,
        se: open_corner,
    };
    ui.painter().rect_filled(rect, header_radius, fill);
    ui.painter().rect_stroke(
        rect,
        header_radius,
        Stroke::new(1.0_f32, stroke_col),
        StrokeKind::Inside,
    );
    let bar = Rect::from_min_max(rect.min, Pos2::new(rect.left() + 3.0, rect.bottom()));
    ui.painter().rect_filled(
        bar,
        2.0,
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), (255.0 * t) as u8),
    );
    let title_col = Color32::from_rgb(
        (ROW_LABEL.r() as f32 + (TITLE.r() as f32 - ROW_LABEL.r() as f32) * t) as u8,
        (ROW_LABEL.g() as f32 + (TITLE.g() as f32 - ROW_LABEL.g() as f32) * t) as u8,
        (ROW_LABEL.b() as f32 + (TITLE.b() as f32 - ROW_LABEL.b() as f32) * t) as u8,
    );
    let chevron_key = if t > 0.5 {
        "chevron_down"
    } else {
        "chevron_right"
    };
    let chevron_x = rect.left() + 14.0;
    if let Some(g) = crate::icons::glyph(chevron_key) {
        ui.painter().text(
            Pos2::new(chevron_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            g,
            crate::icons::font_id(12.0),
            title_col,
        );
    }
    ui.painter().text(
        Pos2::new(chevron_x + 20.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        spaced_upper(title),
        FontId::proportional(11.0),
        title_col,
    );

    // Natural-height body when open — no clip/measure height animation
    // (that thrashed layout inside ScrollArea). Fade content opacity instead.
    let out = if t <= 0.001 {
        None
    } else {
        let bottom_only = CornerRadius {
            nw: 0,
            ne: 0,
            sw: theme::ACCORDION_RADIUS as u8,
            se: theme::ACCORDION_RADIUS as u8,
        };
        let border = Color32::from_rgb(0x20, 0x24, 0x2c);
        let inner = egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(13, 16, 20, (140.0 * t) as u8))
            .stroke(Stroke::NONE)
            .corner_radius(bottom_only)
            .inner_margin(egui::Margin {
                left: 12,
                right: 12,
                top: 10,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 7.0;
                ui.set_opacity(t);
                body(ui)
            });
        let r = inner.response.rect;
        let p = ui.painter();
        let ba = (255.0 * t) as u8;
        p.line_segment(
            [
                Pos2::new(r.left(), r.top()),
                Pos2::new(r.left(), r.bottom()),
            ],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(border.r(), border.g(), border.b(), ba)),
        );
        p.line_segment(
            [
                Pos2::new(r.right(), r.top()),
                Pos2::new(r.right(), r.bottom()),
            ],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(border.r(), border.g(), border.b(), ba)),
        );
        p.line_segment(
            [
                Pos2::new(r.left(), r.bottom()),
                Pos2::new(r.right(), r.bottom()),
            ],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(border.r(), border.g(), border.b(), ba)),
        );
        p.rect_filled(
            Rect::from_min_max(r.min, Pos2::new(r.left() + 2.0, r.bottom())),
            1.0,
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), (51.0 * t) as u8),
        );
        Some(inner.inner)
    };

    ui.spacing_mut().item_spacing.y = prev_spacing;
    out
}

pub fn color_button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    hex: &str,
    rgba: &mut [f32; 4],
) -> bool {
    let col = Color32::from_rgba_unmultiplied(
        (rgba[0] * 255.0) as u8,
        (rgba[1] * 255.0) as u8,
        (rgba[2] * 255.0) as u8,
        (rgba[3] * 255.0) as u8,
    );
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(150.0, 28.0), Sense::click());
    ui.painter().rect_filled(rect, FIELD_RADIUS, col);
    ui.painter().rect_stroke(
        rect,
        FIELD_RADIUS,
        Stroke::new(1.0_f32, INPUT_BORDER),
        StrokeKind::Inside,
    );
    let luminance = 0.299 * col.r() as f32 + 0.587 * col.g() as f32 + 0.114 * col.b() as f32;
    let text = if luminance > 150.0 {
        Color32::from_rgb(0x10, 0x13, 0x19)
    } else {
        Color32::from_rgb(0xf2, 0xf4, 0xf7)
    };
    ui.painter().text(
        rect.left_center() + Vec2::new(9.0, 0.0),
        egui::Align2::LEFT_CENTER,
        hex,
        FontId::monospace(11.0),
        text,
    );
    let popup_id = ui.make_persistent_id(("color_popup", id_salt));
    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }
    let mut changed = false;
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &resp,
        egui::popup::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            egui::Frame::new()
                .fill(POPUP_BG)
                .stroke(Stroke::new(1.0_f32, INPUT_BORDER))
                .corner_radius(FIELD_RADIUS)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    changed = ui.color_edit_button_rgba_unmultiplied(rgba).changed();
                });
        },
    );
    changed
}

/// Info card for Python-only sections.
pub fn info_card(ui: &mut Ui, title: &str, body: &str) {
    enable_card(ui, title, theme::MUTED, |ui| {
        ui.label(RichText::new(body).size(11.0).color(theme::MUTED));
    });
}
