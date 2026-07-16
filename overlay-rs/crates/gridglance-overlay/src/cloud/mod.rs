//! Shared track maps via MongoDB Atlas (Python `track_store` port).

mod dotenv;

use mongodb::bson::{self, doc, Bson, Document};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use dotenv::load_dotenv;

const DB_NAME: &str = "gridglance";
const TRACKS_COLL: &str = "tracks";
const SETTINGS_COLL: &str = "app_settings";
const SETTINGS_DOC_ID: &str = "global";
const APP_SETTINGS_CACHE: &str = "_app_settings.json";

/// Embedded read-only Atlas URI (same as Python `_READ_URI_DEFAULT`).
const READ_URI_DEFAULT: &str =
    "mongodb+srv://GridGlanceUser:3w69ejWh1WGKenQa@gridglance.dguyept.mongodb.net/?appName=GridGlance";

static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("gg-mongo")
        .build()
        .expect("mongo runtime")
});

static WRITE_CLIENT: Mutex<Option<mongodb::Client>> = Mutex::new(None);
static READ_CLIENT: Mutex<Option<mongodb::Client>> = Mutex::new(None);

fn write_uri() -> String {
    std::env::var("GRIDGLANCE_MONGODB_URI")
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn read_uri() -> String {
    for key in ["GRIDGLANCE_MONGODB_READ_URI", "GRIDGLANCE_MONGODB_URI"] {
        if let Ok(v) = std::env::var(key) {
            let t = v.trim().to_string();
            if !t.is_empty() {
                return t;
            }
        }
    }
    READ_URI_DEFAULT.trim().to_string()
}

/// True when author/dev write credential is present.
pub fn can_write() -> bool {
    !write_uri().is_empty()
}

pub fn read_available() -> bool {
    !read_uri().is_empty() || can_write()
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    RUNTIME.block_on(f)
}

async fn client(write: bool) -> anyhow::Result<mongodb::Client> {
    let uri = if write {
        let u = write_uri();
        if u.is_empty() {
            anyhow::bail!("no write URI");
        }
        u
    } else {
        let u = read_uri();
        if u.is_empty() {
            anyhow::bail!("no read URI");
        }
        u
    };
    let slot = if write { &WRITE_CLIENT } else { &READ_CLIENT };
    {
        let guard = slot.lock().unwrap();
        if let Some(c) = guard.as_ref() {
            return Ok(c.clone());
        }
    }
    let c = mongodb::Client::with_uri_str(&uri).await?;
    let mut guard = slot.lock().unwrap();
    *guard = Some(c.clone());
    Ok(c)
}

async fn collection(name: &str, write: bool) -> anyhow::Result<mongodb::Collection<Document>> {
    let c = client(write).await?;
    Ok(c.database(DB_NAME).collection(name))
}

fn value_to_bson(v: &Value) -> Bson {
    bson::to_bson(v).unwrap_or(Bson::Null)
}

fn doc_to_value(d: Document) -> Value {
    bson::from_bson(Bson::Document(d)).unwrap_or(Value::Null)
}

pub fn cloud_track_exists(track_id: &Value) -> Option<bool> {
    if !can_write() {
        return None;
    }
    block_on(async {
        let col = collection(TRACKS_COLL, true).await.ok()?;
        let filter = track_id_filter(track_id);
        let n = col.count_documents(filter, None).await.ok()?;
        Some(n > 0)
    })
}

fn track_id_filter(track_id: &Value) -> Document {
    let mut ors = vec![];
    if let Some(i) = track_id.as_i64() {
        ors.push(doc! { "track_id": i });
        ors.push(doc! { "track_id": i.to_string() });
    } else if let Some(s) = track_id.as_str() {
        ors.push(doc! { "track_id": s });
        if let Ok(i) = s.parse::<i64>() {
            ors.push(doc! { "track_id": i });
        }
    } else if let Some(u) = track_id.as_u64() {
        ors.push(doc! { "track_id": u as i64 });
    }
    if ors.is_empty() {
        doc! { "track_id": track_id.to_string() }
    } else {
        doc! { "$or": ors }
    }
}

pub fn fetch_track(track_id: &Value) -> anyhow::Result<Option<Value>> {
    block_on(async {
        let col = collection(TRACKS_COLL, false).await?;
        let filter = track_id_filter(track_id);
        let doc = col.find_one(filter, None).await?;
        Ok(doc.map(doc_to_value))
    })
}

pub fn upload_doc(doc: &Value) -> anyhow::Result<()> {
    if !can_write() {
        anyhow::bail!("no write credential");
    }
    let tid = doc
        .get("track_id")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing track_id"))?;
    block_on(async {
        let col = collection(TRACKS_COLL, true).await?;
        let bson_doc = match value_to_bson(doc) {
            Bson::Document(d) => d,
            _ => anyhow::bail!("track doc must be an object"),
        };
        let opts = mongodb::options::ReplaceOptions::builder()
            .upsert(true)
            .build();
        col.replace_one(track_id_filter(&tid), bson_doc, opts)
            .await?;
        Ok(())
    })
}

pub fn upload_local(tracks_dir: &Path, tid: &Value) -> anyhow::Result<()> {
    let path = track_file_path(tracks_dir, tid);
    let text = fs::read_to_string(&path)?;
    let doc: Value = serde_json::from_str(&text)?;
    upload_doc(&doc)
}

/// Refresh *already-local* tracks from cloud; return how many files changed.
///
/// Does not bulk-download the library — missing tracks are fetched on demand
/// via [`fetch_track_async`] when a session needs them.
pub fn sync_down(tracks_dir: &Path) -> anyhow::Result<usize> {
    if !read_available() {
        return Ok(0);
    }
    let Ok(entries) = fs::read_dir(tracks_dir) else {
        return Ok(0);
    };
    let mut changed = 0usize;
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some(APP_SETTINGS_CACHE) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(local) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(tid) = local.get("track_id").cloned() else {
            continue;
        };
        // Only refresh cloud-sourced cache files (have updated_at), matching Python.
        let local_ts = local
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if local_ts.is_empty() {
            continue;
        }
        match fetch_track(&tid) {
            Ok(Some(remote)) => {
                let remote_ts = remote
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if remote_ts.is_empty() || remote_ts == local_ts {
                    continue;
                }
                let out = track_file_path(tracks_dir, &tid);
                write_json_atomic(&out, &remote)?;
                changed += 1;
            }
            Ok(None) => {}
            Err(e) => eprintln!("[gridglance] track refresh {}: {e}", tid),
        }
    }
    Ok(changed)
}

