//! Hand-written CoreGraphics / CoreFoundation / ApplicationServices bindings.
//! No -sys crates. Declarations checked against the macOS SDK headers.
//!
//! Extern `static` items that are C pointers (`CFStringRef`, `CFBooleanRef`)
//! must be read by value. Extern `static` items that are C structs
//! (`CFDictionaryKeyCallBacks`) must be passed by address (`&static`).
//! Mixing those up, or building a CFDictionary with null key/value callbacks,
//! produces NULL CFType refs and crashes inside CFGetTypeID (see
//! keytap-2026-10-11-110248.ips).

#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code
)]

use std::ffi::c_void;

pub type CFIndex = isize;
pub type CFHashCode = usize;
pub type CFOptionFlags = usize;
pub type CFTimeInterval = f64;
pub type CGEventRef = *mut c_void;
pub type CGEventSourceRef = *mut c_void;
pub type CFMachPortRef = *mut c_void;
pub type CFRunLoopRef = *mut c_void;
pub type CFRunLoopSourceRef = *mut c_void;
pub type CFAllocatorRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFMutableDictionaryRef = *mut c_void;
pub type CFStringRef = *const c_void;
pub type CFBooleanRef = *const c_void;
pub type CFTypeRef = *const c_void;
pub type CGEventTapProxy = *mut c_void;
pub type CGEventTimestamp = u64;
/// MacTypes Boolean: unsigned char, not C99/Rust bool.
pub type Boolean = u8;

pub type CGEventMask = u64;
pub type CGEventFlags = u64;
pub type CGKeyCode = u16;
pub type CGEventField = u32;
pub type CGEventTapLocation = u32;
pub type CGEventTapPlacement = u32;
pub type CGEventTapOptions = u32;
pub type CGEventType = u32;
pub type CGEventSourceStateID = i32;

