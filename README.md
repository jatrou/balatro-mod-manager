# [![Balatro Mod Manager](images/title.svg)](#)

This is [jatrou's continuation](https://github.com/jatrou/balatro-mod-manager) of Balatro Mod Manager, originally created by _Skyline_. The original project and its hosted services were discontinued. This fork restores updating without relying on those services.

## Changes in 0.4.2

- The interface uses a compact, solid dark theme and the system font. Awesome Balatro categories form the sidebar. Every mod list has live fuzzy search, preview visibility controls, and a shared page size of 10, 25, 50, 100, or All, saved in Settings. Clicking a mod opens its GitHub repository when available; install and details actions remain separate. GitHub stars can be used to sort lists. The About and Submit Mod screens have been removed.
- GitHub stars and source health are bundled as a dated snapshot. Confirmed missing Awesome sources are excluded from bundled, cached, and refreshed lists; inaccessible Discord links remain available. Run `bun scripts/refresh-catalog.ts` to update the public source checks and star counts. See [UI verification](docs/ui-refresh.md) and [catalog coverage](docs/awesome-balatro-coverage.md).
- Lovely 0.10 uses `winmm.dll`. Installation, detection, enable/disable, and Proton launch settings now handle the new filename and retire the legacy `version.dll` after a successful update.
- The catalog comes directly from the maintained [kasimeka/balatro-mod-index](https://github.com/kasimeka/balatro-mod-index). Refreshes reconcile removals as well as additions and version changes. Existing local mods remain on disk when removed from the index.
- Mod archives are extracted and checked before replacing an installed mod. Failed replacement restores the previous directory, and disabled mods stay disabled after updating.
- The Awesome Balatro directory includes 310 mods and four loader references after removing 11 confirmed unavailable sources. Supported GitHub packages, archives, and individual Lua mods can be installed from the directory. Discord downloads open their source and prompt for a local file or folder. A review step shows the destination, declared conflicts, missing dependencies, and duplicate installations before writing to Mods. See [installation coverage](docs/awesome-balatro-coverage.md) for special setups and limits.
- Application update notices use this fork's published releases. The retired telemetry and report-upload services are no longer used.

Build this fork to obtain these changes; an installed copy of the original manager does not acquire them automatically. Releases from the original project's package repositories are also unaffected. See [repair and validation notes](docs/update-repair.md) for sources, migration behavior, and test coverage.

Balatro Mod Manager is a standalone tool made for [Balatro](https://store.steampowered.com/app/2379780/Balatro/) that makes finding, downloading, and installing mods easy.

<p align="center">
    <a href="https://star-history.com/#jatrou/balatro-mod-manager&Date">
        <picture>
            <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=jatrou/balatro-mod-manager&type=Date&theme=dark" />
            <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=jatrou/balatro-mod-manager&type=Date" />
            <img width="75%" alt="Star History Chart" src="https://api.star-history.com/svg?repos=jatrou/balatro-mod-manager&type=Date" />
        </picture>
    </a>
</p>

![GitHub Downloads (all assets, all releases)](https://img.shields.io/github/downloads/jatrou/balatro-mod-manager/total)
![GitHub License](https://img.shields.io/github/license/jatrou/balatro-mod-manager)
![GitHub Tag](https://img.shields.io/github/v/tag/jatrou/balatro-mod-manager)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/jatrou/balatro-mod-manager/ci.yml)

![Rust](https://img.shields.io/badge/rust-%23000000.svg?logo=rust&logoColor=white)
![TypeScript](https://img.shields.io/badge/typescript-%23007ACC.svg?logo=typescript&logoColor=white)
![Fortran](https://img.shields.io/badge/Fortran-%23734F96.svg?logo=fortran&logoColor=white)
![Tauri](https://img.shields.io/badge/tauri-%2324C8DB.svg?logo=tauri&logoColor=%23FFFFFF)
![Svelte](https://img.shields.io/badge/svelte-%23f1413d.svg?logo=svelte&logoColor=white)

# [![Download](images/download.svg)](https://github.com/jatrou/balatro-mod-manager/releases/latest)

Balatro Mod Manager targets Windows, macOS, and Linux. This fork's repair has been built and smoke-tested on Windows; see the validation notes for platform coverage.

> Note: The Balatro Mod Manager is **NOT** compatible with the Xbox Gamepass version of Balatro

Published installers will appear in [this fork's releases](https://github.com/jatrou/balatro-mod-manager/releases). Until a release is published, build from this repository. The installation examples below describe the respective package formats.

Scroll down to find **▸Assets** and download the right version of the installer for your system.

- Windows: `Balatro.Mod.Manager_…_x64-setup.exe` or `Balatro.Mod.Manager_…_x64_en-US.msi`
- macOS: `Balatro.Mod.Manager_…_universal.dmg`
- Linux: Flatpak (recommended), AUR, or `Balatro.Mod.Manager_…_amd64.AppImage`

### Installing on macOS

The macOS build is signed ad-hoc (not notarized by Apple), so Gatekeeper blocks the first launch. After installing once, the app opens normally on every subsequent launch.

1. Mount the `.dmg` and drag **Balatro Mod Manager** into `/Applications`.
2. Try to open it. macOS will refuse with either _"Apple cannot verify..."_ or _"is damaged and can't be opened"_. Close the dialog.
3. Pick one of the following bypasses:

   **Option A — System Settings (macOS 14 and newer, no Terminal):**
   - Open **System Settings → Privacy & Security**.
   - Scroll to the **Security** section. You should see a line about Balatro Mod Manager being blocked, with an **Open Anyway** button. Click it and confirm.
   - If no button appears (macOS 15+ sometimes hides it for "damaged" bundles), use Option B.

   **Option B — Terminal one-liner (works on every macOS version):**

   ```bash
   xattr -dr com.apple.quarantine "/Applications/Balatro Mod Manager.app"
   ```

   Then double-click the app normally.

   **Option C — Older macOS (13 and below):** right-click the app → **Open** → confirm in the dialog.

Why this happens: shipping notarized builds requires an active Apple Developer Program membership ($99/year). The app is ad-hoc signed so it runs on Apple Silicon, but Apple's notarization service is what would silence Gatekeeper. The friction is one-time per install.

### Arch Linux (AUR)

The original AUR packages may still point to the discontinued upstream and do not automatically include this repair. Check the package source before using your preferred AUR helper:

```bash
yay -S balatro-mod-manager-bin
# or
paru -S balatro-mod-manager-bin
```

### Debian / Ubuntu

Download the `.deb` from the [latest release](https://github.com/jatrou/balatro-mod-manager/releases/latest) and install:

```bash
sudo apt install ./Balatro.Mod.Manager_*_amd64.deb
```

## Flatpak (Steam Deck/Linux)

> You need [flatpak-builder](https://docs.flatpak.org/en/latest/flatpak-builder.html) for this.

- Run from a local checkout:
  ```bash
  git clone https://github.com/jatrou/balatro-mod-manager.git
  cd balatro-mod-manager
  ```
- Install runtimes once (GNOME 50 + toolchain extensions):
  ```bash
  flatpak install org.gnome.Platform//50 org.gnome.Sdk//50 \
    org.freedesktop.Sdk.Extension.node20//25.08 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08
  ```
- Build + bundle from this repo:
  ```bash
  flatpak-builder --force-clean --repo=repo build-dir packaging/flatpak/io.balatro.ModManager.json
  flatpak build-bundle repo balatro-mod-manager.flatpak io.balatro.ModManager master
  ```
- Install/run (on the Deck or any Flatpak host):
  ```bash
  flatpak install --user balatro-mod-manager.flatpak
  flatpak run io.balatro.ModManager
  ```

# [![Build](images/build.svg)](#build-prerequisites)

Alternatively, if you would prefer to build Balatro Mod Manager yourself instead of downloading the [prebuilt installer](https://github.com/jatrou/balatro-mod-manager/releases/latest), Balatro Mod Manager can be compiled from source using the instructions below.

## Build Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (for the backend)
- [Bun](https://bun.sh/) (for the frontend)
- [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites#installing-the-tauri-cli)
- [Task](https://taskfile.dev/) (for running task commands)
  - **Arch Linux**: `sudo pacman -S go-task` (package is named `go-task` to avoid conflicts)
  - **Fedora**: `sudo dnf install go-task`
  - **Ubuntu/Debian**: Install via the [official script](https://taskfile.dev/installation/#install-script) or Snap: `sudo snap install task --classic`
  - **macOS**: `brew install go-task`
  - **Windows**: `choco install go-task` or `winget install Task.Task`

## Linux Flatpak helper

For Windows and macOS source builds, use the manual local-checkout workflow below. The legacy one-line installer scripts are not part of this fork's validated release workflow.

For Linux, run the Flatpak helper from a local checkout so you can review the script. It prefers a published Flatpak; if none is available, its build fallback uses your current checkout:

```bash
./scripts/linux-install.sh
```

The script now prefers the latest GitHub Release Flatpak if available, and falls back to a local build if not.

Launch after install:

```bash
flatpak run io.balatro.ModManager
```

The Linux Flatpak helper requires `flatpak` and `flatpak-builder`.
On Linux the manager currently supports the Steam version of Balatro only.

## Manual Installation

1. Clone the repository & install bun's dependencies:
   ```sh
   git clone https://github.com/jatrou/balatro-mod-manager.git
   cd balatro-mod-manager && bun install --allow-scripts
   ```
2. Run the task based on your OS
   - For Windows:
     ```sh
     task release-windows
     ```
   - For macOS:
     ```sh
     task release-macos
     ```
   - For Linux:
     ```sh
     task release-linux
     ```

## Running the Project

### Development Mode

To start the project in development mode, use the provided taskfile:

1. Run the debug target:
   ```sh
   task debug
   ```

> Linux/Wayland: on Wayland sessions the app now prefers X11 (XWayland) to avoid compositor protocol errors. Set `BMM_ALLOW_WAYLAND=1` before running if you want to keep native Wayland.

### Production Mode

To build the project for production:

1. Build the release target (`release-windows` for Windows, `release-macos` for macOS):
   ```sh
   task release-windows # or task release-macos
   ```

The built application will be located under the workspace's `target/<target-triple>/release` directory when the build specifies a target, or `target/release` otherwise. The installer paths are shown at the end of the build process.

## Cleaning the Build

To clean the build files, use the provided taskfile:

1. Run the clean target:
   ```sh
   task clean
   ```

> Font by Daniel Linssen

# Code Signing

This fork does not inherit the original project's SignPath signing arrangement. The locally built Windows 0.4.2 installer is unsigned; Windows may show an unknown-publisher prompt. Signing status for future published installers will be stated in their release notes.
