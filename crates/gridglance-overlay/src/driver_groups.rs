//! Personal driver groups + event-result import (Python parity).

use serde_json::{json, Value};

pub const DRIVER_GROUP_ICONS: &[&str] = &[
    "league",
    "flag",
    "trophy",
    "shield",
    "crown",
    "bolt",
    // Negative / caution tags
    "skull",       // bad / dangerous driver
    "thumbs_down", // clearly negative
    "ban",         // banned / avoid
    "user_slash",  // blocked
    "biohazard",   // dirty / toxic racing
    "smog",        // dirty air metaphor
    "bomb",        // liability
    "angry",       // hot-headed
    "ghost",       // invisible / offline vibe
    "fire",        // dumpster-fire energy
    "poop",        // tongue-in-cheek dirty
];
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

/// Deduplicate member objects by case-insensitive name (keeps first aliases).
pub fn dedupe_members(raw: &[Value]) -> Vec<Value> {
    normalize_member_list(&json!(raw))
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

/// Parse a drivers JSON file → ordered unique display names.
///
/// Accepts:
/// - iRacing `event_result` / `session_results` payloads (deduped by `cust_id`, then name)
/// - a JSON array of strings or `{ "name": … }` objects
/// - `{ "drivers": […] }` / `{ "members": […] }` / `{ "pro_drivers": […] }`
pub fn parse_event_result_names(text: &str) -> anyhow::Result<Vec<String>> {
    let payload: Value = serde_json::from_str(text)?;
    let mut out = Vec::new();
    let mut seen_names = std::collections::HashSet::new();
    let mut seen_ids = std::collections::HashSet::new();

    let mut push_name = |name: &str, cust_id: Option<i64>| {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(id) = cust_id {
            if !seen_ids.insert(id) {
                return;
            }
        }
        let key = name.to_ascii_lowercase();
        if !seen_names.insert(key) {
            return;
        }
        out.push(name.to_string());
    };

    if let Some(arr) = payload.as_array() {
        push_name_list(arr, &mut push_name);
        return Ok(out);
    }

    let data = if payload.get("data").and_then(|d| d.as_object()).is_some() {
        payload.get("data").cloned().unwrap_or(payload.clone())
    } else {
        payload.clone()
    };

    for key in ["drivers", "members", "pro_drivers", "names"] {
        if let Some(arr) = data.get(key).and_then(|v| v.as_array()) {
            push_name_list(arr, &mut push_name);
        }
    }

    let sessions = data
        .get("session_results")
        .or_else(|| data.get("sessionResults"))
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
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
            let cust_id = row
                .get("cust_id")
                .or_else(|| row.get("custId"))
                .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|u| u as i64)));
            let name = row
                .get("display_name")
                .or_else(|| row.get("displayName"))
                .or_else(|| row.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            push_name(name, cust_id);
        }
    }

    if out.is_empty() {
        anyhow::bail!("no driver names found in JSON");
    }
    Ok(out)
}

fn push_name_list(arr: &[Value], push_name: &mut impl FnMut(&str, Option<i64>)) {
    for item in arr {
        if let Some(s) = item.as_str() {
            push_name(s, None);
            continue;
        }
        let Some(obj) = item.as_object() else {
            continue;
        };
        let cust_id = obj
            .get("cust_id")
            .or_else(|| obj.get("custId"))
            .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|u| u as i64)));
        let name = obj
            .get("display_name")
            .or_else(|| obj.get("displayName"))
            .or_else(|| obj.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("");
        push_name(name, cust_id);
    }
}

/// Merge display names into a member list, skipping any name or alias already present.
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
                    occupied.insert(s.trim().to_ascii_lowercase());
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

/// Comma-separated UI member string → deduped member objects.
pub fn members_from_csv(csv: &str) -> Vec<Value> {
    let raw: Vec<Value> = csv
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| json!({ "name": s, "aliases": [] }))
        .collect();
    normalize_member_list(&json!(raw))
}

/// Deduped comma-separated names for the Drivers UI field.
pub fn members_to_csv(members: &[Value]) -> String {
    normalize_member_list(&json!(members))
        .iter()
        .filter_map(|e| e.get("name").and_then(|n| n.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_result_dedupes_same_driver_across_sessions() {
        let json = r#"{
          "data": {
            "session_results": [
              {"results": [
                {"cust_id": 1, "display_name": "Ada Lovelace", "ai": false},
                {"cust_id": 2, "display_name": "Grace Hopper", "ai": false}
              ]},
              {"results": [
                {"cust_id": 1, "display_name": "Ada Lovelace", "ai": false},
                {"cust_id": 3, "display_name": "Alan Turing", "ai": false}
              ]}
            ]
          }
        }"#;
        let names = parse_event_result_names(json).unwrap();
        assert_eq!(names, vec!["Ada Lovelace", "Grace Hopper", "Alan Turing"]);
    }

    #[test]
    fn merge_skips_existing_names_case_insensitively() {
        let existing = vec![json!({ "name": "Ada Lovelace", "aliases": ["Ada"] })];
        let names = vec!["ada lovelace".into(), "ADA".into(), "Grace Hopper".into()];
        let (merged, added, skipped) = merge_names_into_members(&existing, &names);
        assert_eq!(added, 1);
        assert_eq!(skipped, 2);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn members_from_csv_drops_duplicate_names() {
        let members = members_from_csv("Ada, Grace, ada, Bob");
        assert_eq!(members_to_csv(&members), "Ada, Grace, Bob");
    }
}