static FETCH_INFLIGHT: Lazy<Mutex<std::collections::HashSet<i64>>> =
    Lazy::new(|| Mutex::new(std::collections::HashSet::new()));
static FETCH_TRIED: Lazy<Mutex<std::collections::HashSet<i64>>> =
    Lazy::new(|| Mutex::new(std::collections::HashSet::new()));
static FETCH_READY: Lazy<Mutex<std::collections::HashSet<i64>>> =
    Lazy::new(|| Mutex::new(std::collections::HashSet::new()));

/// True once if a background [`fetch_track_async`] just wrote this track to disk.
pub fn take_fetch_ready(tid: i64) -> bool {
    FETCH_READY
        .lock()
        .map(|mut s| s.remove(&tid))
        .unwrap_or(false)
}

/// Download one track into the local cache (deduped; never panics).
pub fn fetch_track_async(tid: i64) {
    if !read_available() || tid <= 0 {
        return;
    }
    {
        let Ok(mut inflight) = FETCH_INFLIGHT.lock() else {
            return;
        };
        let Ok(tried) = FETCH_TRIED.lock() else {
            return;
        };
        if inflight.contains(&tid) || tried.contains(&tid) {
            return;
        }
        inflight.insert(tid);
    }
    std::thread::spawn(move || {
        let tid_val = json!(tid);
        let result = fetch_track(&tid_val);
        match result {
            Ok(Some(doc)) => {
                let dir = crate::paths::tracks_dir();
                let path = track_file_path(&dir, doc.get("track_id").unwrap_or(&tid_val));
                match write_json_atomic(&path, &doc) {
                    Ok(()) => {
                        if let Ok(mut ready) = FETCH_READY.lock() {
                            ready.insert(tid);
                        }
                        eprintln!("[gridglance] fetched track {tid} from cloud");
                    }
                    Err(e) => eprintln!("[gridglance] write track {tid}: {e}"),
                }
                if let Ok(mut tried) = FETCH_TRIED.lock() {
                    tried.insert(tid);
                }
            }
            Ok(None) => {
                if let Ok(mut tried) = FETCH_TRIED.lock() {
                    tried.insert(tid);
                }
                eprintln!("[gridglance] track {tid} not in cloud library");
            }
            Err(e) => {
                // Allow retry later on transient errors.
                eprintln!("[gridglance] fetch track {tid}: {e}");
            }
        }
        if let Ok(mut inflight) = FETCH_INFLIGHT.lock() {
            inflight.remove(&tid);
        }
    });
}

