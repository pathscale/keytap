# keytap

Driverless macOS key remapper. A CoreGraphics event tap rewrites keystrokes;
no Karabiner driver, no system extension, no reboot loop.

## Problem

With the macOS Modifier Keys swap (System Settings → Keyboard → Modifier Keys:
Control and Command swapped on both sides), the physical Command key emits
Control. Physical Command+Tab therefore arrives as Control+Tab and the app
switcher does not open. Karabiner-Elements can fix that, but its DriverKit
virtual keyboard is fragile (`virtual_hid_keyboard is not ready`).

## Design

**Keep the macOS swap ON.** keytap does not undo it. It only rewrites the
chords that break (all default on):

- Control+Tab / Control+Shift+Tab → Command(+Shift)+Tab (`control_tab_to_command_tab`)
- Control+3 → Command+3 (`control_3_to_command_3`)
- Control(+Shift)+arrows → Command(+Shift)+arrows (`control_arrows_to_command_arrows`)
- Control+left click → Command+click (`control_click_to_command_click`)

### Held-modifier session

The app switcher stays open while Command is held. After keytap remaps the
first Control+Tab it enters a short session:

1. Suppress the Tab.
2. Post Control key-up (retract the Control the OS already saw).
3. Post Command key-down (present Command as held).
4. Post Tab key-down with the Command flag (and Shift if present).
5. While that Control stays physically held: rewrite later Tab / arrow /
   FlagsChanged events so they show Command instead of Control.
6. On Control key-up (or FlagsChanged that clears Control): suppress it and
   post Command key-up so the switcher selects and dismisses.

Plain Control without Tab is untouched. Control+C and friends pass through.
No sticky Command is left behind (see fixtures).

### Event tap

- `CGEventTapCreate` at `kCGSessionEventTap`, `kCGHeadInsertEventTap`,
  keyDown / keyUp / flagsChanged, plus leftMouseDown / leftMouseUp /
  leftMouseDragged when `control_click_to_command_click` is on (never mouse
  moves or other buttons)
- Callback kept small: parse → `keytap_rules::Engine::handle` → pass / suppress / post
- On `kCGEventTapDisabledByTimeout` or `...UserInput`, re-enable the tap
- Hand-written `extern "C"` to CoreGraphics, CoreFoundation, ApplicationServices
  (no `-sys` crates, no objc2, no C/C++ in tree)
- Engine state lives in a `Box` owned by `main`; the callback gets a raw pointer.
  The run loop is single-threaded, so no locks, atomics, or lazy globals

### Config

Hand-parsed `key = value` file (no serde, no RON). RON was considered; a four-line
boolean file does not justify a parser crate.

Path order:

1. `KEYTAP_CONFIG`
2. `~/Library/Application Support/keytap/config`

```
# keytap config
control_tab_to_command_tab = true
control_3_to_command_3 = true
control_arrows_to_command_arrows = true
control_click_to_command_click = true
```

`control_3_to_command_3`: physical Command+3 (Control+3 after the OS swap) reaches
apps as Command+3. Only the digit 3 with Control alone; other digits and
Shift/Option combos pass through. Control is restored afterwards while still held.

`control_arrows_to_command_arrows`: physical Command+Left/Right/Up/Down
(Control+arrow after the OS swap) reaches apps as Command+arrow: line start/end,
document top/bottom. Same burst as Control+3, left or right Control. Shift is
kept: physical Command+Shift+arrow (Control+Shift+arrow after the swap) reaches
apps as Command+Shift+arrow, selecting to line start/end or document
top/bottom; the Shift key itself is never touched. Option/Command combos,
Shift+arrow without Control, and plain arrows pass through.

`control_click_to_command_click`: physical Command+click (Control+left click
after the OS swap, which macOS would turn into a context menu) reaches apps as
Command+click, so Chrome opens the link in a new background tab;
Command+Shift+click opens a foreground tab. The tap sees the left mouse down
before AppKit converts Control+click, and only rewrites its flags (Control to
Command; Shift, Option, Caps Lock kept). The mouse down decides; its drags and
mouse up follow that decision, so a click is never half rewritten. Clicks that
already carry Command, plain clicks, Option clicks, and every right click pass
untouched. With the swap, physical Control arrives as Command, so the native
Control+click context menu is not reachable from the keyboard any more: use a
right click (or two-finger click) instead.

## Layout

```
crates/rules      pure state machine (no I/O, no unsafe)
crates/keytap     daemon binary (event tap + config)
crates/fixtures   fixture runner
fixtures/cases    event-sequence transcripts
scripts/          bun bundle + install/uninstall for launchd
packaging/        Info.plist + launchd plist template (placeholders only)
UI.md             design for an optional Tauri/izumo UI
```

## Build and gate

```
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p fixtures -- check
```

No unit tests. Behaviour is proven by fixtures (`cargo run -p fixtures -- regen`
to refresh expected actions after an intentional rule change).

No `Cargo.lock` is committed (same policy as chuzz).

## Rebuild checklist

End-to-end rebuild and reinstall after a rule or daemon change:

1. Gate (build, clippy, fmt, fixtures). Only run `cargo run -p fixtures -- regen`
   after an intentional rule change; otherwise keep `check`:

