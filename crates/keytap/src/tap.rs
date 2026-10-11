//! CoreGraphics event tap: convert CG events to rules::Event, apply Engine, post.

use crate::ffi::{
    self, CFMachPortRef, CFRunLoopSourceRef, CGEventFlags, CGEventMask, CGEventRef,
    CGEventTapProxy, CGEventType,
};
use keytap_rules::{Action, Engine, Event, Flags, Key, Phase};
use std::ffi::c_void;
use std::ptr;

/// Heap state shared with the C callback. Owned by `run()` via Box; the tap
/// holds only a raw pointer. The CFRunLoop delivers callbacks serially on the
/// same thread that called `run()`, so no locks or atomics are required.
pub struct TapState {
    pub engine: Engine,
    pub port: CFMachPortRef,
    pub source: Option<CGEventSource>,
    pub dry_run: bool,
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
    // Preserve unknown high bits (device-dependent) from the base event, then
    // overlay the modifiers we manage.
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

fn phase_from_cg(t: CGEventType) -> Option<Phase> {
    match t {
        ffi::kCGEventKeyDown => Some(Phase::KeyDown),
        ffi::kCGEventKeyUp => Some(Phase::KeyUp),
        ffi::kCGEventFlagsChanged => Some(Phase::FlagsChanged),
        _ => None,
    }
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

fn post_event(state: &TapState, ev: &Event) {
    if state.dry_run {
        eprintln!("dry-run post {}", ev.render());
        return;
    }
    let key_down = match ev.phase {
        Phase::KeyDown => true,
        Phase::KeyUp => false,
        Phase::FlagsChanged => {
            // Represent flags-changed as a key-up/down of the modifier key with
            // the desired flags. Creating a keyboard event and setting flags is
            // enough for the switcher end path (we post KeyUp for command).
            false
        }
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
        ffi::CGEventSetFlags(cg, flags_to_cg(ev.flags, 0));
        // FlagsChanged: set type explicitly if we ever post those. For KeyUp
        // of modifiers, a keyboard event with keyDown=false is correct.
        ffi::CGEventPost(ffi::kCGSessionEventTap, cg);
        ffi::CFRelease(cg as ffi::CFTypeRef);
    }
}

/// Apply actions. Returns the event the callback should hand back to the system
/// (null = suppress original).
fn apply_actions(state: &mut TapState, original: CGEventRef, actions: &[Action]) -> CGEventRef {
    if state.dry_run {
        for a in actions {
            eprintln!("dry-run {}", a.render());
        }
        // Never suppress or rewrite while dry-running; the user may be typing.
        return original;
    }
    let mut pass_back: CGEventRef = ptr::null_mut();
    for a in actions {
        match a {
            Action::Suppress => {}
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
            }
            Action::Post(ev) => post_event(state, ev),
        }
    }
    pass_back
}

pub unsafe extern "C" fn tap_callback(
    _proxy: CGEventTapProxy,
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
        }
        return event;
    }

    let Some(ev) = event_from_cg(event) else {
        return event;
    };

    let actions = state.engine.handle(ev);
    apply_actions(state, event, &actions)
}

pub fn event_mask() -> CGEventMask {
    ffi::cg_event_mask_bit(ffi::kCGEventKeyDown)
        | ffi::cg_event_mask_bit(ffi::kCGEventKeyUp)
        | ffi::cg_event_mask_bit(ffi::kCGEventFlagsChanged)
}

pub struct Tap {
    port: CFMachPortRef,
    source: CFRunLoopSourceRef,
    /// Pinned state; pointer passed to callback.
    state: *mut TapState,
}

impl Tap {
    pub fn create(engine: Engine, dry_run: bool) -> Result<Self, String> {
        let source = CGEventSource::new_private();
        let state = Box::new(TapState {
            engine,
            port: ptr::null_mut(),
            source,
            dry_run,
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
            // reclaim state
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
