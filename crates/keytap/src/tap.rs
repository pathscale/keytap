//! CoreGraphics event tap: convert CG events to rules::Event, apply Engine, post.
//!
//! Synthetic events are tagged with `kCGEventSourceUserData = SYNTHETIC_TAG` and
//! posted with `CGEventTapPostEvent` so they land after this tap and do not
//! re-enter the callback. If one still appears (fallback `CGEventPost`), the
//! callback passes it through without feeding the engine. Re-entry was the
//! works-once bug: a synthetic Control-up ended the switcher session while the
//! physical Control key was still held.

use crate::ffi::{
    self, CFMachPortRef, CFRunLoopSourceRef, CGEventFlags, CGEventMask, CGEventRef,
    CGEventTapProxy, CGEventType,
};
use keytap_rules::{Action, Engine, Event, Flags, Key, Phase};
use std::ffi::c_void;
use std::ptr;

/// Magic `kCGEventSourceUserData` value marking events we synthesized ('KTP1').
pub const SYNTHETIC_TAG: i64 = 0x4B_54_50_31;

pub fn debug_from_env() -> bool {
    std::env::var_os("KEYTAP_DEBUG").is_some_and(|v| v != "0" && v != "false")
}

fn debug_log(state: &TapState, msg: &str) {
    if state.debug {
        eprintln!("keytap: {msg}");
    }
}

/// Privacy-safe key label: named only for modifiers/tab/arrows/escape.
fn debug_key(key: Key) -> String {
    match key {
        Key::Tab
        | Key::ControlL
        | Key::ControlR
        | Key::CommandL
        | Key::CommandR
        | Key::ShiftL
        | Key::ShiftR
        | Key::Left
        | Key::Right
        | Key::Up
        | Key::Down
        | Key::Escape => key.name().to_string(),
        Key::Other(_) => "other".to_string(),
    }
}

/// Heap state shared with the C callback. Owned by `run()` via Box; the tap
/// holds only a raw pointer. The CFRunLoop delivers callbacks serially on the
/// same thread that called `run()`, so no locks or atomics are required.
pub struct TapState {
    pub engine: Engine,
    pub port: CFMachPortRef,
    pub source: Option<CGEventSource>,
    pub dry_run: bool,
    pub debug: bool,
}

/// Thin owner for CGEventSourceRef.
pub struct CGEventSource {
    raw: ffi::CGEventSourceRef,
}

impl CGEventSource {
    pub fn new_private() -> Option<Self> {
        let raw = unsafe { ffi::CGEventSourceCreate(ffi::kCGEventSourceStatePrivate) };
        if raw.is_null() {
            None
        } else {
            Some(Self { raw })
        }
    }

    pub fn raw(&self) -> ffi::CGEventSourceRef {
        self.raw
    }
}

impl Drop for CGEventSource {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { ffi::CFRelease(self.raw as ffi::CFTypeRef) };
            self.raw = ptr::null_mut();
        }
    }
}

pub fn flags_from_cg(f: CGEventFlags) -> Flags {
    Flags {
        control: f & ffi::kCGEventFlagMaskControl != 0,
        command: f & ffi::kCGEventFlagMaskCommand != 0,
        shift: f & ffi::kCGEventFlagMaskShift != 0,
        option: f & ffi::kCGEventFlagMaskAlternate != 0,
        caps: f & ffi::kCGEventFlagMaskAlphaShift != 0,
    }
}

pub fn flags_to_cg(f: Flags, base: CGEventFlags) -> CGEventFlags {
    let managed = ffi::kCGEventFlagMaskControl
        | ffi::kCGEventFlagMaskCommand
        | ffi::kCGEventFlagMaskShift
        | ffi::kCGEventFlagMaskAlternate
        | ffi::kCGEventFlagMaskAlphaShift;
    let mut out = base & !managed;
    if f.control {
        out |= ffi::kCGEventFlagMaskControl;
    }
    if f.command {
        out |= ffi::kCGEventFlagMaskCommand;
    }
    if f.shift {
        out |= ffi::kCGEventFlagMaskShift;
    }
    if f.option {
        out |= ffi::kCGEventFlagMaskAlternate;
    }
    if f.caps {
        out |= ffi::kCGEventFlagMaskAlphaShift;
    }
    out
}

