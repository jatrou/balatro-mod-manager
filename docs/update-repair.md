# Update repair for the maintained fork

## Confirmed causes

The archived upstream README explicitly says its hosted services are being shut down. The inherited application depended on `api-bmi.dasguney.com` for catalog metadata, versions, and download resolution, and its app update links still pointed at the original project. Merely refreshing the existing cache could not repair those dependencies.

Lovely [v0.10.0](https://github.com/ethangreen-dev/lovely-injector/releases/tag/v0.10.0) changed the Windows injector from `version.dll` to `winmm.dll`. Its [installation instructions](https://github.com/ethangreen-dev/lovely-injector/blob/v0.10.0/README.md) also change the Proton/Wine override to `winmm=n,b`. The old manager searched its downloaded archive for `version.dll`, so it could download the current release and still fail installation.

Removal requests are evidenced by upstream index PRs [#846](https://github.com/skyline69/balatro-mod-index/pull/846) and [#876](https://github.com/skyline69/balatro-mod-index/pull/876). This repair does not depend on the Reddit allegations about intent or retaliation.

## Catalog behavior

The default source is [kasimeka/balatro-mod-index](https://github.com/kasimeka/balatro-mod-index), identified as the continuation of Skyline's index by the community [index of indices](https://github.com/balala-modding-zone/mod-index-index). Catalog refresh reads a complete Git tree and fetches metadata at that exact revision. Unchanged metadata can be reused; removed directories disappear from the catalog. Incomplete trees and failed network fetches do not replace a working cache. Malformed individual metadata is quarantined with a logged warning so one broken entry cannot prevent the whole catalog from refreshing.

Installed mods that leave the catalog remain on disk and are treated as local/orphaned mods. A stale catalog ID must resolve successfully against the maintained source before installation. The app does not resurrect a removed mod by guessing its old branch download URL. Offline cache availability does not guarantee that a mod remains eligible for installation.

Download counts from the retired service are unavailable. They must not be presented as current popularity statistics. Catalog browsing, descriptions, and downloads use the public index and each mod's own download location.

The **Awesome Balatro** section supplements the main catalog with the mod listings from [jie65535/awesome-balatro](https://github.com/jie65535/awesome-balatro). It can prepare supported GitHub packages, archives, and individual Lua mods for installation. Discord downloads prompt for a downloaded file or folder. The review step checks the staged package against installed mods before installation; it does not overwrite an existing mod. Withdrawn entries and special loader/game-patching setups retain source instructions. Inclusion in a curated list is not a claim of compatibility with current Balatro or Steamodded. See [coverage notes](awesome-balatro-coverage.md) for installation behavior and limits.

## Installing this repair

Build this source, then run the new BMM executable or install its generated package. Updating Lovely or a mod inside an older BMM executable does not update the manager's code. The repaired app identifies itself as version **0.4.2**. Application update checks use published stable releases with downloadable assets from `jatrou/balatro-mod-manager`; inherited Git tags alone do not trigger an update notice.

Keep a backup of your Mods directory and manager database before adopting a new build. The repair's checks use isolated temporary fixtures; they do not update a live game installation as part of the build.

On Windows, use the MSVC Rust toolchain and Visual Studio C++ build tools. If another Rust installation shadows rustup on `PATH`, prefix the session with `$env:USERPROFILE\.cargo\bin` before building:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;" + $env:PATH
$env:BMM_DATA_DIR = Join-Path $env:TEMP "bmm-validation"
bun install --frozen-lockfile
bun run check
bun run test
bun run lint
bun run build
cargo +stable test -q -p bmm-lib -- --test-threads=1
cargo +stable test -q -p balatro-mod-manager -- --test-threads=1
Remove-Item Env:BMM_DATA_DIR
bun run tauri build --target x86_64-pc-windows-msvc
```

An absolute `BMM_DATA_DIR` overrides the configuration root for development and smoke tests. The app stores its normal `Balatro` directory under that root and caches under its `cache` child. Unset it to use the normal installed profile. It is not a complete sandbox: game discovery can still find Steam installations, so use a temporary game path and avoid launching or modifying a real game during a smoke test.

## Verification

Validation on Windows x64, October 23, 2026:

- Rust workspace: 169 app tests and 111 library tests passed; the two opt-in network tests also passed when run explicitly. Three documentation examples remain intentionally ignored.
- `cargo fmt --all --check` and workspace Clippy with all targets and warnings denied passed using the stable MSVC toolchain.
- Frontend type checking (zero errors/warnings), linting, 135 unit tests, and production static build passed.
- The production Windows x64 NSIS build succeeded and produced `target/release/bundle/nsis/Balatro Mod Manager_0.4.2_x64-setup.exe`. This local installer is unsigned and was not installed over the user's existing manager during validation.
- Live catalog revision `ee9c1d2dcc48530894ae61e007aa646289941f58` contained 448 metadata files and 447 valid entries. `Rageinater@Quality` had an unterminated JSON download URL and was quarantined. The removal regression verified stale Multiplayer data is dropped.
- The official Lovely 0.10.0 Windows archive installed a valid `winmm.dll` into a temporary game fixture. In the built Tauri app, installation detection then returned true and the update check returned no pending update.
- The built app browsed the live main catalog, refreshed and searched Awesome Balatro, opened Cartomancer through its exact catalog match, and downloaded Cartomancer into the isolated Mods directory. The directory showed 321 mods, 33 current catalog matches, and 288 source/manual entries at this revision.
- Hard navigation to `/main/` was checked after changing asset paths to root-relative URLs; the app renders and canonicalizes the route instead of loading a blank page.

The desktop smoke run used a separate app identifier, absolute `BMM_DATA_DIR`, and separate `WEBVIEW2_USER_DATA_FOLDER`. No live Balatro mods or saves were changed, and no game was launched. This verifies manager installation/update operations, not in-game compatibility of every mod. Native macOS and Linux execution remain unverified locally. The existing renderer emits inline-style CSP warnings; the tested navigation and installation flows still worked. The fork's release endpoint returns 404 until an installer release is published, which is handled as no available app update.
