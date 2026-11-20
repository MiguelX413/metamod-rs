#include <cstdio>
#include <cstddef>
#include <extdll.h>
#include <meta_api.h>

int main() {
    std::printf("PluginInfo %zu %zu\n", sizeof(plugin_info_t), alignof(plugin_info_t));
    std::printf("interface %zu\n", offsetof(plugin_info_t, ifvers));
    std::printf("name %zu\n", offsetof(plugin_info_t, name));
    std::printf("version %zu\n", offsetof(plugin_info_t, version));
    std::printf("date %zu\n", offsetof(plugin_info_t, date));
    std::printf("author %zu\n", offsetof(plugin_info_t, author));
    std::printf("url %zu\n", offsetof(plugin_info_t, url));
    std::printf("log_tag %zu\n", offsetof(plugin_info_t, logtag));
    std::printf("loadable %zu\n", offsetof(plugin_info_t, loadable));
    std::printf("unloadable %zu\n", offsetof(plugin_info_t, unloadable));
    std::printf("MetaGlobals %zu %zu\n", sizeof(meta_globals_t), alignof(meta_globals_t));
    std::printf("result %zu\n", offsetof(meta_globals_t, mres));
    std::printf("previous %zu\n", offsetof(meta_globals_t, prev_mres));
    std::printf("status %zu\n", offsetof(meta_globals_t, status));
    std::printf("original_return %zu\n", offsetof(meta_globals_t, orig_ret));
    std::printf("override_return %zu\n", offsetof(meta_globals_t, override_ret));
    std::printf("MetaFunctions %zu %zu\n", sizeof(META_FUNCTIONS), alignof(META_FUNCTIONS));
    std::printf("entity_api %zu\n", offsetof(META_FUNCTIONS, pfnGetEntityAPI));
    std::printf("entity_api_post %zu\n", offsetof(META_FUNCTIONS, pfnGetEntityAPI_Post));
    std::printf("entity_api2 %zu\n", offsetof(META_FUNCTIONS, pfnGetEntityAPI2));
    std::printf("entity_api2_post %zu\n", offsetof(META_FUNCTIONS, pfnGetEntityAPI2_Post));
    std::printf("new_dll %zu\n", offsetof(META_FUNCTIONS, pfnGetNewDLLFunctions));
    std::printf("new_dll_post %zu\n", offsetof(META_FUNCTIONS, pfnGetNewDLLFunctions_Post));
    std::printf("engine %zu\n", offsetof(META_FUNCTIONS, pfnGetEngineFunctions));
    std::printf("engine_post %zu\n", offsetof(META_FUNCTIONS, pfnGetEngineFunctions_Post));
    std::printf("GameDllFunctions %zu %zu\n", sizeof(gamedll_funcs_t), alignof(gamedll_funcs_t));
    std::printf("dll %zu\n", offsetof(gamedll_funcs_t, dllapi_table));
    std::printf("new_dll %zu\n", offsetof(gamedll_funcs_t, newapi_table));
    std::printf("MetaUtilities %zu %zu\n", sizeof(mutil_funcs_t), alignof(mutil_funcs_t));
    std::printf("log_console %zu\n", offsetof(mutil_funcs_t, pfnLogConsole));
    std::printf("log_message %zu\n", offsetof(mutil_funcs_t, pfnLogMessage));
    std::printf("log_error %zu\n", offsetof(mutil_funcs_t, pfnLogError));
    std::printf("log_developer %zu\n", offsetof(mutil_funcs_t, pfnLogDeveloper));
    std::printf("DllFunctions %zu %zu\n", sizeof(DLL_FUNCTIONS), alignof(DLL_FUNCTIONS));
    std::printf("start_frame %zu\n", offsetof(DLL_FUNCTIONS, pfnStartFrame));
    std::printf("EngineFunctions %zu %zu\n", sizeof(enginefuncs_t), alignof(enginefuncs_t));
    std::printf("pfnServerPrint %zu\n", offsetof(enginefuncs_t, pfnServerPrint));
    std::printf("pfnIndexOfEdict %zu\n", offsetof(enginefuncs_t, pfnIndexOfEdict));
    std::printf("pfnServerActivate %zu\n", offsetof(DLL_FUNCTIONS, pfnServerActivate));
    std::printf("pfnServerDeactivate %zu\n", offsetof(DLL_FUNCTIONS, pfnServerDeactivate));
    std::printf("pfnClientPutInServer %zu\n", offsetof(DLL_FUNCTIONS, pfnClientPutInServer));
    std::printf("pfnClientDisconnect %zu\n", offsetof(DLL_FUNCTIONS, pfnClientDisconnect));
    std::printf("constants %s %d %d %d %d\n", META_INTERFACE_VERSION, INTERFACE_VERSION, PT_NEVER, PT_STARTUP, MRES_IGNORED);
}
