//! Minimal Metamod 5:13 ABI. See the repository's licensing notes.
#![no_std]

use core::ffi::{CStr, c_char, c_int, c_void};

pub const META_INTERFACE: &CStr = c"5:13";
pub const ENTITY_INTERFACE: c_int = 140;

// Represent C enums as integers so foreign values remain valid Rust values.
pub type LoadTime = c_int;
pub type UnloadReason = c_int;
pub type MetaResult = c_int;
pub const PT_NEVER: LoadTime = 0;
pub const PT_STARTUP: LoadTime = 1;
pub const PT_CHANGELEVEL: LoadTime = 2;
pub const PT_ANYTIME: LoadTime = 3;
pub const PT_ANYPAUSE: LoadTime = 4;

pub const PNL_NULL: UnloadReason = 0;
pub const PNL_INI_DELETED: UnloadReason = 1;
pub const PNL_FILE_NEWER: UnloadReason = 2;
pub const PNL_COMMAND: UnloadReason = 3;
pub const PNL_CMD_FORCED: UnloadReason = 4;
pub const PNL_DELAYED: UnloadReason = 5;
pub const PNL_PLUGIN: UnloadReason = 6;
pub const PNL_PLG_FORCED: UnloadReason = 7;
pub const PNL_RELOAD: UnloadReason = 8;

pub const MRES_UNSET: MetaResult = 0;
pub const MRES_IGNORED: MetaResult = 1;
pub const MRES_HANDLED: MetaResult = 2;
pub const MRES_OVERRIDE: MetaResult = 3;
pub const MRES_SUPERCEDE: MetaResult = 4;

#[repr(C)]
pub struct PluginInfo {
    pub interface: *const c_char,
    pub name: *const c_char,
    pub version: *const c_char,
    pub date: *const c_char,
    pub author: *const c_char,
    pub url: *const c_char,
    pub log_tag: *const c_char,
    pub loadable: LoadTime,
    pub unloadable: LoadTime,
}

#[repr(C)]
pub struct MetaGlobals {
    pub result: MetaResult,
    pub previous: MetaResult,
    pub status: MetaResult,
    pub original_return: *mut c_void,
    pub override_return: *mut c_void,
}

// Unused signatures are erased. These slots are private and are only
// initialized to None. Supported targets use one representation for C
// function pointers, regardless of signature.
type Slot = Option<unsafe extern "C" fn()>;

#[repr(C)]
#[derive(Default)]
pub struct DllFunctions {
    before_start_frame: [Slot; 25],
    pub start_frame: Slot,
    after_start_frame: [Slot; 24],
}

macro_rules! opaque {
    ($($name:ident),+ $(,)?) => {
        $(#[repr(C)] pub struct $name { _private: [u8; 0] })+
    };
}

opaque!(EngineFunctions, EngineGlobals, NewDllFunctions);

#[repr(C)]
pub struct GameDllFunctions {
    pub dll: *mut DllFunctions,
    pub new_dll: *mut NewDllFunctions,
}

pub type Log = unsafe extern "C" fn(*mut PluginInfo, *const c_char, ...);

#[repr(C)]
pub struct MetaUtilities {
    pub log_console: Option<Log>,
    pub log_message: Option<Log>,
    pub log_error: Option<Log>,
    pub log_developer: Option<Log>,
    remaining: [Slot; 14],
}

pub type GetEntityApi = unsafe extern "C" fn(*mut DllFunctions, c_int) -> c_int;
pub type GetEntityApi2 = unsafe extern "C" fn(*mut DllFunctions, *mut c_int) -> c_int;
pub type GetNewDll = unsafe extern "C" fn(*mut NewDllFunctions, *mut c_int) -> c_int;
pub type GetEngine = unsafe extern "C" fn(*mut EngineFunctions, *mut c_int) -> c_int;

#[repr(C)]
#[derive(Default)]
pub struct MetaFunctions {
    pub entity_api: Option<GetEntityApi>,
    pub entity_api_post: Option<GetEntityApi>,
    pub entity_api2: Option<GetEntityApi2>,
    pub entity_api2_post: Option<GetEntityApi2>,
    pub new_dll: Option<GetNewDll>,
    pub new_dll_post: Option<GetNewDll>,
    pub engine: Option<GetEngine>,
    pub engine_post: Option<GetEngine>,
}

pub type MetaQuery =
    unsafe extern "C" fn(*const c_char, *mut *mut PluginInfo, *mut MetaUtilities) -> c_int;
pub type MetaAttach = unsafe extern "C" fn(
    LoadTime,
    *mut MetaFunctions,
    *mut MetaGlobals,
    *mut GameDllFunctions,
) -> c_int;
pub type MetaDetach = unsafe extern "C" fn(LoadTime, UnloadReason) -> c_int;
pub type GiveEngine = unsafe extern "system" fn(*mut EngineFunctions, *mut EngineGlobals);
