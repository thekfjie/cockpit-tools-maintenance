import { invoke } from '@tauri-apps/api/core';
export interface CodexProxyCountryPolicy { expectedCountry: string | null }
export function getCodexProxyCountryPolicy(accountId: string): Promise<CodexProxyCountryPolicy> {
  return invoke('get_codex_proxy_country_policy', { accountId });
}
export function setCodexProxyCountryPolicy(accountId: string, expectedCountry: string | null): Promise<CodexProxyCountryPolicy> {
  return invoke('set_codex_proxy_country_policy', { accountId, expectedCountry });
}
