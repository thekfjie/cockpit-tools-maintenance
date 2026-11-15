# Windows Codex account proxy launch

Account switching keeps the existing CODEX_HOME, registered Windows package identity,
project/history projection, MCP and skills behavior. No user account is used by CI.

- Independent and shared account proxies use the existing stable loopback ingress.
- Client settings is a separate persisted choice; existing direct choices stay direct.
- Only managed Windows launches take ownership of proxy settings in .env.
  Original settings are recorded per account and profile, encrypted in app data.
  Absence and empty files remain distinct; unrelated user edits are preserved.
- A write-ahead journal plus exact-content comparison makes interrupted changes
  recoverable. A modified owned section is an actionable conflict, never overwritten.
  Snapshots are bounded; no plaintext .env.bak or new CODEX_HOME is created.
- Before launch, a credential-free HTTPS HEAD verifies connectivity through the
  selected loopback proxy. TLS/login/WAF HTTP responses do not refresh credentials.
  Failed transport checks leave the existing switch/start progress UI available.
- Existing loopback entries cannot silently become direct when unbound. Restart
  applies client-original or direct settings after restoring the previous takeover.
- Desktop integration is opt-in in proxy account settings. It replaces only the
  current user's Codex.lnk, preserves its icon and encrypted original bytes, and
  restores them on disable. It never silently replaces a subsequently edited link.
  It launches Cockpit in the background and reuses the existing launch transaction.
  Errors persist until the React panel is ready; retry revalidates, never bypasses.

## Validation boundary

Tests use generated account records, temporary profiles and loopback sockets. They
never switch a real account, query a real model or install/start the user's Codex.
Windows package builds run on GitHub Actions. Successful transport/configuration
checks are not proof of the actual client's complete network path. No claim is
made that UDP, WebRTC or voice is isolated; enforcing that before any packet would
require a separately supported OS-level network policy. Launches bypassing the
managed desktop entry do not run its startup checks.
