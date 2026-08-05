//! Host-owned table / panel animation state (replaces egui Context temp).

use super::inputs_panel::InputsHistory;
use super::mid_panels::ErsAnim;
use super::radar_panel::RadarAnim;
use super::simple_panels::{DeltaBarAnim, FlagsAnim, RadioAnim};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct RowAnimState {
    pub idx: f32,
    pub from: f32,
    pub to: f32,
    pub t0: f64,
    pub opacity: f32,
}

impl Default for RowAnimState {
    fn default() -> Self {
        Self {
            idx: 0.0,
            from: 0.0,
            to: 0.0,
            t0: 0.0,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct TableAnim {
    pub slots: HashMap<String, RowAnimState>,
    pub last_order: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DashAnim {
    pub lit: f32,
    pub lit_last_secs: f64,
    pub blink_since_s: Option<f64>,
    pub blink_suppressed: bool,
}

impl Default for DashAnim {
    fn default() -> Self {
        Self {
            lit: 0.0,
            lit_last_secs: 0.0,
            blink_since_s: None,
            blink_suppressed: false,
        }
    }
}

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let u = -2.0 * t + 2.0;
        1.0 - (u * u * u) * 0.5
    }
}

/// Advance row slot animations toward target indices. Returns true if still moving.
pub fn tick_table_anim(
    anim: &mut TableAnim,
    order: &[String],
    now: f64,
    slide_s: f32,
    snap_slots: f32,
) -> bool {
    if order != anim.last_order.as_slice() {
        anim.last_order = order.to_vec();
    }
    let active: std::collections::HashSet<&str> = order.iter().map(|s| s.as_str()).collect();
    anim.slots.retain(|k, _| active.contains(k.as_str()));

    let slide_s = if slide_s.is_finite() && slide_s > 0.05 {
        slide_s
    } else {
        0.24
    };
    let mut still = false;
    for (i, key) in order.iter().enumerate() {
        let target = i as f32;
        let st = anim.slots.entry(key.clone()).or_insert(RowAnimState {
            idx: target,
            from: target,
            to: target,
            t0: now,
            opacity: 1.0,
        });
        if (st.to - target).abs() > 0.01 {
            let jump = (st.idx - target).abs();
            if jump > snap_slots {
                st.idx = target;
                st.from = target;
                st.to = target;
                st.t0 = now;
            } else {
                st.from = st.idx;
                st.to = target;
                st.t0 = now;
            }
        }
        let t = ((now - st.t0) as f32 / slide_s).clamp(0.0, 1.0);
        if t >= 1.0 {
            st.idx = st.to;
        } else {
            st.idx = st.from + (st.to - st.from) * ease_in_out_cubic(t);
            still = true;
        }
        st.opacity = 1.0;
    }
    still
}

#[derive(Default)]
pub struct AnimStore {
    pub tables: HashMap<String, TableAnim>,
    pub dash: DashAnim,
    pub delta: DeltaBarAnim,
    pub flags: FlagsAnim,
    pub radio: RadioAnim,
    pub ers: ErsAnim,
    pub radar: RadarAnim,
    pub inputs: InputsHistory,
}

impl AnimStore {
    pub fn table_mut(&mut self, key: &str) -> &mut TableAnim {
        self.tables.entry(key.to_string()).or_default()
    }

    pub fn dash_mut(&mut self) -> &mut DashAnim {
        &mut self.dash
    }
}
