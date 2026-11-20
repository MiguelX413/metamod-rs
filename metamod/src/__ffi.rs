//! Implementation details for the export macro.
#![deny(unsafe_op_in_unsafe_fn)]

pub use metamod_sys::*;

use crate::{Context, PlayerId, Plugin};
use raw::{MetamodIndexOfEdict, MetamodServerPrint, edict_t};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    ffi::{CStr, c_char, c_int},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    sync::OnceLock,
    thread::{self, ThreadId},
};

#[derive(Clone, Copy)]
struct Services {
    id: *mut PluginInfo,
    log: Log,
    console: Log,
}

#[derive(Clone, Copy)]
enum State {
    Unqueried,
    Queried(Services),
    Attaching,
    Attached {
        services: Services,
        globals: *mut MetaGlobals,
        disabled: bool,
    },
}

#[derive(Clone, Copy)]
struct Engine {
    print: MetamodServerPrint,
    index: MetamodIndexOfEdict,
}

static OWNER: OnceLock<ThreadId> = OnceLock::new();

thread_local! {
    static STATE: Cell<State> = const { Cell::new(State::Unqueried) };
    static ENGINE: Cell<Engine> = const { Cell::new(Engine { print: None, index: None }) };
    static INSTANCE: RefCell<Option<&'static mut dyn Any>> = const { RefCell::new(None) };
}

/// # Safety
/// A non-null table must have readable engine callback slots. They must
/// remain callable on this thread through the lifetime of the attachment.
pub unsafe fn give_engine(table: *mut EngineFunctions) {
    let _ = contained(|| {
        if table.is_null() || !STATE.with(|state| matches!(state.get(), State::Unqueried)) {
            return;
        }
        // SAFETY: The loader supplies these initialized slots. Copy them
        // without borrowing the table, which may be shared by the loader.
        let engine = unsafe {
            Engine {
                print: ptr::addr_of!((*table).pfnServerPrint).read(),
                index: ptr::addr_of!((*table).pfnIndexOfEdict).read(),
            }
        };
        ENGINE.set(engine);
    });
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
        ifvers: META_INTERFACE.as_ptr(),
        name: metadata.name.as_ptr(),
        version: metadata.version.as_ptr(),
        date: metadata.date.as_ptr(),
        author: metadata.author.as_ptr(),
        url: metadata.url.as_ptr(),
        logtag: metadata.log_tag.as_ptr(),
        loadable: PT_STARTUP,
        unloadable: PT_NEVER,
    }
}

fn interface_version(version: &CStr) -> Option<(u32, u32)> {
    let (major, minor) = version.to_str().ok()?.split_once(':')?;
    let number = |part: &str| {
        part.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| part.parse::<u32>().ok())
            .flatten()
    };
    Some((number(major)?, number(minor)?))
}

fn compatible(version: &CStr) -> bool {
    match (
        interface_version(version),
        interface_version(META_INTERFACE),
    ) {
        (Some((major, minor)), Some((required_major, required_minor))) => {
            major == required_major && minor >= required_minor
        }
        _ => false,
    }
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
        let current = thread::current().id();
        if OWNER.get_or_init(|| current) != &current {
            return 0;
        }
        STATE.with(|state| {
            if !matches!(state.get(), State::Unqueried | State::Queried(_)) {
                return 0;
            }
            // SAFETY: The utility table is readable and the output is writable.
            let log = unsafe { ptr::addr_of!((*utilities).pfnLogMessage).read() };
            let console = unsafe { ptr::addr_of!((*utilities).pfnLogConsole).read() };
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
            let Some(mut plugin) = contained(|| Box::new(P::default())) else {
                state.set(State::Queried(services));
                return 0;
            };
            let scope = ();
            let mut context = make_context(&scope, services);
            if contained(|| plugin.initialize(&mut context)) != Some(true) {
                // Drop separately so a destructor panic cannot collide with
                // an unwind from initialization or escape into Metamod.
                let _ = contained(|| drop(plugin));
                state.set(State::Queried(services));
                return 0;
            }

            // Metamod has no reliable shutdown notification for a resident
            // plugin. Keep this instance alive until process exit, without a
            // TLS destructor that could run after the engine shuts down.
            INSTANCE.with(|instance| instance.replace(Some(Box::leak(plugin))));
            // SAFETY: Metamod provided a full writable output table.
            unsafe {
                table.write(MetaFunctions {
                    pfnGetEntityAPI2: Some(entity_api::<P>),
                    pfnGetEntityAPI2_Post: Some(entity_api_post::<P>),
                    ..Default::default()
                });
            }
            state.set(State::Attached {
                services,
                globals,
                disabled: false,
            });
            1
        })
    })
    .unwrap_or(0)
}

