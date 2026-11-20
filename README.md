# metamod-rs

A minimal Rust SDK for native GoldSrc Metamod plugins. The first milestone
supports startup initialization, server logging, and an observational
`StartFrame` pre-hook. It has no external Cargo dependencies.

## Build and install hello-world

Use Rust with edition 2024 support, the i686 GNU/Linux Rust target, and a C
linker with 32-bit libc development files.

```sh
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

- `metamod-sys` describes the required C ABI with `repr(C)`, integer enum
  values, and nullable C function pointers. Unused callback slots are private
  and retain their positions in the tables. Engine and new-DLL types that only
  appear behind pointers are opaque.
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

Linux i686 is the verified deployment target. Win32 uses the same C layouts
and cdecl callbacks, with the system calling convention for
`GiveFnptrsToDll` (stdcall on Win32). PE export names and the
`GiveFnptrsToDll` ordinal convention used by upstream module definitions
still need target-specific linker verification before Windows deployment.

## Verify

With the upstream Metamod and HLSDK checkouts and a working 32-bit C++
toolchain and runtime:

```sh
verify/linux-i686.sh /path/to/metamod-hl1 /path/to/hlsdk
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The verification script checks ELF32/i386 and the four exported functions,
compares C++ and Rust sizes, alignments, and field offsets, and opens the
actual shared library with `dlopen`. It exercises version negotiation,
metadata stability, startup-only attach, nested frame dispatch, hook results,
and detach refusal. The Rust test also checks recursion and panic containment.

Validated against alliedmodders/metamod-hl1 at
`18a10db686702e8ae9e0fcb5b5febf8881dc9c2d` and alliedmodders/hlsdk at
`a0edb7792a96998d349325bebab8ea41ec5cb239`. No isolated GoldSrc server was
available during development. A Ricochet startup on Metamod-P 1.21p109 confirmed both
console messages, including a single `StartFrame` message, and `meta list`
showed the plugin in `RUN`. Player gameplay and a later map transition
remain unverified.

Before publishing the SDK, run the example on isolated servers with both
Metamod loaders, verify Win32 exports and loading, and establish the supported
Linux libc baseline. Add further typed hooks and scoped capabilities as
their ABI and lifetime contracts are verified.

## Licensing and references

The workspace retains its [AGPL-3.0-only license](LICENSE). The Rust bindings
are a handwritten description of the binary interface and its required
sizes, signatures, slot positions, and numeric values. Upstream headers are
external verification inputs. The workspace ships no generated bindings,
vendored headers, or upstream implementation code. Cargo publication is
disabled for this milestone.

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