/// Non-modifier flag bits real hardware sets for a key. Arrow keys carry
/// SecondaryFn and NumericPad; posting them without these can confuse apps.
fn hardware_bits(key: Key) -> CGEventFlags {
    match key {
        Key::Left | Key::Right | Key::Up | Key::Down => {
            ffi::kCGEventFlagMaskNumericPad | ffi::kCGEventFlagMaskSecondaryFn
        }
        _ => 0,
    }
}

fn phase_from_cg(t: CGEventType) -> Option<Phase> {
    match t {
        ffi::kCGEventKeyDown => Some(Phase::KeyDown),
        ffi::kCGEventKeyUp => Some(Phase::KeyUp),
        ffi::kCGEventFlagsChanged => Some(Phase::FlagsChanged),
        _ => None,
    }
}

fn is_synthetic(event: CGEventRef) -> bool {
    unsafe { ffi::CGEventGetIntegerValueField(event, ffi::kCGEventSourceUserData) == SYNTHETIC_TAG }
}

fn event_from_cg(event: CGEventRef) -> Option<Event> {
    let t = unsafe { ffi::CGEventGetType(event) };
    let phase = phase_from_cg(t)?;
    let code =
        unsafe { ffi::CGEventGetIntegerValueField(event, ffi::kCGKeyboardEventKeycode) } as u16;
    let flags = flags_from_cg(unsafe { ffi::CGEventGetFlags(event) });
    Some(Event {
        phase,
        key: Key::from_code(code),
        flags,
    })
}

fn post_event(state: &TapState, proxy: CGEventTapProxy, ev: &Event) {
    if state.dry_run {
        eprintln!("dry-run post {}", ev.render());
        return;
    }
    let key_down = match ev.phase {
        Phase::KeyDown => true,
        Phase::KeyUp | Phase::FlagsChanged => false,
    };
    let source = state
        .source
        .as_ref()
        .map(CGEventSource::raw)
        .unwrap_or(ptr::null_mut());
    let cg = unsafe { ffi::CGEventCreateKeyboardEvent(source, ev.key.code(), key_down) };
    if cg.is_null() {
        eprintln!("keytap: CGEventCreateKeyboardEvent failed");
        return;
    }
    unsafe {
        ffi::CGEventSetFlags(cg, flags_to_cg(ev.flags, hardware_bits(ev.key)));
        ffi::CGEventSetIntegerValueField(cg, ffi::kCGEventSourceUserData, SYNTHETIC_TAG);
        // Post after this tap so the event does not re-enter our callback.
        if !proxy.is_null() {
            ffi::CGEventTapPostEvent(proxy, cg);
        } else {
            // Fallback: may re-enter; synthetic tag causes a pass-through then.
            ffi::CGEventPost(ffi::kCGSessionEventTap, cg);
        }
        ffi::CFRelease(cg as ffi::CFTypeRef);
    }
    debug_log(
        state,
        &format!(
            "post {} {} {} synthetic",
            ev.phase.name(),
            debug_key(ev.key),
            ev.flags.render()
        ),
    );
}

fn apply_actions(
    state: &mut TapState,
    proxy: CGEventTapProxy,
    original: CGEventRef,
    actions: &[Action],
) -> CGEventRef {
    if state.dry_run {
        for a in actions {
            eprintln!("dry-run {}", a.render());
        }
        return original;
    }
    let mut pass_back: CGEventRef = ptr::null_mut();
    for a in actions {
        match a {
            Action::Suppress => {
                debug_log(state, "action suppress");
            }
            Action::Pass(ev) => {
                unsafe {
                    ffi::CGEventSetIntegerValueField(
                        original,
                        ffi::kCGKeyboardEventKeycode,
                        i64::from(ev.key.code()),
                    );
                    let base = ffi::CGEventGetFlags(original);
                    ffi::CGEventSetFlags(original, flags_to_cg(ev.flags, base));
                }
                pass_back = original;
                debug_log(
                    state,
                    &format!(
                        "action pass {} {} {}",
                        ev.phase.name(),
                        debug_key(ev.key),
                        ev.flags.render()
                    ),
                );
            }
            Action::Post(ev) => post_event(state, proxy, ev),
        }
    }
    pass_back
}