/// Matches CGEventTypes.h. Must be extern "C", not "C-unwind": the callback is
/// invoked from Apple code that is not exception-safe across the boundary.
pub type CGEventTapCallBack = Option<
    unsafe extern "C" fn(
        proxy: CGEventTapProxy,
        etype: CGEventType,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef,
>;

pub type CFDictionaryRetainCallBack =
    Option<unsafe extern "C" fn(allocator: CFAllocatorRef, value: *const c_void) -> *const c_void>;
pub type CFDictionaryReleaseCallBack =
    Option<unsafe extern "C" fn(allocator: CFAllocatorRef, value: *const c_void)>;
pub type CFDictionaryCopyDescriptionCallBack =
    Option<unsafe extern "C" fn(value: *const c_void) -> CFStringRef>;
pub type CFDictionaryEqualCallBack =
    Option<unsafe extern "C" fn(value1: *const c_void, value2: *const c_void) -> Boolean>;
pub type CFDictionaryHashCallBack =
    Option<unsafe extern "C" fn(value: *const c_void) -> CFHashCode>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CFDictionaryKeyCallBacks {
    pub version: CFIndex,
    pub retain: CFDictionaryRetainCallBack,
    pub release: CFDictionaryReleaseCallBack,
    pub copyDescription: CFDictionaryCopyDescriptionCallBack,
    pub equal: CFDictionaryEqualCallBack,
    pub hash: CFDictionaryHashCallBack,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CFDictionaryValueCallBacks {
    pub version: CFIndex,
    pub retain: CFDictionaryRetainCallBack,
    pub release: CFDictionaryReleaseCallBack,
    pub copyDescription: CFDictionaryCopyDescriptionCallBack,
    pub equal: CFDictionaryEqualCallBack,
}

pub const NULL: *mut c_void = core::ptr::null_mut();

// CGEventType
pub const kCGEventNull: CGEventType = 0;
pub const kCGEventLeftMouseDown: CGEventType = 1;
pub const kCGEventLeftMouseUp: CGEventType = 2;
pub const kCGEventLeftMouseDragged: CGEventType = 6;
pub const kCGEventKeyDown: CGEventType = 10;
pub const kCGEventKeyUp: CGEventType = 11;
pub const kCGEventFlagsChanged: CGEventType = 12;
pub const kCGEventTapDisabledByTimeout: CGEventType = 0xFFFFFFFE;
pub const kCGEventTapDisabledByUserInput: CGEventType = 0xFFFFFFFF;

// CGEventTapLocation
pub const kCGHIDEventTap: CGEventTapLocation = 0;
pub const kCGSessionEventTap: CGEventTapLocation = 1;
pub const kCGAnnotatedSessionEventTap: CGEventTapLocation = 2;

// CGEventTapPlacement
pub const kCGHeadInsertEventTap: CGEventTapPlacement = 0;
pub const kCGTailAppendEventTap: CGEventTapPlacement = 1;

// CGEventTapOptions
pub const kCGEventTapOptionDefault: CGEventTapOptions = 0x00000000;
pub const kCGEventTapOptionListenOnly: CGEventTapOptions = 0x00000001;

// CGEventFlags
pub const kCGEventFlagMaskAlphaShift: CGEventFlags = 0x00010000;
pub const kCGEventFlagMaskShift: CGEventFlags = 0x00020000;
pub const kCGEventFlagMaskControl: CGEventFlags = 0x00040000;
pub const kCGEventFlagMaskAlternate: CGEventFlags = 0x00080000;
pub const kCGEventFlagMaskCommand: CGEventFlags = 0x00100000;
pub const kCGEventFlagMaskNumericPad: CGEventFlags = 0x00200000;
pub const kCGEventFlagMaskSecondaryFn: CGEventFlags = 0x00800000;

// CGEventField
pub const kCGKeyboardEventKeycode: CGEventField = 9;
pub const kCGKeyboardEventAutorepeat: CGEventField = 8;
pub const kCGEventSourceUserData: CGEventField = 42;

// CGEventSourceStateID
pub const kCGEventSourceStateHIDSystemState: CGEventSourceStateID = 1;
pub const kCGEventSourceStatePrivate: CGEventSourceStateID = -1;

// CFRunLoop
pub const kCFRunLoopRunFinished: i32 = 1;
pub const kCFRunLoopRunStopped: i32 = 2;
pub const kCFRunLoopRunTimedOut: i32 = 3;
pub const kCFRunLoopRunHandledSource: i32 = 4;

pub const kCFStringEncodingUTF8: u32 = 0x08000100;

#[inline]
pub const fn cg_event_mask_bit(etype: CGEventType) -> CGEventMask {
    1u64 << (etype as u64)
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub fn CGEventTapCreate(
        tap: CGEventTapLocation,
        place: CGEventTapPlacement,
        options: CGEventTapOptions,
        eventsOfInterest: CGEventMask,
        callback: CGEventTapCallBack,
        userInfo: *mut c_void,
    ) -> CFMachPortRef;

    pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    pub fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;

    pub fn CGEventGetType(event: CGEventRef) -> CGEventType;
    pub fn CGEventGetFlags(event: CGEventRef) -> CGEventFlags;
    pub fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
    pub fn CGEventGetIntegerValueField(event: CGEventRef, field: CGEventField) -> i64;
    pub fn CGEventSetIntegerValueField(event: CGEventRef, field: CGEventField, value: i64);

    pub fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtualKey: CGKeyCode,
        keyDown: bool,
    ) -> CGEventRef;

    pub fn CGEventPost(tap: CGEventTapLocation, event: CGEventRef);
    pub fn CGEventTapPostEvent(proxy: CGEventTapProxy, event: CGEventRef);
    pub fn CGEventSourceCreate(stateID: CGEventSourceStateID) -> CGEventSourceRef;
    pub fn CGEventSetSource(event: CGEventRef, source: CGEventSourceRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;

    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopRun();
    pub fn CFRunLoopStop(rl: CFRunLoopRef);

    /// Pointer-typed static: read by value when passing to CFRunLoopAddSource.
    pub static kCFRunLoopCommonModes: CFStringRef;
    pub static kCFRunLoopDefaultMode: CFStringRef;

    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;

    pub fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        numValues: CFIndex,
        keyCallBacks: *const CFDictionaryKeyCallBacks,
        valueCallBacks: *const CFDictionaryValueCallBacks,
    ) -> CFDictionaryRef;

    /// Struct-typed statics: pass `&kCFTypeDictionaryKeyCallBacks`.
    pub static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
    pub static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;

    /// Pointer-typed statics: read by value (`kCFBooleanTrue`, not `&...`).
    pub static kCFBooleanTrue: CFBooleanRef;
    pub static kCFBooleanFalse: CFBooleanRef;

    pub fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        cStr: *const i8,
        encoding: u32,
    ) -> CFStringRef;

    pub fn CFGetTypeID(cf: CFTypeRef) -> usize;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    /// Returns MacTypes Boolean (u8), not Rust bool.
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
    /// Pointer-typed static: read by value.
    pub static kAXTrustedCheckOptionPrompt: CFStringRef;
}

/// Build `@{ kAXTrustedCheckOptionPrompt: prompt }` with CFType callbacks.
/// Caller must CFRelease the returned dictionary when non-null.
pub unsafe fn ax_trusted_options(prompt: bool) -> CFDictionaryRef {
    let key = unsafe { kAXTrustedCheckOptionPrompt };
    let value = unsafe {
        if prompt {
            kCFBooleanTrue
        } else {
            kCFBooleanFalse
        }
    };
    if key.is_null() || value.is_null() {
        return core::ptr::null();
    }
    let keys: [*const c_void; 1] = [key];
    let values: [*const c_void; 1] = [value];
    unsafe {
        CFDictionaryCreate(
            core::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            core::ptr::from_ref(&kCFTypeDictionaryKeyCallBacks),
            core::ptr::from_ref(&kCFTypeDictionaryValueCallBacks),
        )
    }
}

/// Check Accessibility trust. `prompt` asks macOS to show the consent dialog
/// asynchronously when untrusted. Always releases the options dictionary.
pub unsafe fn ax_is_process_trusted(prompt: bool) -> bool {
    let opts = unsafe { ax_trusted_options(prompt) };
    let trusted = unsafe { AXIsProcessTrustedWithOptions(opts) };
    if !opts.is_null() {
        unsafe { CFRelease(opts as CFTypeRef) };
    }
    trusted != 0
}
