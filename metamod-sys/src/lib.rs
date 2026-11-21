//! Metamod ABI generated from the pinned upstream headers at build time.
//! See the repository's licensing notes.
#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

use core::ffi::{CStr, c_int};

#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    clippy::type_complexity
)]
/// Native ABI descriptions. Pointer validity, alignment, initialization,
/// thread affinity, and call-specific lifetimes remain the caller's
/// responsibility when using these bindings.
pub mod raw {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use raw::{
    DLL_FUNCTIONS as DllFunctions, META_FUNCTIONS as MetaFunctions, META_RES as MetaResult,
    MetamodLog as Log, PL_UNLOAD_REASON as UnloadReason, PLUG_LOADTIME as LoadTime,
    enginefuncs_t as EngineFunctions, gamedll_funcs_t as GameDllFunctions,
    globalvars_t as EngineGlobals, meta_globals_t as MetaGlobals, mutil_funcs_t as MetaUtilities,
    plugin_info_t as PluginInfo,
};
pub use raw::{
    MRES_HANDLED, MRES_IGNORED, MRES_OVERRIDE, MRES_SUPERCEDE, MRES_UNSET, PNL_CMD_FORCED,
    PNL_COMMAND, PNL_DELAYED, PNL_FILE_NEWER, PNL_INI_DELETED, PNL_NULL, PNL_PLG_FORCED,
    PNL_PLUGIN, PNL_RELOAD, PT_ANYPAUSE, PT_ANYTIME, PT_CHANGELEVEL, PT_NEVER, PT_STARTUP,
};

pub const META_INTERFACE: &CStr = match CStr::from_bytes_with_nul(raw::META_INTERFACE_VERSION) {
    Ok(version) => version,
    Err(_) => panic!("invalid Metamod interface string in upstream header"),
};
pub const ENTITY_INTERFACE: c_int = raw::INTERFACE_VERSION as c_int;