pub unsafe extern "C" fn tap_callback(
    proxy: CGEventTapProxy,
    etype: CGEventType,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    if user_info.is_null() {
        return event;
    }
    let state = unsafe { &mut *(user_info as *mut TapState) };

    if etype == ffi::kCGEventTapDisabledByTimeout || etype == ffi::kCGEventTapDisabledByUserInput {
        eprintln!("keytap: tap disabled ({etype:#x}); re-enabling");
        if !state.port.is_null() {
            unsafe { ffi::CGEventTapEnable(state.port, true) };
            debug_log(state, "tap re-enabled");
        }
        return event;
    }

    if is_synthetic(event) {
        // Our own post came back (CGEventPost fallback). Do not feed the engine.
        if let Some(ev) = event_from_cg(event) {
            debug_log(
                state,
                &format!(
                    "in {} {} {} synthetic (ignore)",
                    ev.phase.name(),
                    debug_key(ev.key),
                    ev.flags.render()
                ),
            );
        } else {
            debug_log(state, "in unknown synthetic (ignore)");
        }
        return event;
    }

    let Some(ev) = event_from_cg(event) else {
        return event;
    };

    let before = state.engine.debug_state();
    debug_log(
        state,
        &format!(
            "in {} {} {} state={before} real",
            ev.phase.name(),
            debug_key(ev.key),
            ev.flags.render()
        ),
    );

    let actions = state.engine.handle(ev);
    let after = state.engine.debug_state();
    if before != after {
        debug_log(state, &format!("state {before} -> {after}"));
    }

    apply_actions(state, proxy, event, &actions)
}

pub fn event_mask() -> CGEventMask {
    ffi::cg_event_mask_bit(ffi::kCGEventKeyDown)
        | ffi::cg_event_mask_bit(ffi::kCGEventKeyUp)
        | ffi::cg_event_mask_bit(ffi::kCGEventFlagsChanged)
}

pub struct Tap {
    port: CFMachPortRef,
    source: CFRunLoopSourceRef,
    state: *mut TapState,
}

impl Tap {
    pub fn create(engine: Engine, dry_run: bool) -> Result<Self, String> {
        let source = CGEventSource::new_private();
        let debug = debug_from_env();
        if debug {
            eprintln!("keytap: KEYTAP_DEBUG=1 (modifier/tab/arrow/escape only)");
        }
        let state = Box::new(TapState {
            engine,
            port: ptr::null_mut(),
            source,
            dry_run,
            debug,
        });
        let state_ptr = Box::into_raw(state);

        let port = unsafe {
            ffi::CGEventTapCreate(
                ffi::kCGSessionEventTap,
                ffi::kCGHeadInsertEventTap,
                ffi::kCGEventTapOptionDefault,
                event_mask(),
                Some(tap_callback),
                state_ptr as *mut c_void,
            )
        };
        if port.is_null() {
            unsafe {
                drop(Box::from_raw(state_ptr));
            }
            return Err(
                "CGEventTapCreate returned null (Accessibility permission missing?)".into(),
            );
        }

        unsafe {
            (*state_ptr).port = port;
        }

        let rl_source = unsafe { ffi::CFMachPortCreateRunLoopSource(ptr::null(), port, 0) };
        if rl_source.is_null() {
            unsafe {
                ffi::CFRelease(port as ffi::CFTypeRef);
                drop(Box::from_raw(state_ptr));
            }
            return Err("CFMachPortCreateRunLoopSource failed".into());
        }

        Ok(Self {
            port,
            source: rl_source,
            state: state_ptr,
        })
    }

    pub fn run(&self) {
        unsafe {
            let rl = ffi::CFRunLoopGetCurrent();
            ffi::CFRunLoopAddSource(rl, self.source, ffi::kCFRunLoopCommonModes);
            ffi::CGEventTapEnable(self.port, true);
            eprintln!("keytap: tap enabled, run loop starting");
            ffi::CFRunLoopRun();
        }
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        unsafe {
            if !self.port.is_null() {
                ffi::CGEventTapEnable(self.port, false);
            }
            if !self.source.is_null() {
                ffi::CFRelease(self.source as ffi::CFTypeRef);
                self.source = ptr::null_mut();
            }
            if !self.port.is_null() {
                ffi::CFRelease(self.port as ffi::CFTypeRef);
                self.port = ptr::null_mut();
            }
            if !self.state.is_null() {
                drop(Box::from_raw(self.state));
                self.state = ptr::null_mut();
            }
        }
    }
}
