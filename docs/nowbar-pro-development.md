# Now Bar Pro: first development milestone

Development branch: `feature/nowbar-pro`. The application remains the existing
Windows application, with Windows Composition bar/cards and Slint settings.

## Implemented

- `src/activities/engine.rs` owns platform-independent stable stack ordering.
  Existing activity-specific ranks, expiry rules, user selection invalidation,
  and urgent interruptions remain in effect. Sorting records directly removes
  the previous repeated linear rank lookups (O(n²)); sorting is O(n log n).
- A provider lifecycle state machine and adapter trait define startup, readiness,
  suspension, failure, and cleanup. Invalid transitions are rejected without
  mutation. Existing OS workers have **not** yet been migrated to adapters;
  their polling and resource ownership stay unchanged in this milestone.
- Slint's existing dark/light theme now exposes shared spacing, body typography,
  control radius, and motion tokens consumed by real settings controls. The
  existing animation-speed Off setting immediately disables those control
  transitions, including when settings are loaded or reset.

## Linux checks

Install stable Rust with rustfmt and clippy. With `rustc` and `rustfmt` on PATH:

```sh
rustfmt --edition 2021 --check src/activities/engine.rs
rustc --edition 2021 --test -D warnings src/activities/engine.rs -o /tmp/nowbar-engine-tests
/tmp/nowbar-engine-tests
cargo fmt --all -- --check
```

The dependency-free engine file is both the production module and the portable
Rust test target. Its regression test compares scheduling with the original
implementation across 1,210 combinations. No Windows modules are stubbed out.
Full-tree formatting currently fails on upstream formatting; the Windows CI
reports that existing baseline without blocking compilation. Formatting of the
new engine is mandatory. A repository-wide formatting change is separate work.

`cargo check --locked -p nowbar-ui` can check the Slint UI on Linux when native
Slint desktop dependencies (including fontconfig development files) are present.
This is not a check of the Windows app. Root build.rs emits MSVC manifest linker
arguments unconditionally, and the application directly imports WinRT, DWM,
Direct2D, Bluetooth, Explorer, and Windows media APIs. Do not assume other modules
are portable merely because their files contain Rust logic.

## Windows validation

`.github/workflows/windows.yml` runs on Windows with stable x64 MSVC, rustfmt,
Clippy, dependency caching and an explicit resource compiler availability check.
The hosted image supplies Visual C++ tools and Windows SDK. Repository aliases
run Clippy and workspace tests in the quick profile, as documented upstream.
Release compilation targets `nowbar` and `nowbar-settings`; both executables
are uploaded together. The installer is compiled by workspace Clippy/tests;
packaging an installable payload remains the existing `tools/make-setup.ps1`
workflow and is not replaced by CI artifacts.

A green Windows job establishes compilation/test results, not desktop visual or
hardware behavior. Interactive Windows 10 and 11 checks still need to cover
media controls, taskbar placement, compact/expanded cards, multi-DPI monitors,
blur fallback, theme changes and reduced motion.

## Preserved and remaining work

The existing bar already supports compact/expanded cards, media controls,
progress, positioning, high DPI, rounded corners, and compositor materials with
Windows-version handling. This iteration does not claim a complete premium
visual redesign, Mica support, Arabic translations or RTL layout. Follow-on work
should migrate one provider at a time, add snapshot delivery/cancellation, and
introduce Arabic direction and shaping tests before exposing an Arabic selector.

The source remains GPL-3.0-only. Keep the upstream copyright, bundled Inter SIL
Open Font License, and README's Samsung/Anthropic icon notices. No new external
artwork, fonts, or icon assets were introduced.
