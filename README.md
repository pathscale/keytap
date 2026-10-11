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
chords that break:

- Control+Tab → Command+Tab
- Control+Shift+Tab → Command+Shift+Tab

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

Then: System Settings -> Privacy & Security -> Accessibility -> enable **Keytap**.
Verify with:

```
target/Keytap.app/Contents/MacOS/keytap --check-ffi   # expect: trusted true
```

`security find-identity -p codesigning -v` listed no Apple Development identity
on the build machine, so the bundle is ad-hoc signed (`codesign --sign -`) with
`--identifier dev.keytap`. That is enough for a stable local TCC identity. Prefer
an Apple Development identity when one exists (the bundle script picks it
automatically); use Developer ID only if you distribute off this Mac.

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
- **App switcher quirks.** The synthetic Command key-down/up path matches what
  other remappers do, but has not been verified on this machine yet.
- **Accessibility resets** after some OS updates; the daemon will fail to create
  a tap and print the Settings path.
- **Two remappers at once.** Disable Karabiner (or at least its complex
  modifications) so it does not fight keytap.
- **PC-style copy/paste** (Control+C → Command+C under the swap) is out of
  scope for v1; only Tab is remapped.

## Unverified

- Real Control+Tab → app switcher open / cycle / select on macOS 26.5
- Behaviour inside Secure Input fields
- launchd KeepAlive across logout/login
- Right-Control chord on a keyboard that has one
- Whether `kCGSessionEventTap` vs `kCGHIDEventTap` matters for the Dock switcher
