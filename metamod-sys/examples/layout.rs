use metamod_sys::*;
use std::mem::{align_of, offset_of, size_of};

fn main() {
    println!(
        "PluginInfo {} {}",
        size_of::<PluginInfo>(),
        align_of::<PluginInfo>()
    );
    println!("interface {}", offset_of!(PluginInfo, ifvers));
    println!("name {}", offset_of!(PluginInfo, name));
    println!("version {}", offset_of!(PluginInfo, version));
    println!("date {}", offset_of!(PluginInfo, date));
    println!("author {}", offset_of!(PluginInfo, author));
    println!("url {}", offset_of!(PluginInfo, url));
    println!("log_tag {}", offset_of!(PluginInfo, logtag));
    println!("loadable {}", offset_of!(PluginInfo, loadable));
    println!("unloadable {}", offset_of!(PluginInfo, unloadable));
    println!(
        "MetaGlobals {} {}",
        size_of::<MetaGlobals>(),
        align_of::<MetaGlobals>()
    );
    println!("result {}", offset_of!(MetaGlobals, mres));
    println!("previous {}", offset_of!(MetaGlobals, prev_mres));
    println!("status {}", offset_of!(MetaGlobals, status));
    println!("original_return {}", offset_of!(MetaGlobals, orig_ret));
    println!("override_return {}", offset_of!(MetaGlobals, override_ret));
    println!(
        "MetaFunctions {} {}",
        size_of::<MetaFunctions>(),
        align_of::<MetaFunctions>()
    );
    println!("entity_api {}", offset_of!(MetaFunctions, pfnGetEntityAPI));
    println!(
        "entity_api_post {}",
        offset_of!(MetaFunctions, pfnGetEntityAPI_Post)
    );
    println!(
        "entity_api2 {}",
        offset_of!(MetaFunctions, pfnGetEntityAPI2)
    );
    println!(
        "entity_api2_post {}",
        offset_of!(MetaFunctions, pfnGetEntityAPI2_Post)
    );
    println!(
        "new_dll {}",
        offset_of!(MetaFunctions, pfnGetNewDLLFunctions)
    );
    println!(
        "new_dll_post {}",
        offset_of!(MetaFunctions, pfnGetNewDLLFunctions_Post)
    );
    println!(
        "engine {}",
        offset_of!(MetaFunctions, pfnGetEngineFunctions)
    );
    println!(
        "engine_post {}",
        offset_of!(MetaFunctions, pfnGetEngineFunctions_Post)
    );
    println!(
        "GameDllFunctions {} {}",
        size_of::<GameDllFunctions>(),
        align_of::<GameDllFunctions>()
    );
    println!("dll {}", offset_of!(GameDllFunctions, dllapi_table));
    println!("new_dll {}", offset_of!(GameDllFunctions, newapi_table));
    println!(
        "MetaUtilities {} {}",
        size_of::<MetaUtilities>(),
        align_of::<MetaUtilities>()
    );
    println!("log_console {}", offset_of!(MetaUtilities, pfnLogConsole));
    println!("log_message {}", offset_of!(MetaUtilities, pfnLogMessage));
    println!("log_error {}", offset_of!(MetaUtilities, pfnLogError));
    println!(
        "log_developer {}",
        offset_of!(MetaUtilities, pfnLogDeveloper)
    );
    println!(
        "DllFunctions {} {}",
        size_of::<DllFunctions>(),
        align_of::<DllFunctions>()
    );
    println!("start_frame {}", offset_of!(DllFunctions, pfnStartFrame));
    println!(
        "EngineFunctions {} {}",
        size_of::<EngineFunctions>(),
        align_of::<EngineFunctions>()
    );
    println!(
        "pfnServerPrint {}",
        offset_of!(EngineFunctions, pfnServerPrint)
    );
    println!(
        "pfnIndexOfEdict {}",
        offset_of!(EngineFunctions, pfnIndexOfEdict)
    );
    println!(
        "pfnServerActivate {}",
        offset_of!(DllFunctions, pfnServerActivate)
    );
    println!(
        "pfnServerDeactivate {}",
        offset_of!(DllFunctions, pfnServerDeactivate)
    );
    println!(
        "pfnClientPutInServer {}",
        offset_of!(DllFunctions, pfnClientPutInServer)
    );
    println!(
        "pfnClientDisconnect {}",
        offset_of!(DllFunctions, pfnClientDisconnect)
    );
    println!(
        "NewDllFunctions {} {}",
        size_of::<raw::NEW_DLL_FUNCTIONS>(),
        align_of::<raw::NEW_DLL_FUNCTIONS>()
    );
    println!(
        "pfnGameShutdown {}",
        offset_of!(raw::NEW_DLL_FUNCTIONS, pfnGameShutdown)
    );
    println!("new_dll_version {}", raw::NEW_DLL_FUNCTIONS_VERSION);
    println!(
        "constants {} {} {} {} {}",
        META_INTERFACE.to_str().unwrap(),
        ENTITY_INTERFACE,
        PT_NEVER,
        PT_STARTUP,
        MRES_IGNORED
    );
}
