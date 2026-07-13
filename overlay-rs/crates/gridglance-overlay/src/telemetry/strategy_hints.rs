//! Minimal undercut / cover tags for the relative list (Python pit_strategy subset).

use crate::config::OverlayConfig;

use super::tables::TableRow;
use super::FuelCalcState;

/// Strategy hint painted on a relative row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StratTag {
    /// Car ahead within undercut range (opportunity to jump them by pitting).
    Undercut,
    /// Car behind within cover range (risk of losing them if you stay out).
    Cover,
}

impl StratTag {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Undercut => "undercut",
            Self::Cover => "cover",
        }
    }
}

/// True when fuel/strategy pit window is open enough to show undercut/cover cues.
pub fn strategy_window_active(fuel: &FuelCalcState, fuel_pct: f32, cfg: &OverlayConfig) -> bool {
    if !cfg.bool_key("relative", "show_strategy_hints", true) {
        return false;
    }
    if fuel.window_open {
        return true;
    }
    let thresh = cfg.f64_key("relative", "strategy_fuel_pct_thresh", 0.18) as f32;
    fuel_pct > 0.0 && fuel_pct <= thresh
}

/// Tag relative rows from signed gaps (ahead > 0, behind < 0).
pub fn apply_strategy_hints(rows: &mut [TableRow], cfg: &OverlayConfig) {
    let undercut_max = cfg.f64_key("relative", "undercut_gap_max_s", 12.0) as f32;
    let cover_max = cfg.f64_key("relative", "cover_gap_max_s", 8.0) as f32;

    // Nearest ahead / behind only (tightest gaps).
    let mut best_ahead: Option<(usize, f32)> = None;
    let mut best_behind: Option<(usize, f32)> = None;
    for (i, row) in rows.iter().enumerate() {
        if row.empty || row.is_player {
            continue;
        }
        let Some(g) = row.gap_secs else {
            continue;
        };
        if g > 0.0 {
            if best_ahead.map(|(_, d)| g < d).unwrap_or(true) {
                best_ahead = Some((i, g));
            }
        } else if g < 0.0 {
            let abs = g.abs();
            if best_behind.map(|(_, d)| abs < d).unwrap_or(true) {
                best_behind = Some((i, abs));
            }
        }
    }

    if let Some((i, g)) = best_ahead {
        if g <= undercut_max {
            rows[i].strat_tag = Some(StratTag::Undercut.as_str().into());
        }
    }
    if let Some((i, g)) = best_behind {
        if g <= cover_max {
            rows[i].strat_tag = Some(StratTag::Cover.as_str().into());
        }
    }
}
