//! Personal driver groups + event-result import (Python parity).

use serde_json::{json, Value};

pub const DRIVER_GROUP_ICONS: &[&str] = &["league", "flag", "trophy", "shield", "crown", "bolt"];
const DEFAULT_COLOR: &str = "#5bb8ff";
const DEFAULT_ICON: &str = "league";

#[derive(Debug, Clone)]
pub struct DriverGroupMatch {
    pub icon: String,
    pub color: String,
}

pub fn normalize_driver_groups(raw: &Value) -> Vec<Value> {
    let Some(arr) = raw.as_array() else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in arr {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let name = obj
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        let mut icon = obj
            .get("icon")
            .and_then(|i| i.as_str())
            .unwrap_or(DEFAULT_ICON)
            .trim()
            .to_string();
        if icon.is_empty() || !DRIVER_GROUP_ICONS.contains(&icon.as_str()) {
            icon = DEFAULT_ICON.into();
        }
        let mut color = obj
            .get("color")
            .and_then(|c| c.as_str())
            .unwrap_or(DEFAULT_COLOR)
            .trim()
            .to_string();
        if !color.starts_with('#') {
            color = DEFAULT_COLOR.into();
        }
        let members = normalize_member_list(obj.get("members").unwrap_or(&json!([])));
        out.push(json!({
            "name": name,
            "icon": icon,
            "color": color,
            "members": members,
        }));
    }
    out
}

fn normalize_member_list(raw: &Value) -> Vec<Value> {
    let Some(arr) = raw.as_array() else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in arr {
        let (name, aliases) = if let Some(s) = item.as_str() {
            (s.trim().to_string(), vec![])
        } else if let Some(obj) = item.as_object() {
            let name = obj
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let aliases = obj
                .get("aliases")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (name, aliases)
        } else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        out.push(json!({ "name": name, "aliases": aliases }));
    }
    out
}

pub fn driver_group_for_name(user_name: &str, groups: &Value) -> Option<DriverGroupMatch> {
    let needle = user_name.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }
    for group in normalize_driver_groups(groups) {
        let members = group.get("members").and_then(|m| m.as_array())?;
        for entry in members {
            let name = entry
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if name == needle {
                return Some(match_from(&group));
            }
            if let Some(aliases) = entry.get("aliases").and_then(|a| a.as_array()) {
                for a in aliases {
                    if a.as_str()
                        .map(|s| s.trim().to_ascii_lowercase() == needle)
                        .unwrap_or(false)
                    {
                        return Some(match_from(&group));
                    }
                }
            }
        }
    }
    None
}

fn match_from(group: &Value) -> DriverGroupMatch {
    DriverGroupMatch {
        icon: group
            .get("icon")
            .and_then(|i| i.as_str())
            .unwrap_or(DEFAULT_ICON)
            .to_string(),
        color: group
            .get("color")
            .and_then(|c| c.as_str())
            .unwrap_or(DEFAULT_COLOR)
            .to_string(),
    }
}

/// Parse iRacing event_result JSON → ordered unique display names.
pub fn parse_event_result_names(text: &str) -> anyhow::Result<Vec<String>> {
    let payload: Value = serde_json::from_str(text)?;
    let data = if payload.get("data").and_then(|d| d.as_object()).is_some() {
        payload.get("data").cloned().unwrap_or(payload)
    } else {
        payload
    };
    let sessions = data
        .get("session_results")
        .or_else(|| data.get("sessionResults"))
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for sess in sessions {
        let rows = sess
            .get("results")
            .or_else(|| sess.get("Results"))
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        for row in rows {
            if row.get("ai").and_then(|a| a.as_bool()) == Some(true) {
                continue;
            }
            let name = row
                .get("display_name")
                .or_else(|| row.get("displayName"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let key = name.to_ascii_lowercase();
            if !seen.insert(key) {
                continue;
            }
            out.push(name);
        }
    }
    Ok(out)
}

pub fn merge_names_into_members(
    existing: &[Value],
    names: &[String],
) -> (Vec<Value>, usize, usize) {
    let mut merged = normalize_member_list(&json!(existing));
    let mut occupied = std::collections::HashSet::new();
    for entry in &merged {
        if let Some(n) = entry.get("name").and_then(|n| n.as_str()) {
            occupied.insert(n.to_ascii_lowercase());
        }
        if let Some(aliases) = entry.get("aliases").and_then(|a| a.as_array()) {
            for a in aliases {
                if let Some(s) = a.as_str() {
                    occupied.insert(s.to_ascii_lowercase());
                }
            }
        }
    }
    let mut added = 0usize;
    let mut skipped = 0usize;
    for raw in names {
        let name = raw.trim();
        if name.is_empty() {
            skipped += 1;
            continue;
        }
        let key = name.to_ascii_lowercase();
        if occupied.contains(&key) {
            skipped += 1;
            continue;
        }
        occupied.insert(key);
        merged.push(json!({ "name": name, "aliases": [] }));
        added += 1;
    }
    (merged, added, skipped)
}
