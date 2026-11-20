//! Implementation details for the export macro.
#![deny(unsafe_op_in_unsafe_fn)]

pub use metamod_sys::*;

use crate::{Context, Plugin};
use std::{
    cell::Cell,
    ffi::{CStr, c_char, c_int},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

#[derive(Clone, Copy)]
struct Services {
    id: *mut PluginInfo,
    log: Option<Log>,
    console: Option<Log>,
}

#[derive(Clone, Copy)]
enum State {
    Unqueried,
    Queried(Services),
    Attaching,
    Attached {
        services: Services,
        globals: *mut MetaGlobals,
        frame: Frame,
    },
}

#[derive(Clone, Copy)]
enum Frame {
    Ready,
    Running,
    Disabled,
}

thread_local! {
    static STATE: Cell<State> = const { Cell::new(State::Unqueried) };
}

fn contained<T>(callback: impl FnOnce() -> T) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(callback)) {
        Ok(value) => Some(value),
        Err(payload) => {
            // A custom panic payload can itself panic during Drop.
            std::mem::forget(payload);
            None
        }
    }
}

pub const fn info<P: Plugin>() -> PluginInfo {
    let metadata = P::METADATA;
    PluginInfo {
        interface: META_INTERFACE.as_ptr(),
        name: metadata.name.as_ptr(),
        version: metadata.version.as_ptr(),
        date: metadata.date.as_ptr(),
        author: metadata.author.as_ptr(),
        url: metadata.url.as_ptr(),
        log_tag: metadata.log_tag.as_ptr(),
        loadable: PT_STARTUP,
        unloadable: PT_NEVER,
    }
}

fn compatible(version: &CStr) -> bool {
    let Ok(version) = version.to_str() else {
        return false;
    };
    let Some((major, minor)) = version.split_once(':') else {
        return false;
    };
    let number = |part: &str| {
        part.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| part.parse::<u32>().ok())
            .flatten()
    };
    matches!((number(major), number(minor)), (Some(5), Some(13..)))
}

/// # Safety
/// Metamod must supply a readable C string and utility table, a writable
/// output pointer, and the plugin's module-lifetime metadata.
pub unsafe fn query(
    version: *const c_char,
    out: *mut *mut PluginInfo,
    utilities: *mut MetaUtilities,
    id: *mut PluginInfo,
) -> c_int {
    if version.is_null() || out.is_null() || utilities.is_null() || id.is_null() {
        return 0;
    }
    contained(|| {
        // SAFETY: Validity follows from the loader contract above.
        if !compatible(unsafe { CStr::from_ptr(version) }) {
            return 0;
        }
        STATE.with(|state| {
            if !matches!(state.get(), State::Unqueried | State::Queried(_)) {
                return 0;
            }
            // SAFETY: The utility table is readable and the output is writable.
            let log = unsafe { ptr::addr_of!((*utilities).log_message).read() };
            let console = unsafe { ptr::addr_of!((*utilities).log_console).read() };
            unsafe { out.write(id) };
            state.set(State::Queried(Services { id, log, console }));
            1
        })
    })
    .unwrap_or(0)
}

/// # Safety
/// Called on the server thread that queried the plugin. Metamod must supply
/// a writable table and keep globals valid for all subsequent hook calls.
pub unsafe fn attach<P: Plugin>(
    now: LoadTime,
    table: *mut MetaFunctions,
    globals: *mut MetaGlobals,
) -> c_int {
    if now != PT_STARTUP || table.is_null() || globals.is_null() {
        return 0;
    }
    contained(|| {
        STATE.with(|state| {
            let State::Queried(services) = state.get() else {
                return 0;
            };
            state.set(State::Attaching);
            let scope = ();
            let context = Context::new(&scope, services.id, services.log, services.console);
            if contained(|| P::initialize(&context)) != Some(true) {
                state.set(State::Queried(services));
                return 0;
            }
            // SAFETY: Metamod provided a full writable output table.
            unsafe {
                table.write(MetaFunctions {
                    entity_api2: Some(entity_api::<P>),
                    ..Default::default()
                });
            }
            state.set(State::Attached {
                services,
                globals,
                frame: Frame::Ready,
            });
            1
        })
    })
    .unwrap_or(0)
}

unsafe extern "C" fn entity_api<P: Plugin>(table: *mut DllFunctions, version: *mut c_int) -> c_int {
    if table.is_null() || version.is_null() {
        return 0;
    }
    // SAFETY: GetEntityAPI2 supplies valid in/out version storage and a
    // writable DLL_FUNCTIONS table on successful negotiation.
    unsafe {
        if version.read() != ENTITY_INTERFACE {
            version.write(ENTITY_INTERFACE);
            return 0;
        }
        let mut hooks = DllFunctions::default();
        hooks.start_frame = Some(start_frame::<P>);
        table.write(hooks);
    }
    1
}

unsafe extern "C" fn start_frame<P: Plugin>() {
    contained(|| {
        STATE.with(|state| {
            let State::Attached {
                services,
                globals,
                frame,
            } = state.get()
            else {
                return;
            };
            if matches!(frame, Frame::Ready) {
                state.set(State::Attached {
                    services,
                    globals,
                    frame: Frame::Running,
                });
                let scope = ();
                let context = Context::new(&scope, services.id, services.log, services.console);
                let frame = if contained(|| P::start_frame(&context)).is_some() {
                    Frame::Ready
                } else {
                    Frame::Disabled
                };
                state.set(State::Attached {
                    services,
                    globals,
                    frame,
                });
            }
            // SAFETY: Attached globals are valid on this thread. Write after
            // user code so nested dispatch cannot leave a stale result.
            unsafe { ptr::addr_of_mut!((*globals).result).write(MRES_IGNORED) };
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Metadata;

    #[test]
    fn recursive_frame_is_skipped_and_panicking_frame_is_disabled() {
        struct TestPlugin;

        impl Plugin for TestPlugin {
            const METADATA: Metadata = Metadata {
                name: c"test",
                version: c"",
                date: c"",
                author: c"",
                url: c"",
                log_tag: c"",
            };

            fn start_frame(_: &Context<'_>) {
                CALLS.with(|calls| {
                    let count = calls.get() + 1;
                    calls.set(count);
                    if count == 1 {
                        // SAFETY: This test installed live globals on this thread.
                        unsafe { start_frame::<Self>() };
                        panic!("disable this frame callback");
                    }
                });
            }
        }

        thread_local! {
            static CALLS: Cell<usize> = const { Cell::new(0) };
        }

        let mut globals = MetaGlobals {
            result: MRES_UNSET,
            previous: MRES_HANDLED,
            status: MRES_OVERRIDE,
            original_return: ptr::null_mut(),
            override_return: ptr::null_mut(),
        };
        STATE.set(State::Attached {
            services: Services {
                id: ptr::null_mut(),
                log: None,
                console: None,
            },
            globals: &raw mut globals,
            frame: Frame::Ready,
        });
        for _ in 0..2 {
            globals.result = MRES_UNSET;
            // SAFETY: The attached globals remain live until state is cleared.
            unsafe { start_frame::<TestPlugin>() };
            assert_eq!(globals.result, MRES_IGNORED);
            assert_eq!(globals.previous, MRES_HANDLED);
            assert_eq!(globals.status, MRES_OVERRIDE);
        }
        STATE.set(State::Unqueried);
        CALLS.with(|calls| assert_eq!(calls.get(), 1));
    }
}
