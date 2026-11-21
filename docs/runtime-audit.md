# GoldSrc runtime auditing

`metamod-audit` is a resident Metamod plugin built on the public SDK. It uses normal GameDLL hooks, ordinary server commands, and owned SDK snapshots. The Python runner copies an operator-supplied installation, selects one Metamod and one GameDLL, and collects the plugin's observations. The C++ fixture exercises adversarial SDK paths with controlled native inputs.

## Matrix and prerequisites

| Engine | Manager | Priority |
| --- | --- | --- |
| Valve HLDS | alliedmodders/metamod-hl1 | Primary |
| ReHLDS | alliedmodders/metamod-hl1 | Primary |
| ReHLDS | Metamod-R | Primary |
| Valve HLDS | Metamod-R | Experimental |

Each pair has Linux i686 and Windows x86 rows for stock Ricochet and `ricochet-updated`. Half-Life Deathmatch is an optional control. A template row expresses a test target. Compatibility and game-specific exports have to be established for the exact binaries in each run.

Metamod-R's README and current documentation list original HLDS support and recommend ReHLDS API 3.1 or later. Its upstream game-test repository describes dropping old WON-era games from its ReHLDS/Metamod-R testing. The latter describes a narrower test scope and potentially confusing Steam-only language. Treat Steam-era Valve HLDS, older WON builds, and ReHLDS API versions as distinct configurations. ReHLDS also warns that native modules relying on original HLDS internals can be binary-incompatible despite GameDLL/API compatibility claims. Record the precise build and keep crashes or missing exports as evidence for that row. [Metamod-R README](https://github.com/rehlds/Metamod-R#readme) · [Metamod-R documentation](https://rehlds.dev/docs/metamod-r/) · [Upstream game-test scope](https://github.com/rehlds/metamod-r-game-tests#readme) · [ReHLDS README](https://github.com/rehlds/ReHLDS#readme)

You need:

