//! Pit service flag decoding for the pit board widget.
//!
//! Ports `overlay/pit_service.py` — iRacing `PitSvFlags` bits into
//! `{key, label, checked}` entries.

use serde::{Deserialize, Serialize};

/// iRacing PitSvFlags bits (irsdk).
pub const LF_TIRE: i32 = 0x0001;
pub const RF_TIRE: i32 = 0x0002;
pub const LR_TIRE: i32 = 0x0004;
pub const RR_TIRE: i32 = 0x0008;
pub const FUEL_FILL: i32 = 0x0010;
pub const TEAROFF: i32 = 0x0020;
pub const FAST_REPAIR: i32 = 0x0040;

const SERVICES: &[(&str, i32, &str)] = &[
    ("lf_tire", LF_TIRE, "LF tire"),
    ("rf_tire", RF_TIRE, "RF tire"),
    ("lr_tire", LR_TIRE, "LR tire"),
    ("rr_tire", RR_TIRE, "RR tire"),
    ("fuel", FUEL_FILL, "Fuel"),
    ("tearoff", TEAROFF, "Tearoff"),
    ("fast_repair", FAST_REPAIR, "Fast repair"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PitService {
    pub key: String,
    pub label: String,
    pub checked: bool,
}

/// Return pit services as `[{key, label, checked}, ...]`.
pub fn decode_flags(raw: i32) -> Vec<PitService> {
    SERVICES
        .iter()
        .map(|(key, bit, label)| PitService {
            key: (*key).to_string(),
            label: (*label).to_string(),
            checked: (raw & bit) != 0,
        })
        .collect()
}

/// True when any pit service bit is set.
#[allow(dead_code)] // used by IRSDK feed when live pit flags are wired
pub fn any_requested(raw: i32) -> bool {
    raw != 0
}
