//! Shared timing-table painter for relative / standings.

use crate::chrome::{draw_card, draw_section_header, label, panel_pad};
use crate::config::OverlayConfig;
use crate::telemetry::CarRow;
use egui::{Align2, Pos2, Ui};

pub fn paint_table(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    title: &str,
    cars: &[CarRow],
) {
    let rect = crate::chrome::full_rect(ui);
    let (card, radius) = draw_card(ui, cfg, section, rect);
    let pad = panel_pad(card.height());
    let mut y = card.top() + pad;
    let hh = (card.height() * 0.08).max(22.0);
    let hdr = egui::Rect::from_min_size(
        Pos2::new(card.left() + pad, y),
        egui::vec2(card.width() - 2.0 * pad, hh),
    );
    draw_section_header(ui, cfg, section, hdr, title, radius);
    y += hh + 4.0;

    let rh = cfg.f64_key(section, "row_height_px", 36.0) as f32;
    let text = cfg.color(section, "text", "#f4f6f8");
    let muted = cfg.color(section, "muted", "#8b93a1");
    let alt = cfg.color(section, "row_alt", "#ffffff14");
    let player = cfg.color(section, "player_row", "#ff941658");

    // Column header
    let cols_y = y;
    let row_w = card.width() - 2.0 * pad;
    let left = card.left() + pad;
    for (i, h) in ["P", "#", "NAME", "GAP", "LAST"].iter().enumerate() {
        let x = col_x(left, row_w, i);
        label(
            ui,
            Pos2::new(x, cols_y + rh * 0.35),
            Align2::LEFT_CENTER,
            h,
            rh * 0.28,
            muted,
            true,
        );
    }
    y += rh * 0.7;

    for (idx, car) in cars.iter().enumerate() {
        if y + rh > card.bottom() - pad {
            break;
        }
        let row = egui::Rect::from_min_size(Pos2::new(left, y), egui::vec2(row_w, rh));
        if car.is_player {
            ui.painter()
                .rect_filled(row, egui::CornerRadius::same(4), player);
        } else if idx % 2 == 1 {
            ui.painter()
                .rect_filled(row, egui::CornerRadius::same(4), alt);
        }
        let vals = [
            format!("{}", car.position),
            car.car_number.clone(),
            car.name.clone(),
            car.gap.clone(),
            car.last_lap.clone(),
        ];
        for (i, v) in vals.iter().enumerate() {
            let x = col_x(left, row_w, i);
            let col = if car.is_speaking {
                cfg.color(section, "faster", "#46df7a")
            } else if i == 0 {
                muted
            } else {
                text
            };
            label(
                ui,
                Pos2::new(x, row.center().y),
                Align2::LEFT_CENTER,
                v,
                rh * 0.36,
                col,
                i == 2,
            );
        }
        y += rh;
    }
}

fn col_x(left: f32, width: f32, i: usize) -> f32 {
    let fracs = [0.0_f32, 0.10, 0.22, 0.62, 0.78];
    left + width * fracs.get(i).copied().unwrap_or(0.0) + 6.0
}
