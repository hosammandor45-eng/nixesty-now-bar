# Now Bar Pro development

Work on `feature/nowbar-pro`, PR #1. Keep upstream history and existing features.
Use the existing checkout; cloud tasks already have an isolated workspace.

## Provider migration

The production stack uses the portable engine's stable ordering. Activity-specific
ranks, expiry, urgent interruptions and user selection invalidation remain in
`activities/mod.rs`. Regression tests compare with the original algorithm across
1,210 combinations. Sorting no longer repeatedly searches a rank vector.

The engine now owns registration and cooperative lifetimes for **Explorer transfer**
and **screen-time wellbeing** providers. Both retain their original mailboxes,
Windows notifications, data models and activity priorities. Other workers retain
their existing implementations; this is an incremental migration, not a claim
that every worker has been migrated.

- Disabling transfers clears its current snapshot without announcing a completed
  transfer, then suspends the worker. Re-enabling starts a fresh polling interval.
- Disabled screen-time sampling blocks on commands instead of waking every 5 s.
  Its independent enable switch retains the original behavior.
- Stop interrupts long waits. UI teardown requests cancellation without waiting
  on an OS call. `shutdown()` is available off the UI thread to join and confirm
  cleanup. Forced process termination cannot guarantee a final data flush.
- COM initialization is balanced on the transfer thread; UIA resources drop first.
- Thread launch failures are logged; runtime failures/panics are contained and
  retained as diagnostics. The UI logs each error once. Failed workers are not
  automatically restarted into a crash/permission loop.
- The wellbeing worker saves its final partial sampling interval on normal stop.

## Functional interface

The existing Windows Composition bar/cards and Slint settings remain intact.
Compact/large card sizes, media playback, seek controls, taskbar positioning, blur
fallback and DPI scaling remain the production paths.

Settings use Fluent-inspired dark/light surfaces, blue focus/accent colors, shared
spacing and typography, and a persistent compact density option on General.
Narrow windows use smaller navigation and inspector columns. Viewport width is
supplied outside layout negotiation to avoid a circular width constraint.
The actual media card now emphasizes play/pause with a larger target and ignores
unsupported transport controls. Playback/seek/output selection use existing
Windows media commands, not a simulated player.

Slint control animations follow the saved Off preference and Windows' accessibility
animation setting. Native card transitions also respect the Windows preference.
The dark/light palette changes apply to the settings shell; native activity cards
retain their existing content-specific materials and are not all light-themed.

## Arabic and RTL

Arabic is selectable alongside Auto, Turkish and English; the choice is saved in
`settings.json`. Auto detects an Arabic Windows UI language; `NOWBAR_LANG=ar` is
available for developer captures. Both native text and Slint use the shared
catalog. The initial Arabic catalog translates 72 entries, with explicit English
fallback for the remainder and Arabic plural metadata. This is not a full Arabic
translation of every feature.

The settings shell reverses pane order for RTL, inspector label/control rows
reverse, and labels align to the reading direction. Segoe UI supplies Arabic
coverage on Windows 10/11. DirectWrite text uses Arabic reading direction and
system glyph fallback, while icons keep their original direction. Media transport
order and seek progression remain consistent with the player's timeline.
The installer bundles Arabic too; its shell has not yet been fully mirrored.

## Linux validation

Install stable Rust with rustfmt and clippy, then run:

```sh
cargo test --locked --manifest-path tools/portable-check/Cargo.toml
cargo clippy --locked --manifest-path tools/portable-check/Cargo.toml --all-targets -- -D warnings
rustfmt --edition 2021 --check src/activities/engine.rs
```

The independent portable workspace compiles the actual engine, timer and i18n
source files; it does not stub Windows APIs. Its build script generates real
English/Arabic tables. Tests cover lifecycle, suspension, cleanup, provider errors,
ordering, timer behavior, persisted language choice, translation coverage and
placeholder preservation. Do not reformat unrelated upstream source.

The Slint compiler can validate `ui/main.slint` independently of desktop runtime
libraries. A full Linux `cargo check -p nowbar-ui` additionally needs the native
Slint desktop dependencies, including fontconfig development metadata.
The Windows application itself directly imports WinRT/DWM/Direct2D and its build
script emits MSVC manifest options; a successful portable test is not an MSVC build.

## Windows CI and artifacts

The workflow runs MSVC stable on `windows-latest`, verifies the Windows SDK resource
compiler, caches dependencies, and runs Clippy and workspace tests in the documented
quick profile. It builds both release executables with thin LTO (the upstream
distribution profile uses thin LTO to avoid fat-LTO memory exhaustion).
Full-tree formatting remains a visible nonblocking upstream baseline; new engine
formatting is mandatory. Linux portable tests also run independently in CI.

The release settings binary renders all 11 real settings pages plus search using
the software renderer in English dark/expanded and Arabic light/compact at 150%
DPI. Capture errors, nonzero exit status, timeout or missing images fail the job.
CI uploads both executables together and a separate screenshot artifact. These
are validation artifacts, not a packaged installer; `tools/make-setup.ps1` remains
the packaging path. Review screenshots for visual clipping and Arabic shaping.

Interactive Windows 10/11 checks still need real taskbar, Bluetooth/media devices,
Explorer activity, multi-monitor DPI changes, and accessibility/theme changes.
Only actual CI results establish Windows compilation/test success.

## License

Source remains GPL-3.0-only. Preserve upstream copyright, the bundled Inter SIL
Open Font License, and README's Samsung/Anthropic icon notices. No third-party
artwork or new font assets are introduced.
