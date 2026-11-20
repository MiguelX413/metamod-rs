#include <cassert>
#include <cstdio>
#include <cstring>
#include <thread>
#include <extdll.h>
#include <meta_api.h>
#include <h_export.h>

static_assert(sizeof(void *) == 4, "build the loader for x86");

static plugin_info_t *info;
static meta_globals_t globals{};
static DLL_FUNCTIONS hooks{};
static DLL_FUNCTIONS post_hooks{};
static edict_t player{}, world{}, vacant{};
static bool discovery_transition;
static int logged;
static int keyvalue_logs;
static bool engine_demo;

static cvar_t *registered_cvar;
static char cvar_value[1024];
static void (*command)();
static void (*trace_command)();
static void (*entities_command)();
static bool trace_transition;
static const char *args[] = {"RUST_HELLO", nullptr};
static bool reenter_argv;
static bool expire_player;

static cvar_t *find_cvar(const char *name) {
    return registered_cvar && std::strcmp(name, registered_cvar->name) == 0 ? registered_cvar : nullptr;
}

static void register_cvar(cvar_t *cvar) {
    assert(!registered_cvar && std::strcmp(cvar->name, "rust_hello_message") == 0);
    assert(cvar->flags == 0 && cvar->next == nullptr);
    registered_cvar = cvar;
    std::strcpy(cvar_value, cvar->string);
    cvar->string = cvar_value;
}

static const char *get_cvar(const char *name) {
    assert(find_cvar(name));
    return registered_cvar->string;
}

static void set_cvar(const char *name, const char *value) {
    assert(find_cvar(name));
    std::strcpy(cvar_value, value);
}

static void register_command(const char *name, void (*callback)()) {
    assert(callback);
    if (std::strcmp(name, "rust_trace") == 0) {
        assert(!trace_command);
        trace_command = callback;
    } else if (std::strcmp(name, "rust_entities") == 0) {
        assert(!entities_command);
        entities_command = callback;
    } else {
        assert(!command && std::strcmp(name, "rust_hello") == 0);
        command = callback;
    }
}

static int command_argc() {
    return args[1] ? 2 : 1;
}

static const char *command_argv(int index) {
    assert(index >= 0 && index < command_argc());
    if (reenter_argv) {
        reenter_argv = false;
        command();
    }
    return args[index];
}

static edict_t *entity_of_index(int index) {
    assert(index >= 0 && index < 8);
    if (index == 3 && discovery_transition) {
        hooks.pfnServerDeactivate();
    }
    if (index == 0) return &world;
    if (index == 5) return &vacant;
    return index == 7 ? &player : nullptr;
}

static bool string_transition;

static const char *string_from_index(int index) {
    static char value[32];
    assert(index == 1 || index == 2 || index == -3);
    if (string_transition) {
        hooks.pfnServerDeactivate();
    }
    std::strcpy(value, index == 1 ? "worldspawn" : index == 2 ? "player" : "demo_target");
    return value;
}

static int player_userid(edict_t *edict) {
    assert(edict == &player);
    return 42;
}

static const char *player_authid(edict_t *edict) {
    assert(edict == &player);
    if (expire_player) {
        expire_player = false;
        hooks.pfnClientDisconnect(edict);
    }
    return "STEAM_ID_TEST";
}

static void trace_line(const float *start, const float *end, int mode, edict_t *skip, TraceResult *out) {
    assert(start[2] == 64 && end[2] == -64 && mode == dont_ignore_monsters && !skip);
    out->flFraction = 0.5f;
    out->vecEndPos.z = 0;
    out->vecPlaneNormal.z = 1;
    out->pHit = &player;
    if (trace_transition) {
        hooks.pfnServerDeactivate();
    }
}

static void trace_hull(const float *start, const float *end, int mode, int hull, edict_t *skip, TraceResult *out) {
    assert(start[2] == 64 && end[2] == -64 && mode == ignore_monsters && hull == human_hull);
    assert(skip == &player);
    out->flFraction = 1.0f;
}

static int index_of_edict(const edict_t *edict) {
    assert(edict == &player);
    return 7;
}

