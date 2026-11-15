//! Opt-in Windows desktop entry. Reuse the ordinary instance launch transaction,
//! registered-package identity, profile directory and existing progress UI.
use crate::modules;
use serde::{Deserialize, Serialize};
use std::{path::{Path, PathBuf}, sync::{Mutex, atomic::{AtomicBool, Ordering}}};
use tauri::{Emitter, Manager};

const ARG: &str = "--codex-desktop-launch";
const ERROR: &str = "PROXY_SHORTCUT_STORAGE";
static ACTIVE: AtomicBool = AtomicBool::new(false);
static FAILURE: Mutex<Option<String>> = Mutex::new(None);

fn launch_requested(args: &[String]) -> bool { args.len() == 2 && args[1] == ARG }
struct LaunchLease;
impl Drop for LaunchLease { fn drop(&mut self) { ACTIVE.store(false, Ordering::Release); } }

async fn launch_last(app: tauri::AppHandle) -> Result<(), String> {
    if ACTIVE.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() { return Err("PROXY_LAUNCH_BUSY".into()); }
    let _lease = LaunchLease;
    let settings = modules::codex_instance::load_default_settings()?;
    if settings.launch_mode == crate::models::InstanceLaunchMode::Cli { return Err("PROXY_DESKTOP_CLI".into()); }
    let target = super::codex_instance::resolve_codex_instance_start_target(super::codex_instance::DEFAULT_INSTANCE_ID)?;
    let profile_lease = modules::codex_account::try_acquire_profile_mutation_lease(&target.user_data_dir, "desktop-entry")?;
    let proxy = modules::codex_instance::resolve_egress_proxy_for_bind_account(target.bind_account_id.as_deref()).await?;
    if let Err(error) = modules::codex_proxy_launch_guard::preflight(&proxy).await {
        // Only the selected default instance is eligible for stopping.
        let pid = modules::process::resolve_codex_pid(settings.last_pid, None);
        if let Some(pid) = pid { modules::process::close_pid(pid, 5).map_err(|_| "PROXY_LAUNCH_STOP_FAILED")?; }
        return Err(error);
    }
    if let Some(pid) = modules::process::resolve_codex_pid(settings.last_pid, None) {
        if matches!(proxy, modules::process::CodexLaunchProxy::Proxy(_)) {
            let checked = modules::codex_proxy_launch_guard::verify_profile(&target.user_data_dir,
                target.bind_account_id.as_deref().unwrap_or("__client__"), &proxy);
            let mut expected = Vec::new();
            proxy.append_electron_args(&mut expected);
            let actual = modules::process::collect_codex_proxy_process_snapshot()?;
            let args_ok = actual.iter().find(|(id, _, _)| *id == pid).is_some_and(|(_, _, args)| {
                expected.iter().all(|arg| args.iter().any(|a| a.to_str() == Some(arg.as_str())))
            });
            if checked.is_err() || !args_ok {
                modules::process::close_pid(pid, 5).map_err(|_| "PROXY_LAUNCH_STOP_FAILED")?;
                return Err("PROXY_LAUNCH_RESTART_REQUIRED".into());
            }
        }
        return modules::process::focus_codex_instance(Some(pid), None).map(|_| ());
    }
    drop(profile_lease);
    super::codex_instance::codex_start_instance(app, super::codex_instance::DEFAULT_INSTANCE_ID.into(), None, None).await.map(|_| ())
}

#[tauri::command]
pub async fn retry_codex_desktop_launch(app: tauri::AppHandle) -> Result<(), String> {
    *FAILURE.lock().map_err(|_| ERROR)? = None;
    let result = launch_last(app).await;
    if let Err(error) = &result { *FAILURE.lock().map_err(|_| ERROR)? = Some(error.clone()); }
    result
}
#[tauri::command]
pub fn codex_desktop_launch_failure() -> Option<String> { FAILURE.lock().ok().and_then(|v| v.clone()) }
#[tauri::command]
pub fn dismiss_codex_desktop_launch_failure() { if let Ok(mut failure) = FAILURE.lock() { *failure = None; } }

pub fn handle(app: &tauri::AppHandle, args: &[String]) -> bool {
    if !cfg!(windows) || !launch_requested(args) { return false; }
    if let Some(main) = app.get_webview_window("main") { let _ = main.hide(); }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match retry_codex_desktop_launch(app.clone()).await {
            Ok(()) => (),
            Err(_) => {
                let _ = modules::floating_card_window::show_main_window(&app);
                // Store + event: cold startup must not lose an error before React subscribes.
                let _ = app.emit("codex:desktop-launch-blocked", ());
            }
        }
    });
    true
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct Entry {
    original: Option<Vec<u8>>,
    installed: Option<Vec<u8>>,
    pending: Option<Pending>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Pending { before: Option<Vec<u8>>, after: Option<Vec<u8>> }
fn state_path() -> Result<PathBuf, String> { Ok(modules::account::get_data_dir().map_err(|_| ERROR)?.join("codex-desktop-entry.json")) }
fn read_link(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(m) if m.is_file() && !m.file_type().is_symlink() && m.len() <= 1024 * 1024 => (),
        _ => return Err(ERROR.into()),
    }
    std::fs::read(path).map(Some).map_err(|_| ERROR.into())
}
fn read_entry(path: &Path) -> Result<Entry, String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Entry::default()),
        Ok(m) if m.is_file() && !m.file_type().is_symlink() && m.len() <= 16 * 1024 * 1024 => (),
        _ => return Err(ERROR.into()),
    }
    let raw = std::fs::read_to_string(path).map_err(|_| ERROR)?;
    modules::secure_account_storage::deserialize_encrypted_snapshot(&raw).map_err(|_| ERROR.into())
}
fn write_entry(path: &Path, entry: &Entry) -> Result<(), String> {
    let raw = modules::secure_account_storage::serialize_account_file("codex-desktop-entry", entry).map_err(|_| ERROR)?;
    modules::atomic_write::write_secret_string_atomic(path, &raw).map_err(|_| ERROR.into())
}
fn shortcut_path() -> Result<PathBuf, String> { Ok(dirs::desktop_dir().ok_or(ERROR)?.join("Codex.lnk")) }

