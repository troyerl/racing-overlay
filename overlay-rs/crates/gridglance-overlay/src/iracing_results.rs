//! Optional authenticated iRacing results lookup for registration split info.

use serde_json::Value;
use sha2::{Digest, Sha256};

const AUTH_URL: &str = "https://members-ng.iracing.com/auth";
const RESULTS_URL: &str = "https://members-ng.iracing.com/data/results/get";

/// Return `(1-based split number, total splits)`.
///
/// Credentials are optional environment variables, matching the Python path:
/// `IRACING_USERNAME`/`IRACING_EMAIL` and `IRACING_PASSWORD`.
pub fn split_for_subsession(subsession_id: i32) -> Option<(i32, i32)> {
    if subsession_id <= 0 {
        return None;
    }
    let email = std::env::var("IRACING_USERNAME")
        .or_else(|_| std::env::var("IRACING_EMAIL"))
        .ok()?;
    let password = std::env::var("IRACING_PASSWORD").ok()?;
    if email.trim().is_empty() || password.is_empty() {
        return None;
    }

    let normalized = email.trim().to_ascii_lowercase();
    let digest = Sha256::digest(format!("{password}{normalized}").as_bytes());
    let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let agent = ureq::AgentBuilder::new()
        .user_agent("GridGlance/1.0")
        .build();
    let auth: Value = agent
        .post(AUTH_URL)
        .send_form(&[("email", email.trim()), ("password", hash.as_str())])
        .ok()?
        .into_json()
        .ok()?;
    if auth.get("authcode").is_none() {
        return None;
    }

    let payload: Value = agent
        .get(RESULTS_URL)
        .query("subsession_id", &subsession_id.to_string())
        .call()
        .ok()?
        .into_json()
        .ok()?;
    let data = if let Some(link) = payload.get("link").and_then(Value::as_str) {
        agent.get(link).call().ok()?.into_json().ok()?
    } else {
        payload
    };
    split_from_results(&data, subsession_id)
}

fn split_from_results(data: &Value, subsession_id: i32) -> Option<(i32, i32)> {
    let splits = data
        .get("session_splits")
        .or_else(|| data.get("sessionSplits"))
        .and_then(Value::as_array);
    let Some(splits) = splits.filter(|s| !s.is_empty()) else {
        let own = data
            .get("subsession_id")
            .or_else(|| data.get("subsessionId"))
            .and_then(Value::as_i64)?;
        return (own == subsession_id as i64).then_some((1, 1));
    };

    let mut ranked: Vec<(i32, i32)> = splits
        .iter()
        .filter_map(|entry| {
            let sid = entry
                .get("subsession_id")
                .or_else(|| entry.get("subsessionId"))
                .and_then(Value::as_i64)? as i32;
            let sof = entry
                .get("event_strength_of_field")
                .or_else(|| entry.get("eventStrengthOfField"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32;
            Some((sid, sof))
        })
        .collect();
    ranked.sort_by_key(|(sid, sof)| (std::cmp::Reverse(*sof), *sid));
    let total = ranked.len() as i32;
    ranked
        .iter()
        .position(|(sid, _)| *sid == subsession_id)
        .map(|i| (i as i32 + 1, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ranks_split_and_returns_total() {
        let data = json!({
            "session_splits": [
                {"subsession_id": 100, "event_strength_of_field": 1200},
                {"subsession_id": 200, "event_strength_of_field": 2500},
                {"subsession_id": 300, "event_strength_of_field": 1800}
            ]
        });
        assert_eq!(split_from_results(&data, 200), Some((1, 3)));
        assert_eq!(split_from_results(&data, 300), Some((2, 3)));
        assert_eq!(split_from_results(&data, 100), Some((3, 3)));
    }
}