static void server_print(const char *message) {
    if (std::strncmp(message, "Map classname: ", 15) == 0) {
        assert(engine_demo && keyvalue_logs < 2);
        assert(logged == (keyvalue_logs == 0 ? 1 : 4));
        assert(std::strcmp(message, keyvalue_logs == 0
            ? "Map classname: worldspawn\n" : "Map classname: deferred\n") == 0);
        ++keyvalue_logs;
        std::fputs(message, stdout);
        return;
    }
    static const char *hello_output[] = {
        "Hello from Rust Metamod\n",
        "Rust StartFrame hook ran\n",
        "Hello, player in slot 7!\n",
    };
    static const char *engine_output[] = {
        "Rust Engine Demo loaded\n",
        "Rust Engine Demo first frame\n",
        "Hello, player in slot 7 (user ID 42)!\n",
        "Deferred command argument\377\n",
        "Hello, player in slot 7 (user ID 42)!\n",
        "Deferred command argument\377\n",
        "New value\n",
        "Visible entities: 2\n",
        "Entity 0 World class=worldspawn targetname=<unset> origin: [0.0, 0.0, 0.0]\n",
        "Entity 7 FakeClient class=player targetname=demo_target origin: [1.0, 2.0, 3.0]\n",
        "Visible entities: 0\n",
        "Entity discovery failed: the map changed during entity discovery\n",
        "Visible entities: 1\n",
        "Line fraction: 0.5, hit: Some(7)\n",
        "Hull fraction: 1\n",
        "Line trace failed: the map changed during the trace\n",
    };
    assert(logged < (engine_demo ? 16 : 3));
    assert(std::strcmp(message, engine_demo ? engine_output[logged] : hello_output[logged]) == 0);
    std::fputs(message, stdout);
    if (++logged == 2) {
        // Both notifications are queued while the frame owns plugin state.
        globals.mres = MRES_UNSET;
        post_hooks.pfnClientPutInServer(&player);
        assert(logged == 2 && globals.mres == MRES_IGNORED);
        globals.mres = MRES_SUPERCEDE;
        if (!engine_demo) {
            assert(!command && !registered_cvar);
            return;
        }
        char transient[] = "Deferred command argument\377";
        args[1] = transient;
        assert(command);
        command();
        assert(logged == 2 && globals.mres == MRES_SUPERCEDE);
        transient[0] = 'X';
        args[1] = nullptr;
        char key[] = "classname";
        char value[] = "deferred";
        KeyValueData data{"fixture", key, value, 17};
        hooks.pfnKeyValue(&player, &data);
        assert(data.fHandled == 17 && keyvalue_logs == 1);
        assert(globals.mres == MRES_IGNORED);
        key[0] = 'X';
        value[0] = 'X';
    }
}

template <typename Fn>
static Fn symbol(DLHANDLE module, const char *name) {
    auto function = reinterpret_cast<Fn>(DLSYM(module, name));
    assert(function);
    return function;
}

