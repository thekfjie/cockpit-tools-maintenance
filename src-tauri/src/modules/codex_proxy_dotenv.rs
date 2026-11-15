//! Recoverable ownership of the proxy portion of CODEX_HOME/.env (Windows).
//! Never move CODEX_HOME or copy auth/history. Recovery material is encrypted,
//! bounded, and kept outside the client profile; no plaintext .env.bak is made.
use super::{atomic_write, process::CodexLaunchProxy, secure_account_storage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}, sync::Mutex};

const ERROR: &str = "PROXY_ENV_STORAGE";
const CONFLICT: &str = "PROXY_ENV_CHANGED";
const BEGIN: &str = "# >>> Cockpit account proxy (managed)";
const END: &str = "# <<< Cockpit account proxy (managed)";
const MAX_DOCUMENT: usize = 128 * 1024;
static WRITES: Mutex<()> = Mutex::new(());

#[derive(Clone, Default, Serialize, Deserialize)]
struct State {
    version: u32,
    #[serde(default)]
    last_account: Option<String>,
    defaults: BTreeMap<String, Option<String>>,
    active: Option<Owned>,
    pending: Option<Pending>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Owned { account: String, original: Option<String>, block: String }
#[derive(Clone, Serialize, Deserialize)]
struct Pending { before: Option<String>, after: Option<String> }

fn read_document(path: &Path) -> Result<Option<String>, String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= MAX_DOCUMENT as u64 => (),
        _ => return Err(ERROR.into()),
    }
    fs::read_to_string(path).map(Some).map_err(|_| ERROR.into())
}

fn key(line: &str) -> Option<&str> {
    let line = line.trim_start_matches('\u{feff}').trim_start();
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    let (key, _) = line.split_once('=')?;
    let key = key.trim();
    ["http_proxy", "https_proxy", "all_proxy", "no_proxy"]
        .iter().any(|k| key.eq_ignore_ascii_case(k)).then_some(key)
}

// Group multiline quoted values before looking for proxy assignments. A line
// inside an unrelated quoted value must never be mistaken for an assignment.
fn parts(document: &str) -> Result<Vec<(&str, bool)>, String> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut is_proxy = false;
    let mut offset = 0;
    for line in document.split_inclusive('\n') {
        let fresh = quote.is_none();
        if fresh { start = offset; is_proxy = key(line).is_some(); }
        let value = if fresh {
            line.trim_start().strip_prefix("export ").unwrap_or(line.trim_start())
                .split_once('=').map(|(_, value)| value).unwrap_or("")
        } else { line };
        if fresh && !value.trim_start().starts_with(['\'', '"']) {
            if value.trim_end().ends_with('\\') { return Err("PROXY_ENV_UNSUPPORTED".into()); }
            offset += line.len();
            result.push((&document[start..offset], is_proxy));
            continue;
        }
        for ch in value.chars() {
            if escaped { escaped = false; continue; }
            if ch == '\\' && quote != Some('\'') { escaped = true; continue; }
            match quote {
                Some(q) if ch == q => quote = None,
                None if ch == '#' => break,
                None if ch == '\'' || ch == '"' => quote = Some(ch),
                _ => (),
            }
        }
        offset += line.len();
        if quote.is_none() { result.push((&document[start..offset], is_proxy)); }
    }
    if quote.is_some() || escaped { return Err("PROXY_ENV_UNSUPPORTED".into()); }
    Ok(result)
}

fn merge_defaults(current: Option<&str>, saved: Option<&str>) -> Result<Option<String>, String> {
    let current_proxy: String = parts(current.unwrap_or(""))?.into_iter().filter(|(_, proxy)| *proxy).map(|(text, _)| text).collect();
    let saved_proxy: String = parts(saved.unwrap_or(""))?.into_iter().filter(|(_, proxy)| *proxy).map(|(text, _)| text).collect();
    if current_proxy == saved_proxy {
        return Ok(if current.unwrap_or("").is_empty() { saved.map(|_| String::new()) } else { current.map(str::to_owned) });
    }
    let mut text: String = parts(current.unwrap_or(""))?.into_iter()
        .filter(|(_, proxy)| !proxy).map(|(text, _)| text).collect();
    let proxy: String = parts(saved.unwrap_or(""))?.into_iter()
        .filter(|(_, proxy)| *proxy).map(|(text, _)| text).collect();
    if !text.is_empty() && !text.ends_with('\n') && !proxy.is_empty() { text.push('\n'); }
    text.push_str(&proxy);
    Ok(if text.is_empty() && saved.is_none() { None } else { Some(text) })
}

fn unproject(current: Option<&str>, active: &Owned) -> Result<Option<String>, String> {
    let current = current.ok_or(CONFLICT)?;
    if current.matches(&active.block).count() != 1 { return Err(CONFLICT.into()); }
    let text = current.replacen(&active.block, "", 1);
    // Preserve absent versus empty, plus edits to unrelated settings made while managed.
    Ok(if text.is_empty() && active.original.is_none() { None } else { Some(text) })
}

