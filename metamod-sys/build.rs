use std::{env, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../upstream");
    let metamod = root.join("metamod-hl1/metamod");
    let hlsdk = root.join("hlsdk");
    assert!(
        metamod.join("meta_api.h").is_file() && hlsdk.join("dlls/extdll.h").is_file(),
        "missing upstream headers: run git submodule update --init"
    );

    let target = env::var("TARGET").unwrap();
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    // MSVC's supported C++ standard switches start at C++14.
    let standard = if os == "windows" {
        "-std=c++17"
    } else {
        "-std=c++11"
    };
    let mut builder = bindgen::Builder::default()
        .header("wrapper.hpp")
        .clang_args(["-x", "c++", standard, &format!("--target={target}")])
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_type("META_(FUNCTIONS|RES|QUERY_FN|ATTACH_FN|DETACH_FN)")
        .allowlist_type("plugin_info_t|meta_globals_t|gamedll_funcs_t|mutil_funcs_t")
        .allowlist_type("GIVE_ENGINE_FUNCTIONS_FN|Metamod.*|HULL_TYPE|IGNORE_MONSTERS")
        .allowlist_var("META_INTERFACE_VERSION|INTERFACE_VERSION|NEW_DLL_FUNCTIONS_VERSION")
        .opaque_type("customization_s|playermove_s|entity_state_s")
        .opaque_type("saverestore_s|usercmd_s|clientdata_s|weapon_data_s|netadr_s")
        .prepend_enum_name(false)
        .derive_default(true)
        .generate_comments(false)
        .disable_header_comment()
        .use_core()
        .rust_edition(bindgen::RustEdition::Edition2024);

    if os == "linux" {
        builder = builder.clang_args(["-DLINUX", "-DGNUC"]);
    }
    for directory in [
        metamod,
        hlsdk.join("dlls"),
        hlsdk.join("engine"),
        hlsdk.join("common"),
        hlsdk.join("pm_shared"),
        hlsdk.join("public"),
    ] {
        builder = builder.clang_arg(format!("-I{}", directory.display()));
    }

    builder
        .generate()
        .expect("could not generate Metamod bindings with libclang")
        .write_to_file(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("bindings.rs"))
        .expect("could not write Metamod bindings");
}
