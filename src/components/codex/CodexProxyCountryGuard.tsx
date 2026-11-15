import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { getCodexProxyCountryPolicy, setCodexProxyCountryPolicy } from '../../services/codexProxyCountryService';
import { codexCountryErrorKey, codexCountryOptions } from '../../utils/codexProxyCountry';
import { SingleSelectDropdown } from '../SingleSelectDropdown';
import { ModalErrorMessage } from '../ModalErrorMessage';

/** Independent of proxy bindings: client/system/TUN users may opt in too.
 * Editing or closing the dialog never writes until this section's Save action. */
export function CodexProxyCountryGuard({ accountId, disabled = false }: { accountId: string; disabled?: boolean }) {
  const { t, i18n } = useTranslation();
  const [ready, setReady] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [country, setCountry] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState(false);
  const [revision, setRevision] = useState(0);
  const current = useRef(accountId);
  current.current = accountId;
  const language = i18n.resolvedLanguage || i18n.language || 'en';
  const options = useMemo(() => [{ value: '', label: t('codex.proxy.country.choose') }, ...codexCountryOptions(language)], [language, t]);
  useEffect(() => {
    let alive = true;
    setReady(false); setBusy(false); setError(''); setNotice(false);
    void getCodexProxyCountryPolicy(accountId).then(value => {
      if (!alive) return;
      setSaved(value.expectedCountry); setEnabled(Boolean(value.expectedCountry)); setCountry(value.expectedCountry ?? ''); setReady(true);
    }).catch(() => { if (alive) setError(t('codex.proxy.country.storage')); });
    return () => { alive = false; };
  }, [accountId, revision, t]);
  const expected = enabled ? country : null;
  const save = async () => {
    if (disabled || !ready || busy || expected === saved || (enabled && !country)) return;
    const id = accountId;
    setBusy(true); setError(''); setNotice(false);
    try {
      const value = await setCodexProxyCountryPolicy(id, expected);
      if (current.current === id) { setSaved(value.expectedCountry); setNotice(true); }
    } catch (caught) {
      if (current.current === id) setError(t(codexCountryErrorKey(caught) ?? 'codex.proxy.country.storage'));
    } finally { if (current.current === id) setBusy(false); }
  };
  return <section className="codex-proxy-country-guard" aria-label={t('codex.proxy.country.title')}>
    <label className="codex-proxy-country-toggle"><input type="checkbox" checked={enabled} disabled={disabled || !ready || busy}
      onChange={event => { setEnabled(event.target.checked); setNotice(false); }} /><strong>{t('codex.proxy.country.title')}</strong></label>
    <p className="codex-proxy-page-note">{t('codex.proxy.country.hint')}</p>
    {enabled && <label className="codex-proxy-field"><span>{t('codex.proxy.country.expected')}</span>
      <SingleSelectDropdown value={country} options={options} disabled={disabled || !ready || busy} ariaLabel={t('codex.proxy.country.expected')}
        onChange={value => { setCountry(value); setNotice(false); }} /></label>}
    <ModalErrorMessage message={error} />
    {!ready && error && <button type="button" className="btn btn-secondary compact" onClick={() => setRevision(v => v + 1)}>{t('common.retry')}</button>}
    <div className="codex-proxy-account-check-actions"><button type="button" className="btn btn-secondary compact"
      disabled={disabled || !ready || busy || expected === saved || (enabled && !country)} onClick={() => { void save(); }}>
      {t(busy ? 'common.saving' : 'codex.proxy.country.save')}</button>
      {notice && <span className="codex-proxy-page-note" role="status">{t('codex.proxy.country.saved')}</span>}
    </div>
  </section>;
}