fn make_context(scope: &(), services: Services) -> Context<'_> {
    Context::new(
        scope,
        services.id,
        services.log,
        services.console,
        ENGINE.get().print,
    )
}

unsafe fn write_hooks(table: *mut DllFunctions, version: *mut c_int, hooks: DllFunctions) -> c_int {
    if table.is_null() || version.is_null() {
        return 0;
    }
    // SAFETY: Metamod provides valid version storage and a writable table.
    unsafe {
        if version.read() != ENTITY_INTERFACE {
            version.write(ENTITY_INTERFACE);
            return 0;
        }
        table.write(hooks);
    }
    1
}

unsafe extern "C" fn entity_api<P: Plugin>(table: *mut DllFunctions, version: *mut c_int) -> c_int {
    // SAFETY: Metamod supplies the API negotiation arguments.
    unsafe {
        write_hooks(
            table,
            version,
            DllFunctions {
                pfnStartFrame: Some(start_frame::<P>),
                pfnServerDeactivate: Some(server_deactivate::<P>),
                pfnClientDisconnect: Some(player_disconnect::<P>),
                ..Default::default()
            },
        )
    }
}

unsafe extern "C" fn entity_api_post<P: Plugin>(
    table: *mut DllFunctions,
    version: *mut c_int,
) -> c_int {
    // SAFETY: Metamod supplies the API negotiation arguments.
    unsafe {
        write_hooks(
            table,
            version,
            DllFunctions {
                pfnServerActivate: Some(server_activate::<P>),
                pfnClientPutInServer: Some(player_join::<P>),
                ..Default::default()
            },
        )
    }
}

