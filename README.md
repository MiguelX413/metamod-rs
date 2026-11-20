# metamod-rs

A minimal Rust SDK for native GoldSrc Metamod plugins. The first milestone
supports startup initialization, server logging, and an observational
`StartFrame` pre-hook. Bindgen is a build dependency of `metamod-sys`.

## Build and install hello-world

Initialize the pinned Metamod and HLSDK submodules before building. Use Rust
with edition 2024 support, Clang and host libclang, and C/C++ development
headers for the target. For Linux i686, also install 32-bit linker and libc
development files. On Ubuntu, the CI workflow installs `clang`,
`libclang-dev`, and `g++-multilib`. Set `LIBCLANG_PATH` if bindgen cannot
find your host libclang.

```sh
git submodule update --init
rustup target add i686-unknown-linux-gnu
cargo build --locked --release --target i686-unknown-linux-gnu -p hello-world
```

In an **isolated server** where Metamod is already installed, copy
`target/i686-unknown-linux-gnu/release/libhello_world.so` to
`<game>/addons/rust-hello/libhello_world.so`. Add this line to the server's
`<game>/addons/metamod/plugins.ini`:

```text
linux addons/rust-hello/libhello_world.so
```

Start the server with a valid map. Run `meta list` in the server console and
look for `Rust Hello World` with status `RUN`. Look for `Hello from Rust
Metamod` at attach and `Rust StartFrame hook ran` on the first frame in the
server console. The plugin returns `MRES_IGNORED` each frame, leaving game-DLL
dispatch to Metamod. To remove it, stop the server and remove the configuration
line and shared library.

## Architecture and safety

- `metamod-sys` uses libclang to parse a small C++ wrapper around the pinned
  upstream headers for Cargo's target. Bindgen generates the allowlisted
  interface and its dependent types in Cargo's `OUT_DIR`. The raw module
  exposes the upstream field names, integer enums, and nullable C function
  pointers. The export macro checks its entry points against the generated
  function pointer types at compile time.
- `metamod` provides the safe `Plugin` trait and callback-scoped `Context`.
  Declare static C-string metadata and invoke `export_plugin!(YourPlugin)`
  once in a `cdylib`. See [hello-world](examples/hello-world/src/lib.rs).
- The loader supplies valid C arguments and runs query, attach, and hooks on
  the same server thread. It keeps the globals allocation alive while calling
  the hook. The SDK copies the logging function pointer and reads and writes
  foreign fields through raw pointers. The version string is borrowed only
  during query, under the loader's guarantee of a valid readable C string.
  Metadata and its strings have module lifetime. The safe context borrows a
  Rust-owned callback scope and cannot cross threads.
- Query accepts interface major 5 with minor 13 or newer. Attach publishes only
  the pre-`GetEntityAPI2` table factory, which negotiates game DLL interface
  140. `GiveFnptrsToDll` is required by both loaders, and its arguments are
  unused in this milestone.
- Thread-local state skips recursive user callbacks without holding a lock
  across user or foreign code. Every attached frame invocation writes
  `MRES_IGNORED` after any user code, including recursive and disabled frames.
  An unwinding panic in initialization fails attach. An unwinding frame panic
  disables user frame code for the rest of the process. Build with the default
  unwind strategy. Aborting panics, panics in panic hooks, and foreign faults
  can still terminate the server.
- Plugins load at startup and refuse detach. Forced unloading can bypass the
  plugin's refusal in Metamod, so keep the module resident until process exit.

Linux i686 is the verified deployment target. On Win32 the export macro uses
stdcall for `GiveFnptrsToDll` and cdecl for the Metamod entry points. Bindgen
needs Windows SDK and C++ headers for a Windows target. Windows binding
generation, PE export names and ordinals, and loading remain unverified.

## Verify

With the initialized submodules and a working 32-bit C++ toolchain and
runtime:

```sh
verify/linux-i686.sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The verification script checks ELF32/i386 and the four exported functions,
compares C++ and Rust sizes, alignments, and field offsets, and opens the
actual shared library with `dlopen`. It exercises version negotiation,
metadata stability, startup-only attach, nested frame dispatch, hook results,
and detach refusal. The Rust test also checks recursion and panic containment.
The script accepts two optional paths for comparing against separate C++
reference checkouts. Binding generation always uses the submodules.

Validated against alliedmodders/metamod-hl1 at
`18a10db686702e8ae9e0fcb5b5febf8881dc9c2d` and alliedmodders/hlsdk at
`a0edb7792a96998d349325bebab8ea41ec5cb239`. Before the bindgen migration,
a Ricochet startup on Metamod-P 1.21p109 confirmed both console messages,
including a single `StartFrame` message, and `meta list` showed the plugin in
`RUN`. The migrated plugin has been exercised in the loader harness. A new
server smoke test, player gameplay, and a later map transition remain
unverified.

Before publishing the SDK, run the example on isolated servers with both
Metamod loaders, verify Win32 exports and loading, and establish the supported
Linux libc baseline. Add further typed hooks and scoped capabilities as
their ABI and lifetime contracts are verified.

## Licensing and references

The workspace retains its [AGPL-3.0-only license](LICENSE) for SDK-authored
code. The repository records Git links to the upstream revisions. Header
notices and licenses remain in their respective submodules. Bindgen is
BSD-3-Clause licensed and generates declarations locally. The generated files
stay in the build directory, and Cargo publication remains disabled.

Submodules preserve upstream provenance. They do not resolve redistribution
rights for archives that include the headers, generated Rust declarations,
or binaries that incorporate them. Review the upstream terms and the actual
release contents before publishing any such artifact. In particular, review
Valve's redistribution restrictions and notice requirements, and whether
GPL obligations apply to output derived from Metamod headers. Metamod's
linking exception addresses linking with Valve's engine and mods.

- Official Metamod header material by Will Day, copyright © 2001–2003, is
  GPL-2.0-or-later with an upstream exception for linking with Valve’s
  Half-Life engine and Valve MODs. [Metamod interface header and notice](https://github.com/alliedmodders/metamod-hl1/blob/18a10db686702e8ae9e0fcb5b5febf8881dc9c2d/metamod/meta_api.h)
- The Half-Life SDK is copyright © Valve Corp. Its source has separate
  redistribution terms. [HLSDK license](https://github.com/alliedmodders/hlsdk/blob/a0edb7792a96998d349325bebab8ea41ec5cb239/LICENSE)
- Metamod-R uses GPLv3. Its loader was checked for the required exports
  and interface version. [Metamod-R](https://github.com/rehlds/Metamod-R/tree/56590c5c70fa95775051888b7fe0a4e285856ebb)
- The FWGS template illustrates plug-in hook registration and Win32 exports.
  The reviewed revision had no top-level license granting reuse of the
  template. [FWGS Metamod plugin template](https://github.com/FWGS/metamod-plugin-template/tree/b9dbed28728722e8fc751200f824f72e3b8e7123)