```
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p fixtures -- check
```

2. Release binary, bundle, install. The install script turns debug ON by
   default, so prefix `KEYTAP_DEBUG_INSTALL=0` unless you want noisy logs:

```
cargo build --release
bun scripts/bundle.bun.ts
KEYTAP_DEBUG_INSTALL=0 bun scripts/install.bun.ts install
```

3. Re-grant Accessibility (every ad-hoc rebuild drops the TCC grant; see
   Bundle and Accessibility below). Toggle Keytap off and on in
   System Settings → Privacy & Security → Accessibility.

4. Verify the daemon is live:

```
tail ~/Library/Logs/keytap.log
# expect: keytap: tap enabled, run loop starting
```

Optional restart without a full reinstall:

```
launchctl kickstart -k gui/$UID/dev.keytap
```

Stop:

```
launchctl bootout gui/$UID/dev.keytap
```

Debug logging is controlled by `KEYTAP_DEBUG` in the launchd plist. Leave it
off for normal use; it is noisy.

## Bundle and Accessibility (TCC)

macOS keys Accessibility grants to the **signed code identity**. An unbundled
`target/debug/keytap` run from a terminal inherits the terminal's identity, so
toggling the terminal (or a one-off binary path) does not trust Keytap. Package
it as `Keytap.app` with a stable identifier (`dev.keytap`):

```
bun scripts/bundle.bun.ts
codesign -dvv target/Keytap.app          # Identifier=dev.keytap
target/Keytap.app/Contents/MacOS/keytap --check-ffi
open target/Keytap.app                   # registers Keytap in Accessibility
```

Then: System Settings → Privacy & Security → Accessibility → enable **Keytap**.
Verify with:

```
target/Keytap.app/Contents/MacOS/keytap --check-ffi   # expect: trusted true
```

`security find-identity -p codesigning -v` listed no Apple Development identity
on the build machine, so the bundle is ad-hoc signed (`codesign --sign -`) with
`--identifier dev.keytap`. Ad-hoc signing is **not** a stable local TCC
identity: macOS ties the Accessibility grant to the binary's code hash, so
**every rebuild drops Accessibility**. The daemon then logs
`keytap: not trusted for Accessibility yet` and launchd retries every 10s.

Fix after each rebuild: toggle Keytap off and on in System Settings →
Privacy & Security → Accessibility (or remove the entry, `open target/Keytap.app`,
re-enable).

Permanent fix options:

- A self-signed Code Signing certificate from Keychain Access → Certificate
  Assistant. The bundle script today only picks an `Apple Development`
  identity; it would need a small change to accept the self-signed cert name.
- A free Apple Development certificate via Xcode (the bundle script picks it
  automatically when one exists).

Use Developer ID only if you distribute off this Mac.

## Run (manual)

Do not start the tap while you are mid-typing if you are unsure of the build.
`--print-config` is always safe. `--check-ffi` is safe. `--dry-run` creates a
tap but never suppresses or rewrites (still needs Accessibility).

```
bun scripts/bundle.bun.ts
target/Keytap.app/Contents/MacOS/keytap --print-config
target/Keytap.app/Contents/MacOS/keytap --check-ffi
target/Keytap.app/Contents/MacOS/keytap --write-config
open target/Keytap.app
```

## Login item

```
bun scripts/bundle.bun.ts
bun scripts/install.bun.ts install
bun scripts/install.bun.ts uninstall
```

The agent runs `Keytap.app/Contents/MacOS/keytap` (`ThrottleInterval` 10). The
plist template uses `@KEYTAP_BIN@` / `@KEYTAP_LOG@`; the bun script fills them
into `~/Library/LaunchAgents/dev.keytap.plist` at install time.

## macOS swap

Leave the System Settings Control ↔ Command swap enabled. keytap assumes that
layout. If you turn the swap off, Control+Tab will start opening the switcher
(because keytap will turn real Control+Tab into Command+Tab), which is probably
not what you want.

## Risks

- **Secure Input** (password fields, some sudo prompts) blocks event taps. Keys
  pass through unchanged until Secure Input ends; the tap stays alive.
- **Accessibility resets** after every ad-hoc rebuild (code-hash identity) and
  after some OS updates; the daemon fails to create a tap, logs
  `keytap: not trusted for Accessibility yet`, and launchd retries every 10s.
  Re-grant as under Bundle and Accessibility.
- **Two remappers at once.** Disable Karabiner (or at least its complex
  modifications) so it does not fight keytap.
- **Only the listed chords are remapped.** Control+C and other Control chords
  still pass through; PC-style copy/paste under the swap is out of scope.

## Verified

On 2026-10-11 (macOS 26.5), the user confirmed:

- Control+Tab app switcher open / cycle / select
- Command+3
- Command+arrows

## Unverified

- Behaviour inside Secure Input fields
- launchd KeepAlive across logout/login
- Right-Control chord on a keyboard that has one
- Whether `kCGSessionEventTap` vs `kCGHIDEventTap` matters for the Dock switcher
- Command+click and Command+Shift+arrow selection: fixtures pass, awaiting live confirmation
