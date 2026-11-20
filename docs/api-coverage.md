# API coverage

The pinned headers in `upstream/` are the slot-by-slot inventory. This matrix
tracks SDK support by interface and safety contract. Update the relevant row
and verification when a wrapper is added. A generated table exposes its
function pointers through `metamod_sys::raw`; it does not imply that its
pointees have exposed fields or that the functions are safe to call.

| Interface and upstream inventory | Raw FFI | Safe SDK | Safety requirements and verification |
| --- | --- | --- | --- |
| Metamod loader and results, `metamod/meta_api.h`, `plinfo.h`, `h_export.h` | Entry point typedefs, metadata, result globals, and all eight table factory slots | Export macros, startup attachment, resident plugin policy, typed outcomes for synchronous hooks | x86 loader and layout probes. Mutable result slots belong to the current native invocation. |
| Metamod utilities, `metamod/mutil.h` | Utility table generated | Console and log output | Fixed formats for varargs. Game info, user-message lookup, entity dispatch, and plugin control still need individual ownership and reentrancy contracts. |
| GameDLL lifecycle and player callbacks, `engine/eiface.h` `DLL_FUNCTIONS`, `metamod/dllapi.h` | Complete callback table | Frame, map activate/deactivate, player put-in-server/disconnect, and lag-compensation pre/post | Loader probes cover current registrations and original lag-compensation return. Client admission, user-info editing, save/restore, movement, entity dispatch, and networking hooks remain raw; pointer/out-buffer validity and synchronous results need individual treatment. |
| NewDLL, `engine/eiface.h` `NEW_DLL_FUNCTIONS` | Complete callback table | Optional game shutdown | Layout and loader probes. Entity-private-data release, collision filtering, and CVar replies remain raw. Their lifetimes and query correlation need separate contracts. |
| Engine console, commands and CVars, `engine/eiface.h` `enginefuncs_t`, `common/cvardef.h` | Engine slots and CVar layout generated | Printing, registration, owned command tokens, CVar reads/writes | Native harness exercises reentrant commands and CVar storage. Server command execution and arbitrary engine callback hooks remain raw. |
| Engine players and entities, `engine/eiface.h`, `engine/edict.h`, `engine/progdefs.h` | Callback slots and edict, entvars, and engine globals layouts | Session-aware player lookup, owned user/auth IDs, index lookup, bounded edict discovery, and owned spatial snapshots | Native harness exercises session revalidation. All entity handles check the map epoch, edict serial, and free flag. Discovery uses map capacity captured from engine globals. The x86 probes cover vacant and out-of-range slots, spatial reads, and map transitions. Entity creation/removal and keyvalues remain raw. |
| Engine tracing, physics and movement, `engine/eiface.h`, `pm_shared/pm_defs.h` | Engine slots and trace payload layout; movement payloads remain opaque | Owned line and standard hull traces, collision mode and generation-checked skip entity | x86 layout/loader probes cover line hit, hull miss, skip entity, and a map transition during a trace. Movement and physics state still require callback-scoped mutation and reentry analysis. |
| Engine fake clients, messages, events, sound and resources, `engine/eiface.h` | Engine slots | Pending | Fake client creation needs a partial-failure and disconnect policy. Message construction needs a complete, validated send operation around the native begin/write/end sequence. Resource registration is phase-dependent. |
| Other SDK data and auxiliary interfaces, `common/`, `engine/`, `pm_shared/`, `dlls/` | Selected transitively referenced types; no blanket header coverage | Pending | Add audited public ABI types by header family. Keep game implementation types and templates outside the ABI surface. |

The Linux i686 layout probes and loader harness passed for both examples.
Win32 CI is configured to run the same cases. The harness simulates the host,
so live-server verification and cross-fork behavior are tracked separately.

## Extension rules

- Use the bindgen callback table as the signature source. Keep native pointer
  reads and writes in the FFI boundary and add layout or loader checks for
  each new call shape.
- Keep observations deferable only when their payload is owned. A synchronous
  decision, writable buffer, or borrowed native handle needs a same-call
  policy. Returning hooks skipped under reentry leave `MRES_IGNORED` and
  return a placeholder the host ignores.
- Use `HookResult<T>` for pre decisions and `PostHookResult<T>` for post
  decisions. Superceding skips the GameDLL call and is a pre-only operation.
  Copy original returns before calling plugin code and read only storage the
  specific ABI callback supplies. Metamod owns result arbitration.
- Prefer snapshots and validated handles to references into the engine. Take
  copies before another native call and revalidate identities after calls
  that can reenter. Keep map, callback, and engine lifetimes distinct.

## Next milestones

1. Extend entity access with owned classification and keyvalue snapshots,
   with explicit rules for string-table lifetimes.
2. Add a synchronous client admission hook with bounded rejection output and
   expand result verification against a real Metamod host on both x86 targets.
3. Add validated message construction and fake-client lifecycle management,
   then audit movement/physics and expand raw payload coverage family by family.
