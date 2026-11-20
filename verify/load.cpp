#include <cassert>
#include <cstdio>
#include <cstring>
#include <dlfcn.h>
#include <extdll.h>
#include <meta_api.h>
#include <h_export.h>

static plugin_info_t *info;
static meta_globals_t globals{};
static DLL_FUNCTIONS hooks{};
static DLL_FUNCTIONS post_hooks{};
static edict_t player{};
static int logged;

static int index_of_edict(const edict_t *edict) {
    assert(edict == &player);
    return 7;
}

static void server_print(const char *message) {
    static const char *expected[] = {
        "Hello from Rust Metamod\n",
        "Rust StartFrame hook ran\n",
        "Hello, player in slot 7!\n",
    };
    assert(logged < 3 && std::strcmp(message, expected[logged]) == 0);
    std::fputs(message, stdout);
    if (++logged == 2) {
        // Reenter through a different user hook while the frame owns state.
        globals.mres = MRES_UNSET;
        post_hooks.pfnClientPutInServer(&player);
        assert(logged == 2);
        assert(globals.mres == MRES_IGNORED);
        globals.mres = MRES_SUPERCEDE;
    }
}

template <typename Fn>
static Fn symbol(void *module, const char *name) {
    auto function = reinterpret_cast<Fn>(dlsym(module, name));
    assert(function);
    return function;
}

int main(int argc, char **argv) {
    assert(argc == 2);
    void *module = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!module) {
        std::fprintf(stderr, "%s\n", dlerror());
        return 1;
    }
    auto give = symbol<GIVE_ENGINE_FUNCTIONS_FN>(module, "GiveFnptrsToDll");
    auto query = symbol<META_QUERY_FN>(module, "Meta_Query");
    auto attach = symbol<META_ATTACH_FN>(module, "Meta_Attach");
    auto detach = symbol<META_DETACH_FN>(module, "Meta_Detach");
    enginefuncs_t engine{};
    engine.pfnServerPrint = server_print;
    engine.pfnIndexOfEdict = index_of_edict;
    give(&engine, nullptr);

    META_FUNCTIONS functions{};
    gamedll_funcs_t game{};
    mutil_funcs_t utilities{};
    assert(!attach(PT_STARTUP, &functions, &globals, &game));
    assert(!query("5:12", &info, &utilities));
    assert(!query("6:13", &info, &utilities));
    assert(!query("5:13x", &info, &utilities));
    assert(query("5:14", &info, &utilities));
    auto *saved_info = info;
    assert(query(META_INTERFACE_VERSION, &info, &utilities));
    assert(info == saved_info);
    assert(std::strcmp(info->name, "Rust Hello World") == 0);
    assert(info->loadable == PT_STARTUP && info->unloadable == PT_NEVER);
    assert(!attach(PT_ANYTIME, &functions, &globals, &game));
    assert(attach(PT_STARTUP, &functions, &globals, &game));
    assert(!attach(PT_STARTUP, &functions, &globals, &game));
    assert(functions.pfnGetEntityAPI2 && functions.pfnGetEntityAPI2_Post);
    int version = 0;
    assert(!functions.pfnGetEntityAPI2(&hooks, &version));
    assert(version == INTERFACE_VERSION && !hooks.pfnStartFrame);
    assert(functions.pfnGetEntityAPI2(&hooks, &version));
    version = 0;
    assert(!functions.pfnGetEntityAPI2_Post(&post_hooks, &version));
    assert(version == INTERFACE_VERSION && !post_hooks.pfnServerActivate);
    assert(functions.pfnGetEntityAPI2_Post(&post_hooks, &version));
    assert(post_hooks.pfnClientPutInServer && !post_hooks.pfnClientDisconnect);
    assert(hooks.pfnClientDisconnect && !hooks.pfnClientPutInServer);
    assert(post_hooks.pfnServerActivate && hooks.pfnServerDeactivate);
    globals.mres = MRES_UNSET;
    post_hooks.pfnServerActivate(nullptr, 0, 0);
    assert(globals.mres == MRES_IGNORED);
    assert(hooks.pfnStartFrame && !hooks.pfnThink && !hooks.pfnAllowLagCompensation);
    for (int frame = 0; frame < 2; ++frame) {
        globals.mres = MRES_UNSET;
        hooks.pfnStartFrame();
        assert(globals.mres == MRES_IGNORED);
    }
    assert(logged == 2);
    globals.mres = MRES_UNSET;
    post_hooks.pfnClientPutInServer(nullptr);
    assert(logged == 2 && globals.mres == MRES_IGNORED);
    globals.mres = MRES_UNSET;
    post_hooks.pfnClientPutInServer(&player);
    assert(logged == 3 && globals.mres == MRES_IGNORED);
    globals.mres = MRES_UNSET;
    hooks.pfnClientDisconnect(&player);
    assert(globals.mres == MRES_IGNORED);
    globals.mres = MRES_UNSET;
    hooks.pfnServerDeactivate();
    assert(globals.mres == MRES_IGNORED);
    assert(!detach(PT_ANYTIME, PNL_COMMAND));
    assert(!detach(PT_ANYTIME, PNL_CMD_FORCED));
}
