import React from 'react';
import { createRoot } from 'react-dom/client';
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import zh from '../src/locales/zh-CN.json';
import { CodexProxyAccountDialog } from '../src/components/codex/CodexProxyAccountDialog';
import '../src/App.css';
window.__TAURI_INTERNALS__ = {
  metadata: {currentWindow:{label:'main'}, currentWebview:{label:'main'}},
  transformCallback: () => 1, unregisterCallback: () => {},
  invoke: async (command, args) => {
    if (command === 'plugin:event|listen') return 1;
    if (command === 'get_codex_proxy_country_policy') return {expectedCountry:'JP'};
    if (command === 'get_codex_proxy_country_checks') return [];
    if (command === 'get_codex_proxy_env') return {document:'HTTP_PROXY=http://127.0.0.1:7890\nHTTPS_PROXY=http://127.0.0.1:7890', source:'env', revision:'fixture', recognitionFailed:false};
    return {};
  },
};
i18n.use(initReactI18next).init({lng:'zh-CN', resources:{'zh-CN':{translation:zh}}, interpolation:{escapeValue:false}}).then(() => {
 createRoot(document.getElementById('root')!).render(<CodexProxyAccountDialog accountId="fixture" initialTab="edit" onClose={()=>{}} onApplied={()=>{}}/>);
});
