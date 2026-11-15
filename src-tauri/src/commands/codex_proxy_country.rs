use crate::modules::codex_proxy_launch_guard::{self, CountryPolicy};

#[tauri::command]
pub async fn get_codex_proxy_country_policy(account_id: String) -> Result<CountryPolicy, String> {
    tauri::async_runtime::spawn_blocking(move || codex_proxy_launch_guard::policy(&account_id))
        .await.map_err(|_| "PROXY_COUNTRY_STORAGE".to_string())?
}
#[tauri::command]
pub async fn set_codex_proxy_country_policy(account_id: String, expected_country: Option<String>) -> Result<CountryPolicy, String> {
    tauri::async_runtime::spawn_blocking(move || codex_proxy_launch_guard::save_policy(&account_id, expected_country))
        .await.map_err(|_| "PROXY_COUNTRY_STORAGE".to_string())?
}
