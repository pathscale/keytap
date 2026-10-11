//! Hand-written CoreGraphics / CoreFoundation / ApplicationServices bindings.
//! No -sys crates. Declarations checked against the macOS SDK headers.

#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code
)]

use std::ffi::c_void;

pub type CFIndex = isize;
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
pub type CFTypeRef = *const c_void;
pub type CGEventTapProxy = *mut c_void;
pub type CGEventTimestamp = u64;

pub type CGEventMask = u64;
pub type CGEventFlags = u64;
pub type CGKeyCode = u16;
pub type CGEventField = u32;
pub type CGEventTapLocation = u32;
pub type CGEventTapPlacement = u32;
pub type CGEventTapOptions = u32;
pub type CGEventType = u32;
pub type CGEventSourceStateID = i32;

pub type CGEventTapCallBack = Option<
    unsafe extern "C-unwind" fn(
        proxy: CGEventTapProxy,
        etype: CGEventType,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef,
>;

pub const NULL: *mut c_void = core::ptr::null_mut();

// CGEventType
pub const kCGEventNull: CGEventType = 0;
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

// CGEventField
pub const kCGKeyboardEventKeycode: CGEventField = 9;
pub const kCGKeyboardEventAutorepeat: CGEventField = 8;

// CGEventSourceStateID
pub const kCGEventSourceStateHIDSystemState: CGEventSourceStateID = 1;
pub const kCGEventSourceStatePrivate: CGEventSourceStateID = -1;

// CFRunLoop
pub const kCFRunLoopRunFinished: i32 = 1;
pub const kCFRunLoopRunStopped: i32 = 2;
pub const kCFRunLoopRunTimedOut: i32 = 3;
pub const kCFRunLoopRunHandledSource: i32 = 4;

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

    pub static kCFRunLoopCommonModes: CFStringRef;
    pub static kCFRunLoopDefaultMode: CFStringRef;

    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;

    pub fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        numValues: CFIndex,
        keyCallbacks: *const c_void,
        valueCallbacks: *const c_void,
    ) -> CFDictionaryRef;

    pub fn CFBooleanGetTypeID() -> usize;
    pub static kCFBooleanTrue: *const c_void;
    pub static kCFBooleanFalse: *const c_void;

    pub fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        cStr: *const i8,
        encoding: u32,
    ) -> CFStringRef;
}

pub const kCFStringEncodingUTF8: u32 = 0x08000100;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    /// Prompt / check Accessibility trust. options may contain
    /// kAXTrustedCheckOptionPrompt = true.
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

/// Build the CFDictionary @{ kAXTrustedCheckOptionPrompt: true }.
pub unsafe fn ax_prompt_options() -> CFDictionaryRef {
    // kAXTrustedCheckOptionPrompt is CFSTR("AXTrustedCheckOptionPrompt")
    let key_c = b"AXTrustedCheckOptionPrompt\0";
    let key = unsafe {
        CFStringCreateWithCString(
            core::ptr::null(),
            key_c.as_ptr() as *const i8,
            kCFStringEncodingUTF8,
        )
    };
    let keys: [*const c_void; 1] = [key];
    let values: [*const c_void; 1] = [unsafe { kCFBooleanTrue }];
    unsafe {
        CFDictionaryCreate(
            core::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            // Use default CFType callbacks by passing null: for a short-lived
            // options dict on the calling thread this is acceptable; the
            // strings/booleans outlive the call.
            core::ptr::null(),
            core::ptr::null(),
        )
    }
}