fn transition(mut state: State, current: Option<String>, account: &str, proxy: &CodexLaunchProxy)
    -> Result<(State, Option<String>), String> {
    let original = if let Some(active) = state.active.take() {
        let original = unproject(current.as_deref(), &active)?;
        state.defaults.insert(active.account, original.clone());
        original
    } else {
        if current.as_deref().is_some_and(|s| s.contains(BEGIN) || s.contains(END)) {
            return Err(CONFLICT.into());
        }
        current.clone()
    };
    if let Some(previous) = state.last_account.as_ref() {
        state.defaults.insert(previous.clone(), original.clone());
    }
    let original = if state.last_account.as_deref() == Some(account) { original }
    else if let Some(saved) = state.defaults.get(account) {
        merge_defaults(original.as_deref(), saved.as_deref())?
    } else { original };
    parts(original.as_deref().unwrap_or(""))?;
    if state.defaults.len() >= 128 && !state.defaults.contains_key(account) { return Err(ERROR.into()); }
    state.defaults.insert(account.to_owned(), original.clone());
    let pairs = proxy.dotenv_overrides()?;
    let after = if pairs.is_empty() {
        original
    } else {
        let mut block = String::from("\n");
        block.push_str(BEGIN); block.push('\n');
        for (name, value) in pairs {
            // Values are generated local endpoints or explicit empty/direct values.
            if value.contains(['\n', '\r', '\'', '"', '$', '\\']) { return Err(ERROR.into()); }
            block.push_str(&format!("{name}='{value}'\n"));
        }
        block.push_str(END); block.push('\n');
        let projected = format!("{}{block}", original.as_deref().unwrap_or(""));
        if projected.len() > MAX_DOCUMENT { return Err(ERROR.into()); }
        state.active = Some(Owned { account: account.into(), original, block });
        Some(projected)
    };
    state.last_account = Some(account.to_owned());
    state.version = 1;
    state.pending = Some(Pending { before: current, after: after.clone() });
    Ok((state, after))
}

fn state_path(home: &Path) -> Result<PathBuf, String> {
    let home = fs::canonicalize(home).map_err(|_| ERROR)?;
    let identity = home.to_string_lossy().replace('\\', "/").to_lowercase();
    let hash = format!("{:x}", Sha256::digest(identity.as_bytes()));
    Ok(super::account::get_data_dir().map_err(|_| ERROR)?.join("codex-client-network").join(format!("{hash}.json")))
}
fn read_state(path: &Path) -> Result<State, String> {
    let Some(raw) = read_document_with_limit(path, 32 * 1024 * 1024)? else { return Ok(State::default()); };
    let state: State = secure_account_storage::deserialize_encrypted_snapshot(&raw).map_err(|_| ERROR)?;
    if state.version != 1 || state.defaults.len() > 128 { return Err(ERROR.into()); }
    Ok(state)
}
fn read_document_with_limit(path: &Path, limit: u64) -> Result<Option<String>, String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= limit => (),
        _ => return Err(ERROR.into()),
    }
    fs::read_to_string(path).map(Some).map_err(|_| ERROR.into())
}
fn write_state(path: &Path, state: &State) -> Result<(), String> {
    let raw = secure_account_storage::serialize_account_file("codex-client-network", state).map_err(|_| ERROR)?;
    atomic_write::write_secret_string_atomic(path, &raw).map_err(|_| ERROR.into())
}

/// Call under the existing stopped-profile mutation lease, before the GUI or
/// app-server can start. Do not restore on exit: the same routing must survive reboot.
pub fn apply(home: &Path, account: &str, proxy: &CodexLaunchProxy) -> Result<(), String> {
    if !cfg!(windows) { return Ok(()); }
    apply_inner(home, account, proxy)
}
fn apply_inner(home: &Path, account: &str, proxy: &CodexLaunchProxy) -> Result<(), String> {
    let _write = WRITES.lock().map_err(|_| ERROR)?;
    let path = state_path(home)?;
    let env = home.join(".env");
    let mut current = read_document(&env)?;
    let mut state = read_state(&path)?;
    if let Some(pending) = state.pending.as_ref() {
        if current == pending.before {
            if !atomic_write::replace_secret_string_if_matches(&env, current.as_deref(), pending.after.as_deref()).map_err(|_| ERROR)? {
                return Err(CONFLICT.into());
            }
            current = pending.after.clone();
        } else if current != pending.after { return Err(CONFLICT.into()); }
        state.pending = None;
        write_state(&path, &state)?;
    }
    // With no takeover and no explicit route, leave the original file untouched.
    if state.active.is_none() && proxy.dotenv_overrides()?.is_empty() {
        if !path.exists() { return Ok(()); }
    }
    let (mut next, after) = transition(state, current.clone(), account, proxy)?;
    // Write-ahead journal makes a crash between .env and state writes recoverable.
    write_state(&path, &next)?;
    if !atomic_write::replace_secret_string_if_matches(&env, current.as_deref(), after.as_deref()).map_err(|_| ERROR)? {
        return Err(CONFLICT.into());
    }
    next.pending = None;
    write_state(&path, &next)
}

#[cfg(test)]
#[path = "codex_proxy_dotenv_tests.rs"]
mod tests;

pub fn verify(home: &Path, account: &str, proxy: &CodexLaunchProxy) -> Result<(), String> {
    if proxy.dotenv_overrides()?.is_empty() { return Ok(()); }
    let _write = WRITES.lock().map_err(|_| ERROR)?;
    let state = read_state(&state_path(home)?)?;
    if state.pending.is_some() { return Err(CONFLICT.into()); }
    let active = state.active.as_ref().filter(|a| a.account == account).ok_or(CONFLICT)?;
    let current = read_document(&home.join(".env"))?;
    unproject(current.as_deref(), active)?;
    let (_, expected) = transition(state, current.clone(), account, proxy)?;
    if current != expected { return Err(CONFLICT.into()); }
    Ok(())
}