fn dispatch<P: Plugin>(callback: impl FnOnce(&mut P, &mut Context<'_>)) {
    let _ = contained(|| {
        STATE.with(|state| {
            let State::Attached {
                services,
                globals,
                disabled,
            } = state.get()
            else {
                // An off-thread callback has no attachment and must not
                // access the server thread's state or Metamod globals.
                return;
            };
            if !disabled {
                INSTANCE.with(|instance| {
                    if let Ok(mut slot) = instance.try_borrow_mut()
                        && let Some(plugin) = slot
                            .as_deref_mut()
                            .and_then(|plugin| plugin.downcast_mut::<P>())
                    {
                        let scope = ();
                        let mut context = make_context(&scope, services);
                        if contained(|| callback(plugin, &mut context)).is_none() {
                            state.set(State::Attached {
                                services,
                                globals,
                                disabled: true,
                            });
                        }
                    }
                });
            }
            // SAFETY: Globals stay live on the server thread while attached.
            // Also restore the result after recursive or disabled dispatch.
            unsafe { ptr::addr_of_mut!((*globals).mres).write(MRES_IGNORED) };
        });
    });
}

unsafe extern "C" fn start_frame<P: Plugin>() {
    dispatch(P::start_frame);
}

unsafe extern "C" fn server_activate<P: Plugin>(_: *mut edict_t, _: c_int, _: c_int) {
    dispatch(P::server_activate);
}

unsafe extern "C" fn server_deactivate<P: Plugin>() {
    dispatch(P::server_deactivate);
}

fn dispatch_player<P: Plugin>(
    edict: *mut edict_t,
    callback: impl FnOnce(&mut P, &mut Context<'_>, PlayerId),
) {
    dispatch(|plugin, context| {
        if let Some(index) = ENGINE.get().index
            && !edict.is_null()
            // SAFETY: Metamod supplied a live player edict for this callback.
            && let Some(player) = PlayerId::from_index(unsafe { index(edict) })
        {
            callback(plugin, context, player);
        }
    });
}

unsafe extern "C" fn player_join<P: Plugin>(edict: *mut edict_t) {
    dispatch_player(edict, P::player_join);
}

unsafe extern "C" fn player_disconnect<P: Plugin>(edict: *mut edict_t) {
    dispatch_player(edict, P::player_disconnect);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Metadata;

    const METADATA: Metadata = Metadata {
        name: c"test",
        version: c"",
        date: c"",
        author: c"",
        url: c"",
        log_tag: c"",
    };

    fn queried() {
        STATE.set(State::Queried(Services {
            id: ptr::null_mut(),
            log: None,
            console: None,
        }));
    }

    #[test]
    fn recursive_and_off_thread_dispatch_is_skipped_and_panic_disables_all_hooks() {
        #[derive(Default)]
        struct TestPlugin {
            calls: usize,
        }

        impl Plugin for TestPlugin {
            const METADATA: Metadata = METADATA;

            fn start_frame(&mut self, _: &mut Context<'_>) {
                self.calls += 1;
                dispatch::<Self>(|plugin, _| plugin.calls += 100);
                panic!("disable all callbacks");
            }
        }

        queried();
        let mut globals = MetaGlobals {
            prev_mres: MRES_HANDLED,
            ..Default::default()
        };
        let mut functions = MetaFunctions::default();
        // SAFETY: This test keeps its foreign output storage live on this thread.
        assert_eq!(
            unsafe { attach::<TestPlugin>(PT_STARTUP, &mut functions, &mut globals) },
            1
        );
        let off_thread_ran = thread::spawn(|| {
            let mut ran = false;
            dispatch::<TestPlugin>(|_, _| ran = true);
            ran
        })
        .join()
        .unwrap();
        assert!(!off_thread_ran);
        dispatch::<TestPlugin>(|plugin, ctx| plugin.start_frame(ctx));
        assert_eq!(globals.mres, MRES_IGNORED);
        assert_eq!(globals.prev_mres, MRES_HANDLED);
        assert!(matches!(
            STATE.get(),
            State::Attached { disabled: true, .. }
        ));
        globals.mres = MRES_UNSET;
        dispatch::<TestPlugin>(|plugin, _| plugin.calls += 1);
        assert_eq!(globals.mres, MRES_IGNORED);
        INSTANCE.with(|instance| {
            let mut slot = instance.borrow_mut();
            let plugin = slot
                .as_deref_mut()
                .unwrap()
                .downcast_mut::<TestPlugin>()
                .unwrap();
            assert_eq!(plugin.calls, 1);
        });
        STATE.set(State::Unqueried);
        INSTANCE.with(|instance| instance.take());
    }

    #[test]
    fn failed_attach_drops_instance_and_allows_retry() {
        #[derive(Default)]
        struct TestPlugin;

        thread_local! {
            static DROPS: Cell<usize> = const { Cell::new(0) };
        }

        impl Plugin for TestPlugin {
            const METADATA: Metadata = METADATA;

            fn initialize(&mut self, _: &mut Context<'_>) -> bool {
                if DROPS.get() == 0 {
                    panic!("initialization failed");
                }
                false
            }
        }

        impl Drop for TestPlugin {
            fn drop(&mut self) {
                DROPS.set(DROPS.get() + 1);
                panic!("destructor panic");
            }
        }

        queried();
        let mut globals = MetaGlobals::default();
        let mut functions = MetaFunctions::default();
        for drops in 1..=2 {
            // SAFETY: The output pointers remain live during the call.
            assert_eq!(
                unsafe { attach::<TestPlugin>(PT_STARTUP, &mut functions, &mut globals) },
                0
            );
            assert_eq!(DROPS.get(), drops);
            assert!(matches!(STATE.get(), State::Queried(_)));
            assert!(functions.pfnGetEntityAPI2.is_none());
        }
        STATE.set(State::Unqueried);
    }
}
