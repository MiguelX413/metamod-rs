//! Startup-only stateful GoldSrc Metamod plugins with typed hooks.

use std::ffi::CStr;

#[doc(hidden)]
pub mod __ffi;
mod engine;
mod hooks;
mod runtime;
mod trace;

pub use engine::{Command, ConsoleName, CvarValue, EngineError, PlayerInfo};
pub use hooks::{HookResult, KeyValueEvent, PostHookResult};
pub use trace::{
    EntityError, EntityHandle, EntityInfo, EntityKeyValues, EntityKind, EntitySpatial, Hull,
    TraceError, TraceMonsters, TraceOptions, TracePlane, TraceResult,
};

pub use runtime::{
    Context, InvalidPlayerSlot, InvalidPlayerUserId, ParsePlayerNumberError, PlayerId, PlayerSlot,
    PlayerUserId, Server,
};

#[cfg(feature = "macros")]
pub use metamod_macros::metamod_plugin;

pub mod prelude {
    #[cfg(feature = "macros")]
    pub use crate::metamod_plugin;
    pub use crate::{
        Command, ConsoleName, Context, CvarValue, EngineError, EntityError, EntityHandle,
        EntityInfo, EntityKeyValues, EntityKind, EntitySpatial, HookResult, Hull, KeyValueEvent,
        Metadata, PlayerId, PlayerInfo, PlayerSlot, PlayerUserId, Plugin, PostHookResult, Server,
        TraceError, TraceMonsters, TraceOptions, TracePlane, TraceResult, export_plugin,
    };
}

/// Strings remain available to Metamod for the lifetime of the module.
pub struct Metadata {
    pub name: &'static CStr,
    pub version: &'static CStr,
    pub date: &'static CStr,
    pub author: &'static CStr,
    pub url: &'static CStr,
    pub log_tag: &'static CStr,
}

/// One server-thread-owned instance per successful startup attachment.
pub trait Plugin: Default + 'static {
    const METADATA: Metadata;

    /// Return false to decline attachment and drop this instance.
    fn initialize(&mut self, _context: &mut Context<'_>) -> bool {
        true
    }

    /// Best-effort cleanup when Metamod forwards the game-DLL shutdown hook.
    /// The SDK drops this instance afterward. Engine access is unavailable.
    fn shutdown(&mut self) {}

    /// Handle a command registered through `Server::register_command`.
    fn server_command(&mut self, _context: &mut Context<'_>, _command: &Command) {}

    /// Register the pre and post lag compensation hooks independently.
    const LAG_COMPENSATION_PRE: bool = false;
    const LAG_COMPENSATION_POST: bool = false;

    fn allow_lag_compensation(&mut self, _context: &mut Context<'_>) -> HookResult<bool> {
        HookResult::Ignored
    }

    /// Metamod's reported original result. A superceded call has no original.
    fn allow_lag_compensation_post(
        &mut self,
        _context: &mut Context<'_>,
        _original: Option<bool>,
    ) -> PostHookResult<bool> {
        PostHookResult::Ignored
    }

    /// Opt into observing GameDLL keyvalue inputs, including before map activation.
    const KEYVALUE_PRE: bool = false;

    /// Observe copied inputs. Reentrant observations may be delivered later.
    fn key_value(&mut self, _context: &mut Context<'_>, _event: &KeyValueEvent) {}

    fn start_frame(&mut self, _context: &mut Context<'_>) {}

    /// Called after the game DLL activates the map.
    fn server_activate(&mut self, _context: &mut Context<'_>) {}

    /// Called before the game DLL deactivates the map.
    fn server_deactivate(&mut self, _context: &mut Context<'_>) {}

    /// Observes a session after the game DLL puts the player into the server.
    fn player_join(&mut self, _context: &mut Context<'_>, _player: PlayerId) {}

    /// Observes a session ending. The ID is already invalidated.
    fn player_disconnect(&mut self, _context: &mut Context<'_>, _player: PlayerId) {}
}

/// Put this once at the root of the plugin's cdylib crate.
///
/// ```
/// use metamod_rs::prelude::*;
///
/// #[derive(Default)]
/// struct MyPlugin;
///
/// impl Plugin for MyPlugin {
///     const METADATA: Metadata = Metadata {
///         name: c"My Plugin",
///         version: c"0.1.0",
///         author: c"Developer",
///         date: c"",
///         url: c"",
///         log_tag: c"MYPLUGIN",
///     };
/// }
///
/// export_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        use $crate::__ffi as __metamod_ffi;

        static mut __METAMOD_INFO: __metamod_ffi::PluginInfo = __metamod_ffi::info::<$plugin>();

        #[cfg(all(windows, target_arch = "x86"))]
        #[unsafe(no_mangle)]
        unsafe extern "stdcall" fn GiveFnptrsToDll(
            engine: *mut __metamod_ffi::EngineFunctions,
            globals: *mut __metamod_ffi::EngineGlobals,
        ) {
            // SAFETY: The loader supplies the engine function table and globals.
            unsafe { __metamod_ffi::give_engine(engine, globals) };
        }

        #[cfg(not(all(windows, target_arch = "x86")))]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn GiveFnptrsToDll(
            engine: *mut __metamod_ffi::EngineFunctions,
            globals: *mut __metamod_ffi::EngineGlobals,
        ) {
            // SAFETY: The loader supplies the engine function table and globals.
            unsafe { __metamod_ffi::give_engine(engine, globals) };
        }

        #[unsafe(no_mangle)]
        unsafe extern "C" fn Meta_Query(
            version: *const ::core::ffi::c_char,
            out: *mut *mut __metamod_ffi::PluginInfo,
            utilities: *mut __metamod_ffi::MetaUtilities,
        ) -> ::core::ffi::c_int {
            // SAFETY: The loader supplies the C ABI arguments.
            unsafe { __metamod_ffi::query(version, out, utilities, &raw mut __METAMOD_INFO) }
        }

        #[unsafe(no_mangle)]
        unsafe extern "C" fn Meta_Attach(
            now: __metamod_ffi::LoadTime,
            table: *mut __metamod_ffi::MetaFunctions,
            globals: *mut __metamod_ffi::MetaGlobals,
            _: *mut __metamod_ffi::GameDllFunctions,
        ) -> ::core::ffi::c_int {
            // SAFETY: The loader supplies the C ABI arguments.
            unsafe { __metamod_ffi::attach::<$plugin>(now, table, globals) }
        }

        #[unsafe(no_mangle)]
        unsafe extern "C" fn Meta_Detach(
            _: __metamod_ffi::LoadTime,
            _: __metamod_ffi::UnloadReason,
        ) -> ::core::ffi::c_int {
            0
        }

        // Check exported signatures against bindgen's upstream declarations.
        const _: __metamod_ffi::raw::GIVE_ENGINE_FUNCTIONS_FN = Some(GiveFnptrsToDll);
        const _: __metamod_ffi::raw::META_QUERY_FN = Some(Meta_Query);
        const _: __metamod_ffi::raw::META_ATTACH_FN = Some(Meta_Attach);
        const _: __metamod_ffi::raw::META_DETACH_FN = Some(Meta_Detach);
    };
}
