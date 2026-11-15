import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { useEscCloseTopmost } from '../../hooks/useEscClose';
import { useModalScrollLock } from '../../hooks/useModalScrollLock';
import { useModalFocusTrap } from '../../hooks/useModalFocusTrap';
import { ModalErrorMessage } from '../ModalErrorMessage';

function messageKey(error: unknown): string {
  const code = String(error);
  if (code.includes('PROXY_ENV_CHANGED')) return 'codex.proxy.desktop.envChanged';
  if (code.includes('PROXY_LAUNCH_RESTART_REQUIRED')) return 'codex.proxy.desktop.restartRequired';
  if (code.includes('PROXY_SHORTCUT_CHANGED')) return 'codex.proxy.desktop.shortcutChanged';
  if (code.includes('PROXY_LAUNCH_CHECK_FAILED')) return 'codex.proxy.desktop.checkFailed';
  if (code.includes('PROXY_LAUNCH_STOP_FAILED')) return 'codex.proxy.desktop.stopFailed';
  return '';
}

/** Mounted at app root so native desktop-entry failures cannot disappear with page navigation. */
export function CodexDesktopLaunchNotice() {
  const { t } = useTranslation();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const dismiss = () => {
    if (busy) return;
    setError('');
    void invoke('dismiss_codex_desktop_launch_failure').catch(() => {});
  };
  useEscCloseTopmost(Boolean(error), dismiss, ref);
  useModalScrollLock(Boolean(error));
  useModalFocusTrap(ref, Boolean(error));
  useEffect(() => {
    let alive = true;
    const read = () => { void invoke<string | null>('codex_desktop_launch_failure').then(value => {
      if (alive) setError(value ?? '');
    }).catch(() => {}); };
    const subscribed = listen('codex:desktop-launch-blocked', read);
    read();
    return () => { alive = false; void subscribed.then(unlisten => unlisten()); };
  }, []);
  if (!error) return null;
  const retry = async () => {
    if (busy) return;
    setBusy(true);
    try { await invoke('retry_codex_desktop_launch'); setError(''); }
    catch (caught) { setError(String(caught)); }
    finally { setBusy(false); }
  };
  return createPortal(<div className="modal-overlay"><div className="modal" ref={ref} role="dialog" aria-modal="true" aria-labelledby="codex-desktop-blocked-title" tabIndex={-1} style={{ maxWidth: 560 }}>
    <header className="modal-header"><h2 id="codex-desktop-blocked-title">{t('codex.proxy.desktop.blocked')}</h2></header>
    <div className="modal-body"><p>{t('codex.proxy.desktop.blockedHint')}</p>
      <ModalErrorMessage message={messageKey(error) ? t(messageKey(error)) : error} /></div>
    <footer className="modal-footer"><button type="button" className="btn btn-secondary" disabled={busy} onClick={dismiss}>{t('codex.proxy.desktop.back')}</button>
      <button type="button" className="btn btn-primary" disabled={busy} onClick={() => { void retry(); }}>{t(busy ? 'common.loading' : 'codex.proxy.desktop.retry')}</button></footer>
  </div></div>, document.body);
}

export function CodexDesktopProxyEntryButton() {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const ref = useRef<HTMLDivElement>(null);
  useEscCloseTopmost(open, () => { if (!busy) setOpen(false); }, ref);
  useModalScrollLock(open);
  useModalFocusTrap(ref, open);
  useEffect(() => {
    let alive = true;
    void invoke<boolean>('codex_desktop_entry_enabled').then(value => { if (alive) setEnabled(value); }).catch(() => {});
    return () => { alive = false; };
  }, []);
  if (!navigator.userAgent.includes('Windows')) return null;
  const save = async () => {
    if (busy) return;
    setBusy(true); setError('');
    try {
      await invoke('configure_codex_desktop_entry', { enabled: !enabled });
      setEnabled(!enabled); setOpen(false);
    } catch (caught) { setError(messageKey(caught) ? t(messageKey(caught)) : t('codex.proxy.desktop.setupFailed')); }
    finally { setBusy(false); }
  };
  return <><button type="button" className="btn btn-secondary compact" onClick={() => { setError(''); setOpen(true); }}>
    {t(enabled ? 'codex.proxy.desktop.enabled' : 'codex.proxy.desktop.enable')}</button>
    {open && createPortal(<div className="modal-overlay"><div className="modal" ref={ref} role="dialog" aria-modal="true" aria-labelledby="codex-desktop-entry-title" tabIndex={-1} style={{ maxWidth: 560 }}>
      <header className="modal-header"><h2 id="codex-desktop-entry-title">{t('codex.proxy.desktop.title')}</h2></header>
      <div className="modal-body"><p>{t(enabled ? 'codex.proxy.desktop.disableHint' : 'codex.proxy.desktop.enableHint')}</p>
        <p className="codex-proxy-page-note">{t('codex.proxy.desktop.scopeHint')}</p><ModalErrorMessage message={error} /></div>
      <footer className="modal-footer"><button type="button" className="btn btn-secondary" disabled={busy} onClick={() => setOpen(false)}>{t('common.cancel')}</button>
        <button type="button" className="btn btn-primary" disabled={busy} onClick={() => { void save(); }}>{t(enabled ? 'codex.proxy.desktop.disable' : 'common.confirm')}</button></footer>
    </div></div>, document.body)}</>;
}