int main(int argc, char **argv) {
    assert(argc == 3);
    engine_demo = std::strcmp(argv[2], "engine-demo") == 0;
    assert(engine_demo || std::strcmp(argv[2], "hello-world") == 0);
    DLHANDLE module = DLOPEN(argv[1]);
    if (!module) {
#ifdef _WIN32
        std::fprintf(stderr, "LoadLibrary failed with error %lu\n", GetLastError());
#else
        std::fprintf(stderr, "%s\n", dlerror());
#endif
        return 1;
    }
    auto give = symbol<GIVE_ENGINE_FUNCTIONS_FN>(module, "GiveFnptrsToDll");
    auto query = symbol<META_QUERY_FN>(module, "Meta_Query");
    auto attach = symbol<META_ATTACH_FN>(module, "Meta_Attach");
    auto detach = symbol<META_DETACH_FN>(module, "Meta_Detach");
    enginefuncs_t engine{};
    engine.pfnServerPrint = server_print;
    engine.pfnTraceLine = trace_line;
    engine.pfnTraceHull = trace_hull;
    engine.pfnIndexOfEdict = index_of_edict;
    engine.pfnCVarRegister = register_cvar;
    engine.pfnCVarGetPointer = find_cvar;
    engine.pfnCVarGetString = get_cvar;
    engine.pfnCVarSetString = set_cvar;
    engine.pfnAddServerCommand = register_command;
    engine.pfnCmd_Argc = command_argc;
    engine.pfnCmd_Argv = command_argv;
    engine.pfnPEntityOfEntIndex = entity_of_index;
    engine.pfnSzFromIndex = string_from_index;
    engine.pfnGetPlayerUserId = player_userid;
    engine.pfnGetPlayerAuthId = player_authid;
    globalvars_t engine_globals{};
    engine_globals.maxEntities = 8;
    player.v.origin.x = 1;
    player.v.origin.y = 2;
    player.v.origin.z = 3;
    vacant.free = 1;
    world.v.classname = 1;
    player.v.classname = 2;
    player.v.targetname = static_cast<string_t>(-3);
    player.v.flags = FL_CLIENT | FL_FAKECLIENT;
    META_FUNCTIONS functions{};
    gamedll_funcs_t game{};
    mutil_funcs_t utilities{};
    assert(!query(META_INTERFACE_VERSION, &info, &utilities));
    give(&engine, &engine_globals);
    std::thread foreign_loader([&] {
        enginefuncs_t other_engine{};
        give(&other_engine, nullptr);
        plugin_info_t *foreign_info = nullptr;
        assert(!query(META_INTERFACE_VERSION, &foreign_info, &utilities));
        assert(!foreign_info);
        assert(!attach(PT_STARTUP, &functions, &globals, &game));
    });
    foreign_loader.join();
    assert(!attach(PT_STARTUP, &functions, &globals, &game));
    assert(!query("5:12", &info, &utilities));
    assert(!query("6:13", &info, &utilities));
    assert(!query("5:13x", &info, &utilities));
    assert(query("5:14", &info, &utilities));
    auto *saved_info = info;
    assert(query(META_INTERFACE_VERSION, &info, &utilities));
    assert(info == saved_info);
    assert(std::strcmp(info->name, engine_demo ? "Rust Engine Demo" : "Rust Hello World") == 0);
    assert(std::strcmp(info->logtag, engine_demo ? "RUSTENGINE" : "RUSTHELLO") == 0);
    if (engine_demo) {
        assert(std::strcmp(info->version, "0.1.0") == 0);
        assert(std::strcmp(info->author, "metamod-rs contributors") == 0);
    }
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
    assert(post_hooks.pfnServerActivate && hooks.pfnServerActivate && hooks.pfnServerDeactivate);
    NEW_DLL_FUNCTIONS new_hooks{};
    version = 0;
    assert(functions.pfnGetNewDLLFunctions && !functions.pfnGetNewDLLFunctions(&new_hooks, &version));
    assert(version == NEW_DLL_FUNCTIONS_VERSION && !new_hooks.pfnGameShutdown);
    assert(functions.pfnGetNewDLLFunctions(&new_hooks, &version));
    assert(new_hooks.pfnGameShutdown && !new_hooks.pfnShouldCollide);
    assert((hooks.pfnKeyValue != nullptr) == engine_demo && !post_hooks.pfnKeyValue);
    if (engine_demo) {
        KeyValueData data{nullptr, "classname", "worldspawn", 17};
        hooks.pfnKeyValue(&world, &data);
        assert(keyvalue_logs == 1 && data.fHandled == 17 && globals.mres == MRES_IGNORED);
        char oversized[4097];
        std::memset(oversized, 'x', sizeof(oversized));
        oversized[sizeof(oversized) - 1] = '\0';
        data.szValue = oversized;
        hooks.pfnKeyValue(&world, &data);
        data.szValue = nullptr;
        hooks.pfnKeyValue(&world, &data);
        hooks.pfnKeyValue(&world, nullptr);
        assert(keyvalue_logs == 1 && data.fHandled == 17 && globals.mres == MRES_IGNORED);
    }
    hooks.pfnServerActivate(nullptr, 0, 0);
    globals.mres = MRES_UNSET;
    post_hooks.pfnServerActivate(nullptr, 0, 0);
    assert(globals.mres == MRES_IGNORED);
    assert(hooks.pfnStartFrame && !hooks.pfnThink);
    assert((hooks.pfnAllowLagCompensation != nullptr) == engine_demo);
    assert((post_hooks.pfnAllowLagCompensation != nullptr) == engine_demo);
    if (engine_demo) {
        assert(hooks.pfnAllowLagCompensation() == 0 && globals.mres == MRES_HANDLED);
        int original = 1;
        globals.orig_ret = &original;
        globals.status = MRES_HANDLED;
        assert(post_hooks.pfnAllowLagCompensation() == 0 && globals.mres == MRES_HANDLED);
        globals.status = MRES_SUPERCEDE;
        assert(post_hooks.pfnAllowLagCompensation() == 0 && globals.mres == MRES_IGNORED);
        globals.orig_ret = nullptr;
        globals.status = MRES_UNSET;
    }
    globals.mres = MRES_SUPERCEDE;
    std::thread foreign_hook([] { hooks.pfnStartFrame(); });
    foreign_hook.join();
    assert(globals.mres == MRES_SUPERCEDE && logged == 1);
    for (int frame = 0; frame < 2; ++frame) {
        globals.mres = MRES_UNSET;
        hooks.pfnStartFrame();
        assert(globals.mres == MRES_IGNORED);
    }
    if (engine_demo) {
        assert(logged == 4);
        globals.mres = MRES_UNSET;
        post_hooks.pfnClientPutInServer(nullptr);
        assert(logged == 4 && globals.mres == MRES_IGNORED);
        globals.mres = MRES_UNSET;
        post_hooks.pfnClientPutInServer(&player);
        assert(logged == 5 && globals.mres == MRES_IGNORED);
        expire_player = true;
        post_hooks.pfnClientPutInServer(&player);
        assert(!expire_player && logged == 5);
        reenter_argv = true;
        command();
        assert(!reenter_argv && logged == 5);
        globals.mres = MRES_SUPERCEDE;
        command();
        assert(logged == 6 && globals.mres == MRES_SUPERCEDE);
        args[1] = "New value";
        command();
        args[1] = nullptr;
        assert(logged == 7 && std::strcmp(cvar_value, "New value") == 0);
        assert(entities_command);
        args[0] = "rust_entities";
        entities_command();
        args[1] = "8";
        entities_command();
        args[1] = nullptr;
        discovery_transition = true;
        entities_command();
        discovery_transition = false;
        post_hooks.pfnServerActivate(nullptr, 0, 0);
        assert(logged == 12);
        args[1] = "7";
        string_transition = true;
        entities_command();
        string_transition = false;
        args[1] = nullptr;
        assert(logged == 13);
        post_hooks.pfnServerActivate(nullptr, 0, 0);
        assert(trace_command);
        args[0] = "rust_trace";
        trace_command();
        assert(logged == 15);
        trace_transition = true;
        trace_command();
        assert(logged == 16);
    } else {
        assert(logged == 3 && !command && !registered_cvar);
    }
    assert(keyvalue_logs == (engine_demo ? 2 : 0));
    const int expected_logs = engine_demo ? 16 : 3;
    globals.mres = MRES_UNSET;
    hooks.pfnClientDisconnect(&player);
    assert(globals.mres == MRES_IGNORED);
    globals.mres = MRES_UNSET;
    hooks.pfnServerDeactivate();
    assert(globals.mres == MRES_IGNORED);
    assert(!detach(PT_ANYTIME, PNL_COMMAND));
    assert(!detach(PT_ANYTIME, PNL_CMD_FORCED));
    globals.mres = MRES_UNSET;
    new_hooks.pfnGameShutdown();
    assert(globals.mres == MRES_IGNORED);
    globals.mres = MRES_UNSET;
    post_hooks.pfnClientPutInServer(&player);
    assert(logged == expected_logs && globals.mres == MRES_IGNORED);
    if (engine_demo) {
        command();
        assert(logged == expected_logs);
    }
}
