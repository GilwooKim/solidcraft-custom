# SolidCraft Custom

Windows customization of [SolidCraft](https://github.com/tangency-labs/solidcraft), based on upstream commit `18dfe20b` (0.44.0). Original copyright, attribution and license files are retained. This is an independent distribution, not an official upstream release.

Repository and releases: https://github.com/GilwooKim/solidcraft-custom

## Installation and updates

Download the `solidcraft-<version>-windows-x64-setup.exe` release asset. The installer supports English and Korean, installs under the current user's LocalAppData, and creates a Start Menu shortcut. Developer tools and GitHub login are not required on employee PCs.

The installed app checks this repository at startup and daily while open. For this per-user Windows installation, it downloads a newer stable release's portable ZIP, verifies SHA-256 against that release's `SHA256SUMS.txt`, and replaces the binaries using the existing updater. The current session stays open; the new version runs at the next launch. The user can turn off automatic installation in General preferences while keeping notifications. Offline checks retry later; failed installations do not retry every frame. An external launcher or machine-wide MSI installation is never auto-installed by this feature.

The installer is currently unsigned. Checksums protect against corrupt downloads; release integrity depends on the GitHub repository account and HTTPS. No token is included in the software.

## Development

Run `./Start-SolidCraft.ps1 -Sample` to build and launch a sample. The checkout uses the toolchain pinned by upstream.

* UI and layout: `crates/ui-egui`, shared controls in `crates/ui-kit`.
* Korean translations: `assets/locales/ko.json`, new strings in `assets/locales/pending`.
* Modeling commands: `crates/engine`; geometry implementation: `solidcore`.
* STEP/STL and other file formats: `crates/io`.
* Parameter-driven example: `examples/custom-plate.json` (80 x 50 x 6 mm, central 8 mm through-hole). Expected volume: `(80*50 - pi*4*4)*6` cubic millimetres.

## Building a release

1. Update `packaging/windows/custom-version.txt` to a strictly newer stable version.
2. Run the relevant tests, then `./packaging/windows/build-custom.ps1 -Iscc <path-to-ISCC.exe>`. Inno Setup 6 and Visual Studio C++ build tools are needed on the build machine.
3. Test the installer and the portable ZIP. Outputs and checksums are under `dist/custom/<version>`.
4. Commit the reviewed source and push a matching `v<version>` tag. `Custom Windows release` builds and publishes the installer, update ZIP and checksums. A manual workflow dispatch produces artifacts without publishing.

`SOLIDCRAFT_VERSION` is embedded by the packaging script; ordinary development builds do not contact the release feed. The custom installer has its own identity and folder, independent of upstream's MSI. Original upstream workflows are preserved under `.github/upstream-workflows` and do not run in this repository.
