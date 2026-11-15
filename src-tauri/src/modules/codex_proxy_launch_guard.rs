//! Optional country gate for launches initiated inside Cockpit only.
use super::process::CodexLaunchProxy;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::Mutex, time::Duration};

const UNKNOWN: &str = "PROXY_COUNTRY_UNAVAILABLE";
const STORAGE: &str = "PROXY_COUNTRY_STORAGE";
const ENDPOINT: &str = "https://chatgpt.com/cdn-cgi/trace";
static POLICY_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CountryPolicy { pub expected_country: Option<String> }

fn country_valid(value: &str) -> bool {
    value.len() == 2 && value.bytes().all(|b| b.is_ascii_uppercase()) &&
        serde_json::from_str::<Vec<String>>(include_str!("../../../src/data/codexCountryCodes.json"))
            .is_ok_and(|codes| codes.iter().any(|code| code == value))
}
fn policy_key(account: &str) -> Result<String, String> {
    let account = account.trim();
    if account.is_empty() || account.len() > 256 || account.chars().any(char::is_control) {
        return Err(STORAGE.into());
    }
    Ok(super::codex_instance::parse_provider_gateway_bind_account_id(account).unwrap_or_else(|| account.into()))
}
fn policy_path() -> Result<PathBuf, String> {
    Ok(super::account::get_data_dir().map_err(|_| STORAGE)?.join("codex-proxy-country.json"))
}
fn read_policies(path: &Path) -> Result<BTreeMap<String, String>, String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Ok(m) if m.is_file() && !m.file_type().is_symlink() && m.len() <= 1024 * 1024 => (),
        _ => return Err(STORAGE.into()),
    }
    let raw = std::fs::read_to_string(path).map_err(|_| STORAGE)?;
    let values: BTreeMap<String, String> = super::secure_account_storage::deserialize_encrypted_snapshot(&raw).map_err(|_| STORAGE)?;
    if values.len() > 4096 || values.values().any(|c| !country_valid(c)) { return Err(STORAGE.into()); }
    Ok(values)
}
pub fn policy(account: &str) -> Result<CountryPolicy, String> {
    let _lock = POLICY_LOCK.lock().map_err(|_| STORAGE)?;
    Ok(CountryPolicy { expected_country: read_policies(&policy_path()?)?.get(&policy_key(account)?).cloned() })
}
pub fn save_policy(account: &str, expected: Option<String>) -> Result<CountryPolicy, String> {
    let expected = expected.map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty());
    if expected.as_deref().is_some_and(|s| !country_valid(s)) { return Err("PROXY_COUNTRY_INVALID".into()); }
    let _lock = POLICY_LOCK.lock().map_err(|_| STORAGE)?;
    let path = policy_path()?;
    let mut policies = read_policies(&path)?;
    let key = policy_key(account)?;
    if let Some(country) = &expected {
        if policies.len() >= 4096 && !policies.contains_key(&key) { return Err(STORAGE.into()); }
        policies.insert(key, country.clone());
    } else { policies.remove(&key); }
    let raw = super::secure_account_storage::serialize_account_file("codex-proxy-country", &policies).map_err(|_| STORAGE)?;
    super::atomic_write::write_secret_string_atomic(&path, &raw).map_err(|_| STORAGE)?;
    Ok(CountryPolicy { expected_country: expected })
}
fn parse_country(body: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(body).map_err(|_| UNKNOWN)?;
    let mut found = text.lines().filter_map(|line| line.strip_prefix("loc="));
    let country = found.next().filter(|c| country_valid(c)).ok_or(UNKNOWN)?;
    if found.next().is_some() { return Err(UNKNOWN.into()); }
    Ok(country.to_owned())
}
fn compare_country(expected: &str, actual: &str) -> Result<(), String> {
    if expected == actual { Ok(()) } else { Err(format!("PROXY_COUNTRY_MISMATCH:{expected}:{actual}")) }
}
fn probe_client(proxy: Option<&str>, bypass: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(12));
    if let Some(proxy) = proxy {
        let route = reqwest::Proxy::all(proxy).map_err(|_| "PROXY_COUNTRY_ROUTE_UNKNOWN")?
            .no_proxy(bypass.and_then(reqwest::NoProxy::from_string));
        builder = builder.proxy(route);
    }
    builder.build().map_err(|_| UNKNOWN.into())
}
async fn query_country(client: &reqwest::Client, endpoint: &str) -> Result<String, String> {
    let mut response = client.get(endpoint).send().await.map_err(|_| UNKNOWN)?;
    if !response.status().is_success() { return Err(UNKNOWN.into()); }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| UNKNOWN)? {
        if body.len() + chunk.len() > 16384 { return Err(UNKNOWN.into()); }
        body.extend_from_slice(&chunk);
    }
    parse_country(&body)
}
pub async fn check_for_launch(account: Option<&str>, home: &Path) -> Result<(), String> {
    if !cfg!(windows) { return Ok(()); }
    let bound = account;
    let account = account.unwrap_or("__client__");
    let expected = policy(account)?.expected_country;
    let Some(expected) = expected else { return Ok(()); };
    // Account proxy resolution does not authenticate or request a model.
    let proxy = super::codex_instance::resolve_egress_proxy_for_bind_account(bound).await?;
    check_route(account, home, &proxy, &expected).await?;
    if policy(account)?.expected_country.as_deref() != Some(expected.as_str()) {
        return Err("PROXY_COUNTRY_CHANGED".into());
    }
    Ok(())
}
async fn check_route(account: &str, home: &Path, proxy: &CodexLaunchProxy, expected: &str) -> Result<(), String> {
    let (route, bypass) = match proxy {
        CodexLaunchProxy::Proxy(url) => { proxy.dotenv_overrides()?; (Some(url.clone()), None) },
        CodexLaunchProxy::Direct => (None, None),
        CodexLaunchProxy::Inherit | CodexLaunchProxy::ClientDefaults => {
            let mut env = super::process::proxy_environment_for_check(proxy);
            env.extend(super::codex_proxy_dotenv::preview_proxy_variables(home, account, proxy)?);
            let explicit = env.get("https_proxy").filter(|s| !s.trim().is_empty())
                .or_else(|| env.get("all_proxy").filter(|s| !s.trim().is_empty())).cloned();
            let route = explicit.or_else(|| super::codex_local_access::system_proxy_url_for_target(ENDPOINT));
            (route, env.get("no_proxy").cloned())
        },
    };
    let client = probe_client(route.as_deref(), bypass.as_deref())?;
    let actual = tokio::time::timeout(Duration::from_secs(15), query_country(&client, ENDPOINT))
        .await.map_err(|_| UNKNOWN)??;
    compare_country(expected, &actual)
}
pub fn verify_profile(home: &Path, account: &str, proxy: &CodexLaunchProxy) -> Result<(), String> {
    if !cfg!(windows) { return Ok(()); }
    super::codex_proxy_dotenv::verify(home, account, proxy)
}
pub fn verify_or_stop(home: &Path, account: &str, pid: u32, proxy: &CodexLaunchProxy) -> Result<(), String> {
    if let Err(error) = verify_profile(home, account, proxy) {
        super::process::close_pid(pid, 5).map_err(|_| "PROXY_LAUNCH_STOP_FAILED")?;
        return Err(error);
    }
    Ok(())
}
#[cfg(test)]
#[path = "codex_proxy_launch_guard_tests.rs"]
mod tests;
