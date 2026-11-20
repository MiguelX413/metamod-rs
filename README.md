# metamod-rs

A Rust SDK for native GoldSrc Metamod plugins with server-thread-owned state,
typed game-DLL hooks, and scoped server access. Bindgen runs at build time.

## Write a plugin

Set your library's crate type to `cdylib` and add `metamod` as a dependency with
the optional `macros` feature. The attribute fills in metadata and exports the
Metamod entry points.

```rust
use metamod::prelude::*;

#[derive(Default)]
struct MyPlugin {
    greeted: bool,
}

#[metamod_plugin(name = "My Plugin", version = "0.1.0", author = "Developer")]
impl Plugin for MyPlugin {
    fn start_frame(&mut self, ctx: &mut Context<'_>) {
        if !self.greeted {
            self.greeted = true;
            ctx.server().print("Hello from Rust!");
        }
    }
}
```

The attribute also accepts `date`, `url`, and `log_tag`. With the macro feature
disabled, declare `const METADATA: Metadata` in your `impl Plugin` using static
C strings, then call `export_plugin!(MyPlugin)` once in the library. Both forms
use the same dispatch. To migrate a stateless plugin, implement `Default`
and add `&mut self` and `&mut Context<'_>` to its callbacks. Existing
`Context::log` and `Context::print_console` methods still accept C strings.

| Plugin method | Timing |
| --- | --- |
| `initialize` | Once per startup attach attempt, after construction. Return `false` to decline. |
| `start_frame` | Before the game DLL's frame callback. |
| `server_activate` | After the game DLL activates the map. |
| `server_deactivate` | Before the game DLL deactivates the map. |
| `player_join` | After the game DLL puts a player into the server. |
| `player_disconnect` | Before the game DLL disconnects a player. |

Player hooks receive a `PlayerId` resolved through GoldSrc. It represents a
one-based slot, which the engine can reuse after disconnect. Use the lifecycle
hooks to clear map-specific state. Use `ctx.server().print(&str)` for console
messages within a callback.

## Build and install hello-world

### Linux i686

Initialize the pinned Metamod and HLSDK submodules before building. Use Rust
stable 1.88 or newer, Clang and host libclang, and C/C++ development
headers for the target. For Linux i686, also install 32-bit linker and libc
development files. On Ubuntu, the CI workflow installs `clang`,
`libclang-dev`, and `g++-multilib`. Set `LIBCLANG_PATH` if bindgen cannot
find your host libclang.

```sh
git submodule update --init
rustup target add i686-unknown-linux-gnu
cargo build --locked --release --target i686-unknown-linux-gnu -p hello-world
```

### Windows x86

Install the Visual Studio C++ build tools and Windows SDK, plus LLVM with
host libclang. Use an **x86 Native Tools Command Prompt**. Set
`LIBCLANG_PATH` to the LLVM `bin` directory if bindgen cannot locate libclang.

```bat
git submodule update --init
rustup target add i686-pc-windows-msvc
cargo build --locked --release --target i686-pc-windows-msvc -p hello-world
```

### Install the plugin

In an **isolated Linux server** where Metamod is already installed, copy
`target/i686-unknown-linux-gnu/release/libhello_world.so` to
`<game>/addons/rust-hello/libhello_world.so`. Add this line to the server's
`<game>/addons/metamod/plugins.ini`:

```text
linux addons/rust-hello/libhello_world.so
```

For a **Windows server**, copy
`target/i686-pc-windows-msvc/release/hello_world.dll` to
`<game>/addons/rust-hello/hello_world.dll` and add this line instead:

```text
win32 addons/rust-hello/hello_world.dll
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
- `metamod` constructs one `Default` state value per startup attach attempt
  and mutably borrows it for each callback. State survives map changes.
  See [hello-world](examples/hello-world/src/lib.rs) for registration and
  callback examples.
- The loader supplies valid C arguments and calls `GiveFnptrsToDll`, query,
  attach, and hooks on the server thread. It keeps Metamod globals live during
  hooks. The SDK copies the few function pointers it uses and accesses
  foreign fields through raw pointers. Metadata and its strings have module
  lifetime. Scoped contexts and server handles cannot cross threads or
  outlive their callbacks. An off-thread hook skips user code and leaves
  server-thread globals alone.
- Query accepts interface major 5 with minor 13 or newer. Both pre- and
  post-`GetEntityAPI2` factories negotiate game-DLL interface 140. All
  registered hooks are observational. They set `MRES_IGNORED` on the attached
  thread, including on skipped recursive calls or after a panic.
- A thread-local checked mutable borrow guards the entire callback, including
  engine calls that can reenter through a different hook. Construction or
  initialization panic fails attach. A rejected or failed instance is
  dropped with separate panic containment. A hook panic disables all user
  hooks for that instance. Build with the default unwind strategy. Aborting
  panics, panics in panic hooks, and foreign faults can terminate the server.
- Plugins load at startup and refuse detach. A successfully attached instance
  stays resident until process exit, with no guaranteed shutdown destructor.
  Release map-specific resources during `server_deactivate`. Metamod can
  bypass detach refusal on forced unload, which the SDK does not support.

On Win32 the export macro uses stdcall for `GiveFnptrsToDll` and cdecl
for the Metamod entry points. Metamod resolves these exports by name.
The SDK leaves PE ordinals to the linker.

## Verify

With the initialized submodules and a working 32-bit C++ toolchain and
runtime:

```sh
verify/linux-i686.sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

On Windows, run the following from the x86 Native Tools Command Prompt with
PowerShell 7.4 or newer and LLVM's `bin` directory on PATH:

```bat
pwsh -NoProfile -File verify/windows-i686.ps1
```

The platform scripts check the x86 binary and its four named exports, compare
C++ and Rust sizes, alignments, and field offsets, and load the actual plugin
with `dlopen` or `LoadLibrary`. The shared loader exercises version negotiation,
metadata stability, startup-only attach, both hook tables, engine printing,
player-slot translation, cross-hook reentry, hook results, and detach refusal.
Rust tests cover failed initialization, panicking destructors, off-thread
dispatch, and callback disabling after panic.
The Linux script accepts two optional paths for comparing against separate C++
reference checkouts. Binding generation always uses the submodules.

Validated against alliedmodders/metamod-hl1 at
`18a10db686702e8ae9e0fcb5b5febf8881dc9c2d` and alliedmodders/hlsdk at
`a0edb7792a96998d349325bebab8ea41ec5cb239`. Before the bindgen migration,
a Ricochet startup on Metamod-P 1.21p109 confirmed both console messages,
including a single `StartFrame` message, and `meta list` showed the plugin in
`RUN`. The current stateful plugin has been exercised in the loader harness.
A new server smoke test, player gameplay, and a later map transition remain
unverified.

Before publishing the SDK, run the example on isolated servers with both
Metamod loaders, including a native Win32 server, and establish the supported
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