- A dedicated local test installation provisioned through Valve's Steam tooling, with legal copies of the appropriate mod assets, maps and stock GameDLL. SteamCMD users can provision Half-Life Dedicated Server (app 90) in a dedicated directory and select the appropriate mod. Check the installed files and the server's `version` output before configuring the matrix. The runner has no download step. [Valve SteamCMD documentation](https://developer.valvesoftware.com/wiki/SteamCMD)
- Independently provisioned x86 manager binaries and an x86 GameDLL for every selected row. Build `ricochet-updated` from the chosen revision and record that revision and build details. The upstream project targets VS2019, is archived, and provides no guarantee for a particular Linux build. Leave an unavailable build explicitly untested. [Classic Metamod source](https://github.com/alliedmodders/metamod-hl1) · [ricochet-updated source](https://github.com/twhl-community/ricochet-updated)
- Python 3.10+ and a host able to execute its OS's x86 binaries. Linux needs the installation's 32-bit runtime dependencies. Windows needs its GameDLL and Metamod dependencies. For `--build`, install the Rust i686 target and the repo's normal C/C++/libclang prerequisites, including the x86 Windows SDK on Windows. The Windows build uses the same MSVC target as the existing ABI checks.
- Trusted, stand-alone test binaries and configs, sufficient space for a full copy, and an unused loopback server port. The copied server executes its own startup configs, so use an installation with clean test configs and no external redirects, credentials, custom startup scripts or unrelated plugins. Staging provides filesystem isolation from the source installation through copying. It is a test environment, not an OS security sandbox. Use a separate OS account or VM if the runtime needs stronger isolation.

## Configure and run

From the repository root:

```sh
python3 verify/audit/runner.py template --hldm > /path/to/local/matrix.json
python3 verify/audit/runner.py run /path/to/local/matrix.json /path/to/new/report-dir \
  --only linux-hlds-classic-stock --build
```

Edit the generated rows before use. Keep only rows you intend to run, or select them with repeated `--only` arguments. `--platform linux` and `--platform windows` filter a shared matrix on native hosts. The same schema accepts new games by adding a row with a new `id`, its mod directory, compatible binaries, and maps. The template uses illustrative paths. Use actual platform-specific filenames, including the correct `liblist.gam` entry and engine module for your installation.

| Field | Meaning |
| --- | --- |
| `schema_version` | Configuration version `1` |
| `runtimes[].id` | Unique stable comparison key |
| `os`, `installation`, `mod` | Target OS, source tree relative to the configuration or absolute, and mod directory beneath that tree |
| `engine`, `metamod`, `gamedll` | Each entry has an independent `implementation`, exact `version` or source revision, and installation-relative `binary`. Optional `sha256` pins that artifact. |
| `engine.modules` | Additional relative paths to hash, such as `swds.dll` or `engine_i486.so`, when the launcher is separate from the engine |
| `engine.library_dirs` | Optional installation-relative directories to add to the staged library search path |
| `maps`, `port`, `timeout_seconds` | One to eight map names, a dedicated loopback port (default 27045), and per-readiness or command timeout (default 45 seconds) |
| `plugin` | Optional prebuilt plugin path relative to the configuration or absolute. Report its actual `audit_plugin_commit` and `sdk_commit` in the same row. An optional `plugin_sha256` pins the binary. |
| `experimental` | Records experimental scope in the report without changing how failures are evaluated |
| `unsupported_reason` | Optional evidence-backed exclusion. The runner records the row as unsupported and skips launch. |

Without `plugin`, `--build` compiles for the selected target into the output tree and records the repository HEAD and dirty state. Otherwise the runner looks in the standard Cargo release target directory. For a prebuilt binary with unknown provenance, commit fields remain null. Hashes identify actual files. Version labels are operator attestations, and the runner records matching lines from `version` and `meta version` separately for review. It does not derive any one component's identity from another.

The runner validates x86 binary headers and maps, rejects source-tree symlinks and reparse points, then modifies only its copy. It routes the staged mod's platform `liblist.gam` entry through the selected manager and supplies isolated `mm_configfile`, `mm_pluginsfile`, and `mm_gamedll` settings. It starts a LAN server on loopback, waits for the first-frame readiness event, then verifies console round trips. Later maps are requested with `changelevel` and checked for a new activation event before replaying the suites. Normal exit uses `quit`; timed-out processes are terminated as necessary. Standard output and error are captured separately, each capped at 64 MiB. Available staged crash dumps are copied with count and size limits. Dumps handled by a system crash collector outside the staging directory need to be collected using that system's tools.

Only use `--client-wait SECONDS` when an operator is available to connect a local game client. Neither player manipulation nor server shutdown uses a forced plugin unload. The server copy is removed after normal automated runs. An unavailable installation, missing readiness, failed console transport, process crash, protocol loss, sanitizer diagnostic, or FAIL yields a nonzero exit.

## Console and manual protocol

Use a real local console if the runtime fails to receive piped stdin or redirect output. In particular, a Win32 engine may use console APIs that bypass redirected stdin/stdout. Prepare an isolated copy with `run ... --only ID --prepare-only`. Its `launch.json` contains the exact argv, working directory, and environment additions. Start the command in that directory using those values. Preparation writes a `runtime.json` and retains the staging copy for the operator. Keep that copy local and remove it when done. The prepare-only exit code describes staging, and the report state is `PREPARED`.

In the dedicated server console:

1. Wait for the `ready` record and run `version`, `meta version`, `meta list`, and `mm_audit_status`. Confirm the selected manager and plugin actually loaded.
2. Run `mm_audit_run all "audit two words"`. The optional quoted marker tests argument preservation. Individual suites are `console`, `entities`, `trace`, and `resources`.
3. Change to a second configured map with `changelevel MAP`. Wait for activation and run the suite again. Keep the callback sequence around the transition.
4. Connect a client, enter the match, disconnect, and reconnect. Record whether the same slot is reused. Repeat on the same map for the slot-reuse probe. Lag-compensation callbacks are optional and depend on the host/GameDLL and gameplay. Play normally and record their presence if they occur.
5. Run `mm_audit_report`, then `quit`. Preserve the console transcript through exit, including stderr if available. The launch enables `-condebug` for builds that support it. If the engine does not forward shutdown, or the platform does not expose cleanup logs, leave those observations skipped.

Import one run's transcript and optional structured operator findings into a new report directory:

```sh
python3 verify/audit/runner.py import prepared/ID/runtime.json imported/ console.log shutdown.log \
  --manual operator-findings.json
```

The manual file is a JSON array. Each object contains `test` (a test name or contract finding ID from the report), `result`, `classification`, `observed` with reproducible supporting evidence, and `operator`. Use one entry per identifier. A manual result can fill an unobserved probe. Conflicting evidence is labeled and does not erase an observed failure. For manually collected logs, include the observed process exit status in `runtime.json` when known. Keep the original transcripts referenced by the imported report. The protocol parser expects one plugin instance and one server run per import.

## Interpretation

The plugin emits one `MM_AUDIT `-prefixed, versioned JSON record per observation. The runner builds `events.jsonl` in sequence order, attaches runtime and run IDs, and keeps each test's contributing sequence numbers in `report.json`. `report.md` summarizes identity, execution and test evidence. Missing probes get a reason and remain SKIP. A prior observed FAIL survives retries. The plugin caps its output at 4,096 events including a terminal budget marker, tracks at most 64 disconnected slots, and records no auth identifiers or arbitrary client text. The resource suite performs 64 owned reads, reports time without asserting a latency threshold, and the runner samples process RSS where Linux exposes it. These snapshots can inform comparisons and do not establish a leak rate.

| Result | Meaning |
| --- | --- |
| PASS | The required scenario was observed and met its local expectation |
| FAIL | A concrete expectation failed under the exercised conditions |
| SKIP | Required callback, configuration, or scenario was unavailable or unobserved |
| INCONCLUSIVE | The scenario partially occurred, but the evidence was insufficient for an assertion |

Classification is independent of result. Automatic observations are scoped to the exact runtime and classified as **Implementation-specific behavior**. **Verified portable behavior** requires corroborating observations across the claimed platforms and versions. **Unverified host assumption** covers contracts that safe SDK observations cannot establish. Confirm a failure's cause before attributing an **SDK defect** or **Host defect**. Use **Unsupported configuration** for documented or evidenced exclusions, including the exact affected version range. A missing executable alone only establishes lack of coverage.

```sh
python3 verify/audit/runner.py compare report-dir/report.json
python3 verify/audit/runner.py compare previous/report.json current/report.json
```

One report compares its runtimes. Two reports compare stable runtime IDs across runs and distinguish PASS-to-FAIL regressions from coverage loss. Same-result differences in trace fractions and optional identity or lag evidence can be legitimate across maps and GameDLLs. To annotate known result transitions, pass `--allowances FILE` with a JSON array. Entries specify `test`, `before`, `after`, `reason`, and either `runtime` for historical comparison or `left` and `right` for a matrix comparison. Allowances annotate differences without changing the underlying results.

## FFI evidence and CI

| Contract | Live observation | Native fixture or other evidence |
| --- | --- | --- |
| Strings and registration | Private CVar registration and write/read, retained owned copy, exact command marker. Local input bounds are tested separately. | Retained versus copied storage needs implementation evidence. The native tests cover oversized input and reentrant getter invalidation. |
| Edict arena and serials | Map lifecycle, stale handle rejection, optional player slot reuse. | Tests cover retirement during lookup, serial replacement and trace invalidation. Raw arena lifetimes and serial arithmetic remain host contracts. |
| Hook results and reentrancy | Lag pre/post delivery and original result availability, with ignored return decisions. | C++ asserts fixture hook status and nested-delivery ordering. The safe plugin has no result-slot or native pointer inspection. |
| Shutdown and cleanup | Cleanup and destructor entry if `GameShutdown` is forwarded. | The native fixture asserts no later command dispatch, and existing tests cover shutdown during an FFI call and panicking cleanup. |

Run `python3 -m unittest discover -s verify/audit -v` for reporting and process fixtures. The ordinary repository CI builds and loads the audit plugin in the Linux i686 and Windows x86 ABI checks. Linux retains the loader ASan/UBSan/LSan run and the combined Rust/C++ ASan run. No Valve runtime is needed for these tests. Those checks exercise adversarial scenarios without using a real server as a fault-injection tool.

The optional **GoldSrc runtime audit** workflow is dispatch-only on a provisioned self-hosted runner with the `goldsrc-audit` label. Enable `GOLDSRC_AUDIT_ENABLED` and configure the applicable `GOLDSRC_AUDIT_LINUX_CONFIG` or `GOLDSRC_AUDIT_WINDOWS_CONFIG` path. Provision Git, Python, PowerShell, the target Rust toolchain, and the corresponding native build dependencies. The workflow uploads structured reports only. Keep raw logs and dumps on the authorized runner since diagnostics can contain local process data.

When adding a probe, use a stable `*.vN` identifier with an explicit expectation. Bump its identifier version if the expectation changes. Add the scenario to the native harness when safe public callbacks cannot establish it, and exercise its reporting path. Cross-runtime behavior tests and sanitizer runs can expose contract violations, but successful observations cannot prove that the host always supplies valid native pointers or that every interleaving is memory-safe.
