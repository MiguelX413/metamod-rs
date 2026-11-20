use metamod_sys::*;
use std::mem::{align_of, offset_of, size_of};

fn main() {
    println!(
        "PluginInfo {} {}",
        size_of::<PluginInfo>(),
        align_of::<PluginInfo>()
    );
    println!("interface {}", offset_of!(PluginInfo, interface));
    println!("name {}", offset_of!(PluginInfo, name));
    println!("version {}", offset_of!(PluginInfo, version));
    println!("date {}", offset_of!(PluginInfo, date));
    println!("author {}", offset_of!(PluginInfo, author));
    println!("url {}", offset_of!(PluginInfo, url));
    println!("log_tag {}", offset_of!(PluginInfo, log_tag));
    println!("loadable {}", offset_of!(PluginInfo, loadable));
    println!("unloadable {}", offset_of!(PluginInfo, unloadable));
    println!(
        "MetaGlobals {} {}",
        size_of::<MetaGlobals>(),
        align_of::<MetaGlobals>()
    );
    println!("result {}", offset_of!(MetaGlobals, result));
    println!("previous {}", offset_of!(MetaGlobals, previous));
    println!("status {}", offset_of!(MetaGlobals, status));
    println!(
        "original_return {}",
        offset_of!(MetaGlobals, original_return)
    );
    println!(
        "override_return {}",
        offset_of!(MetaGlobals, override_return)
    );
    println!(
        "MetaFunctions {} {}",
        size_of::<MetaFunctions>(),
        align_of::<MetaFunctions>()
    );
    println!("entity_api {}", offset_of!(MetaFunctions, entity_api));
    println!(
        "entity_api_post {}",
        offset_of!(MetaFunctions, entity_api_post)
    );
    println!("entity_api2 {}", offset_of!(MetaFunctions, entity_api2));
    println!(
        "entity_api2_post {}",
        offset_of!(MetaFunctions, entity_api2_post)
    );
    println!("new_dll {}", offset_of!(MetaFunctions, new_dll));
    println!("new_dll_post {}", offset_of!(MetaFunctions, new_dll_post));
    println!("engine {}", offset_of!(MetaFunctions, engine));
    println!("engine_post {}", offset_of!(MetaFunctions, engine_post));
    println!(
        "GameDllFunctions {} {}",
        size_of::<GameDllFunctions>(),
        align_of::<GameDllFunctions>()
    );
    println!("dll {}", offset_of!(GameDllFunctions, dll));
    println!("new_dll {}", offset_of!(GameDllFunctions, new_dll));
    println!(
        "MetaUtilities {} {}",
        size_of::<MetaUtilities>(),
        align_of::<MetaUtilities>()
    );
    println!("log_console {}", offset_of!(MetaUtilities, log_console));
    println!("log_message {}", offset_of!(MetaUtilities, log_message));
    println!("log_error {}", offset_of!(MetaUtilities, log_error));
    println!("log_developer {}", offset_of!(MetaUtilities, log_developer));
    println!(
        "DllFunctions {} {}",
        size_of::<DllFunctions>(),
        align_of::<DllFunctions>()
    );
    println!("start_frame {}", offset_of!(DllFunctions, start_frame));
    println!(
        "constants {} {} {} {} {}",
        META_INTERFACE.to_str().unwrap(),
        ENTITY_INTERFACE,
        PT_NEVER,
        PT_STARTUP,
        MRES_IGNORED
    );
}
