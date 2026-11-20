//! Startup-only GoldSrc Metamod plugins with an observational StartFrame hook.

use std::ffi::CStr;

#[doc(hidden)]
pub mod __ffi;
mod runtime;

pub use runtime::Context;

/// Strings remain available to Metamod for the lifetime of the module.
pub struct Metadata {
    pub name: &'static CStr,
    pub version: &'static CStr,
    pub date: &'static CStr,
    pub author: &'static CStr,
    pub url: &'static CStr,
    pub log_tag: &'static CStr,
}

pub trait Plugin: 'static {
    const METADATA: Metadata;

    /// Return false to decline attachment.
    fn initialize(_context: &Context<'_>) -> bool {
        true
    }

    fn start_frame(_context: &Context<'_>) {}
}

/// Put this once at the root of the plugin's cdylib crate.
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        use $crate::__ffi as __metamod_ffi;

        static mut __METAMOD_INFO: __metamod_ffi::PluginInfo = __metamod_ffi::info::<$plugin>();

        #[cfg(all(windows, target_arch = "x86"))]
        #[unsafe(no_mangle)]
        unsafe extern "stdcall" fn GiveFnptrsToDll(
            _: *mut __metamod_ffi::EngineFunctions,
            _: *mut __metamod_ffi::EngineGlobals,
        ) {
        }

        #[cfg(not(all(windows, target_arch = "x86")))]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn GiveFnptrsToDll(
            _: *mut __metamod_ffi::EngineFunctions,
            _: *mut __metamod_ffi::EngineGlobals,
        ) {
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