#[cfg(windows)]
fn build_shortcut(staging: &Path, exe: &Path, original: Option<&Path>) -> Result<Vec<u8>, String> {
    use std::{os::windows::process::CommandExt, process::{Command, Stdio}};
    use base64::Engine as _;
    fn quoted(path: &Path) -> String { format!("'{}'", path.to_string_lossy().replace('\'', "''")) }
    let script = format!(r#"$ErrorActionPreference='Stop'
$shell=New-Object -ComObject WScript.Shell
$link=$shell.CreateShortcut({})
$link.TargetPath={}
$link.Arguments='{}'
$link.Description='Codex'
$link.IconLocation='{},0'
{}
$link.Save()
"#, quoted(staging), quoted(exe), ARG, exe.to_string_lossy().replace('\'', "''"),
        original.map(|path| format!("$previous=$shell.CreateShortcut({}); if ($previous.IconLocation) {{ $link.IconLocation=$previous.IconLocation }}", quoted(path))).unwrap_or_default());
    let encoded = base64::engine::general_purpose::STANDARD.encode(script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>());
    let status = Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .creation_flags(0x08000000).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status().map_err(|_| ERROR)?;
    if !status.success() { return Err(ERROR.into()); }
    read_link(staging)?.ok_or_else(|| ERROR.into())
}

#[tauri::command]
pub fn codex_desktop_entry_enabled() -> Result<bool, String> {
    if !cfg!(windows) { return Ok(false); }
    let entry = read_entry(&state_path()?)?;
    Ok(entry.pending.is_none() && entry.installed.is_some() && entry.installed == read_link(&shortcut_path()?)?)
}

#[tauri::command]
pub async fn configure_codex_desktop_entry(enabled: bool) -> Result<(), String> {
    #[cfg(not(windows))]
    { let _ = enabled; return Err("PROXY_DESKTOP_WINDOWS_ONLY".into()); }
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || configure_inner(enabled)).await.map_err(|_| ERROR)?
    }
}
#[cfg(windows)]
fn configure_inner(enabled: bool) -> Result<(), String> {
    static WRITES: Mutex<()> = Mutex::new(());
    let _write = WRITES.lock().map_err(|_| ERROR)?;
    let path = state_path()?;
    let shortcut = shortcut_path()?;
    let mut current = read_link(&shortcut)?;
    let mut state = read_entry(&path)?;
    if let Some(pending) = state.pending.as_ref() {
        if current == pending.before {
            if !modules::atomic_write::replace_bytes_if_matches(&shortcut, current.as_deref(), pending.after.as_deref())? { return Err("PROXY_SHORTCUT_CHANGED".into()); }
            current = pending.after.clone();
        } else if current != pending.after { return Err("PROXY_SHORTCUT_CHANGED".into()); }
        state.pending = None;
        write_entry(&path, &state)?;
    }
    if state.installed.is_some() && current != state.installed { return Err("PROXY_SHORTCUT_CHANGED".into()); }
    if !enabled && state.installed.is_none() { return Ok(()); }
    if enabled && state.installed.is_some() { return Ok(()); }
    let next = if enabled {
        let staging = path.with_file_name(format!("codex-entry-{}.lnk", uuid::Uuid::new_v4()));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }
        let _cleanup = Cleanup(staging.clone());
        let bytes = build_shortcut(&staging, &std::env::current_exe().map_err(|_| ERROR)?, current.as_ref().map(|_| shortcut.as_path()))?;
        state.original = current.clone();
        state.installed = Some(bytes.clone());
        Some(bytes)
    } else { let original = state.original.take(); state.installed = None; original };
    state.pending = Some(Pending { before: current.clone(), after: next.clone() });
    write_entry(&path, &state)?;
    if !modules::atomic_write::replace_bytes_if_matches(&shortcut, current.as_deref(), next.as_deref())? { return Err("PROXY_SHORTCUT_CHANGED".into()); }
    state.pending = None;
    write_entry(&path, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_launch_requires_the_exact_opt_in_argument() {
        assert!(launch_requested(&["cockpit.exe".into(), ARG.into()]));
        for args in [vec!["cockpit.exe"], vec!["cockpit.exe", "--other"], vec!["cockpit.exe", ARG, "untrusted"]] {
            assert!(!launch_requested(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()));
        }
    }
    #[cfg(windows)]
    #[test]
    fn shortcut_generation_is_isolated_and_preserves_spaces_in_paths() {
        let root = std::env::temp_dir().join(format!("codex-entry-fixture-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("fixture.lnk");
        let bytes = build_shortcut(&file, &std::env::current_exe().unwrap(), None).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(read_link(&file).unwrap(), Some(bytes));
        std::fs::remove_dir_all(root).unwrap();
    }
}
