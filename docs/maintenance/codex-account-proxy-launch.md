# Windows Codex account proxy launch

Account switching keeps the existing CODEX_HOME, Windows package identity,
project/history projection, MCP and skills behavior. CI does not use real accounts.

- Independent and shared account proxies use the existing stable loopback ingress.
- Client settings remains separate from an explicit force-direct choice.
- The existing managed .env projection preserves per-account originals, unrelated
  edits, absent/empty files, and recoverable encrypted ownership metadata.

## Optional country check inside Cockpit

The account proxy dialog exposes an independent country-check setting, disabled
by default. An account without a proxy binding can also choose an expected country.
The setting is saved separately from the proxy binding.

When launching from Cockpit, ordinary instance startup and switch-and-start both
check the expected HTTP exit before proceeding, then recheck just before starting
the GUI. Retry repeats the check; skipping a launch-progress step never skips it.
The check is not applied to prepare-only CLI actions.

A credential-free GET to ChatGPT Cloudflare trace reads its loc country code.
A mismatch shows expected and observed ISO country codes and blocks launch.
Timeouts, non-success responses, invalid/ambiguous country data or settings changes
also block launch. No cookies, OAuth tokens, quota checks or model requests are sent.
A selected proxy failure never falls back to another route.

Bound proxies use the same stable ingress as the client. Unbound modes inspect
the launch environment and the existing .env projection without changing either,
and use the existing system-proxy resolver when applicable. Unsupported .env
interpolation is not guessed. Country checking does not certify every client
protocol or dynamic PAC/Chromium-specific routing rule.

## Desktop behavior

There is no desktop-startup integration, background launcher, shortcut editor or
startup hook. This version does not inspect, replace, migrate or restore desktop
shortcuts. Opening Codex directly is outside the Cockpit startup check.

## Validation

Tests use temporary profiles, generated policies and loopback sockets. They do
not install or start the user's Codex, switch a real account, or use account tokens.
Windows builds and regression checks run on GitHub Actions.
