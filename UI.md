# keytap-ui (design only)

Optional menu-bar / small window UI, built the chuzz way. Not in this tree yet.

## Stack

Same shape as chuzz:

- Rust host via Tauri + izumo (Blitz), SolidJS chrome interpreted by Boa
- UI crate is a separate workspace member (`apps/keytap-ui`) so the daemon stays
  zero-deps / no Cocoa
- Heavy deps (tauri, izumo, blitz, boa, frontend bundle) live only in the UI
  package; `keytap` and `keytap-rules` do not depend on them

## Process split

```
keytap          (daemon, launchd)     <--->  unix socket
keytap-ui       (Tauri window / menu)
```

- Daemon owns the event tap and the remap engine. Always.
- UI never installs its own tap. It talks to the daemon over a local socket.
- Socket path from env `KEYTAP_SOCK`, default
  `$HOME/Library/Application Support/keytap/keytap.sock`, mode 0600, never bound
  to a network interface.

## Protocol (draft)

Length-prefixed JSON lines, one request / one response, no auth (owner socket).

Requests the UI needs for v1:

- `status` → `{ running, config_path, config, tap_enabled, accessibility }`
- `get_config` / `set_config` → read/write the hand-parsed config file and ask
  the daemon to reload (no process restart)
- `enable` / `disable` → pause remapping without tearing down the tap
- `quit` → stop the daemon (launchd KeepAlive will restart unless unloaded)

Events the daemon may push (optional later):

- `tap_disabled` / `tap_reenabled`
- `reload_ok` / `reload_err`

## UI chrome

Minimal:

- Menu-bar icon: on / paused / permission-missing
- Window: toggle, current rule list (start with the one Control+Tab rule),
  "Open config", "Install / uninstall login item" (shells out to
  `bun scripts/install.bun.ts`), Accessibility deep-link
- No account, no network, no telemetry

## Permissions

Accessibility is required by the daemon binary. The UI should detect a missing
grant via `status` and show the System Settings path. If the UI is bundled as
`Keytap.app`, the entitlement / TCC identity is the app bundle; prefer launching
the daemon as a helper tool inside the bundle so both share one identity.

## Build cut

Ship the daemon alone first. Add `apps/keytap-ui` only after the remap is proven
on a real Control+Tab session. Keep the socket protocol small so a `nc`-level
client can debug without the UI.