pub fn fetch_app_settings() -> anyhow::Result<Value> {
    let cloud = block_on(async {
        let col = collection(SETTINGS_COLL, false).await?;
        let doc = col.find_one(doc! { "_id": SETTINGS_DOC_ID }, None).await?;
        Ok::<_, anyhow::Error>(doc.map(|mut d| {
            d.remove("_id");
            doc_to_value(d)
        }))
    })?;
    if let Some(v) = cloud {
        let _ = cache_app_settings(&v);
        return Ok(v);
    }
    Ok(load_app_settings_cache().unwrap_or_else(|| json!({})))
}

pub fn save_app_settings(patch: &Value) -> anyhow::Result<Value> {
    if !can_write() {
        anyhow::bail!("no write credential");
    }
    let mut cur = fetch_app_settings().unwrap_or_else(|_| json!({}));
    if let (Some(base), Some(p)) = (cur.as_object_mut(), patch.as_object()) {
        for (k, v) in p {
            base.insert(k.clone(), v.clone());
        }
    }
    block_on(async {
        let col = collection(SETTINGS_COLL, true).await?;
        let mut doc = match value_to_bson(&cur) {
            Bson::Document(d) => d,
            _ => anyhow::bail!("settings must be object"),
        };
        doc.insert("_id", SETTINGS_DOC_ID);
        let opts = mongodb::options::ReplaceOptions::builder()
            .upsert(true)
            .build();
        col.replace_one(doc! { "_id": SETTINGS_DOC_ID }, doc, opts)
            .await?;
        Ok::<_, anyhow::Error>(())
    })?;
    let _ = cache_app_settings(&cur);
    Ok(cur)
}

fn cache_path() -> PathBuf {
    crate::paths::tracks_dir().join(APP_SETTINGS_CACHE)
}

fn cache_app_settings(v: &Value) -> anyhow::Result<()> {
    write_json_atomic(&cache_path(), v)
}

pub fn load_app_settings_cache() -> Option<Value> {
    let text = fs::read_to_string(cache_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn resolve_track_id(tracks_dir: &Path, tid: &Value) -> Option<Value> {
    let path = track_file_path(tracks_dir, tid);
    if path.is_file() {
        return Some(tid.clone());
    }
    let want = tid_as_i64(tid)?;
    let entries = fs::read_dir(tracks_dir).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        if p.file_name().and_then(|n| n.to_str()) == Some(APP_SETTINGS_CACHE) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&p) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if tid_as_i64(doc.get("track_id")?) == Some(want) {
            return doc.get("track_id").cloned();
        }
        if let Some(arr) = doc.get("alias_track_ids").and_then(|a| a.as_array()) {
            if arr.iter().any(|a| tid_as_i64(a) == Some(want)) {
                return doc.get("track_id").cloned();
            }
        }
    }
    Some(tid.clone())
}

pub fn tid_as_i64(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_u64().map(|u| u as i64))
        .or_else(|| v.as_str()?.parse().ok())
}

pub fn track_file_path(tracks_dir: &Path, tid: &Value) -> PathBuf {
    let name = match tid {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => tid.to_string(),
    };
    tracks_dir.join(format!("{name}.json"))
}

pub fn write_json_atomic(path: &Path, doc: &Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(doc)? + "\n")?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Background sync-down; logs errors, never panics.
pub fn sync_down_async(tracks_dir: PathBuf) {
    std::thread::spawn(move || match sync_down(&tracks_dir) {
        Ok(n) if n > 0 => eprintln!("[gridglance] refreshed {n} local tracks from cloud"),
        Ok(_) => {}
        Err(e) => eprintln!("[gridglance] track sync: {e}"),
    });
}

/// Warm the local app-settings cache (pro drivers) on launch.
pub fn fetch_app_settings_async() {
    std::thread::spawn(|| match fetch_app_settings() {
        Ok(_) => {}
        Err(e) => eprintln!("[gridglance] app settings: {e}"),
    });
}

pub fn is_pro_driver(name: &str, settings: &Value) -> bool {
    let Some(arr) = settings.get("pro_drivers").and_then(|a| a.as_array()) else {
        return false;
    };
    let needle = name.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return false;
    }
    for entry in arr {
        let primary = entry
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if primary == needle {
            return true;
        }
        if let Some(aliases) = entry.get("aliases").and_then(|a| a.as_array()) {
            for a in aliases {
                if a.as_str()
                    .map(|s| s.trim().to_ascii_lowercase() == needle)
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }
    }
    false
}
