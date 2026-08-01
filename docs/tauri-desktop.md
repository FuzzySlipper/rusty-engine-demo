# Loading Bay Tauri desktop package

Loading Bay has one Tauri 2 shell around the existing product. It does not contain a second
gameplay or rendering implementation:

```text
Tauri Rust lifecycle owner
  -> packaged browser-host sidecar on an ephemeral 127.0.0.1 port
  -> existing relative HTTP and WebSocket protocol
  -> existing Angular loading-bay production build in one WebView
```

Rust owns sidecar launch, the loopback bind requirement, a 15-second readiness and health bound,
resource verification, user-data locations, logging, crash handling, and shutdown. TypeScript has
no Tauri shell, process, or filesystem permission. The main remote WebView capability grants no
Tauri commands.

## Build inputs and outputs

All Tauri, plugin, and CLI versions are exact in `src-tauri/Cargo.toml`, `Cargo.lock`,
`package.json`, and `pnpm-lock.yaml`. Build from a checkout of the public Demo repository; a sibling
Engine checkout is neither read nor accepted.

```bash
pnpm install --frozen-lockfile
pnpm run test:tauri
pnpm run build:tauri:binary
pnpm run smoke:tauri
```

`prepare:tauri` builds Angular, builds `browser-host` for the selected Rust target, and creates a
canonical manifest containing the exact Git revision, byte length, and SHA-256 digest of every
web/content resource plus the sidecar. Tauri runs that preparation automatically before a release
build, and `test:tauri` runs it before checking the generated manifest. Generated binaries and
manifests are build outputs, not committed inputs. Ordinary non-release Cargo workspace checks
compile the desktop crate with bundle resources disabled; release builds retain the complete
configuration and fail if the prepared sidecar or resource tree is absent.

The direct Linux layout is:

```text
target/release/loading-bay-desktop
target/release/loading-bay-browser-host
target/lib/loading-bay-desktop/
  desktop-package-manifest.json
  web/
  content/
```

Moving only the first executable is deliberately unsupported: startup fails closed when the
manifest, sidecar, canonical project, or an asset is missing, has a different size, or has a
different hash.

The installable packages are built with:

```bash
pnpm run build:tauri
```

The supported reproducible bundle baseline is the `verify-tauri` GitHub job on Ubuntu 22.04 with
WebKitGTK 4.1 development packages, `patchelf`, and Tauri's Linux bundler. It produces and uploads
the deb, AppImage, direct binary layout, native-smoke JSON, and WebView screenshot. An Arch package
build is useful for the direct binary and deb, but it is not the portability baseline: the current
linuxdeploy strip binary cannot read Arch's RELR sections, so an Arch AppImage failure must not be
represented as a product failure or a portable artifact.

## Runtime locations and security

The shell resolves Tauri's platform application directories rather than the current directory or
environment-provided project paths:

- application data: save slots and WebKit durable data;
- application cache: atomic sidecar readiness;
- application logs: `browser-host.log`.

The sidecar receives explicit absolute packaged paths and a cleared environment. It binds
`127.0.0.1:0`, publishes an atomic PID/address/content-hash receipt, and rejects a non-loopback
address before binding. The WebView accepts only that exact origin (plus WebView-owned `about:` and
`data:` documents). HTTP responses include a restrictive CSP, `Referrer-Policy: no-referrer`, and
`X-Content-Type-Options: nosniff`. The CSP permits Angular's exact hashed production stylesheet
activation handler and runtime component styles; the native smoke fails if the optimized
stylesheet remains in its pre-load `print` state.

Normal exit removes the sidecar once. A host crash exits the shell, and a shell crash is detected
by the host's parent monitor so the host cannot survive as an orphan. `smoke:tauri` exercises all
four paths with the compiled application through WebKit WebDriver, also proving the Rust-owned menu,
New Game path, and first retained renderer submission.

## Local deployment

The supported deployment consumes an exact reviewed Debian artifact; it never rebuilds from the
checkout being used to install it. Supply the artifact SHA-256 and the full source revision from
the same immutable CI run:

```bash
pnpm run deploy:tauri -- install \
  --artifact /absolute/path/to/Loading\ Bay_0.1.0_amd64.deb \
  --artifact-sha256 <64-hex-deb-digest> \
  --evidence /absolute/path/to/tauri-package-evidence.json \
  --source-revision <40-hex-source-revision>
pnpm run deploy:tauri -- status
```

The installer verifies the Debian digest before extraction, the embedded manifest identity, every
packaged resource, and the sidecar. Tauri's Debian bundler may strip the main executable, so the
receipt deliberately records three distinct identities: the exact Debian artifact, the executable
actually installed from it, and the direct-build executable reported by CI. Treating those bytes
as interchangeable would make a valid package impossible to install and would hide which artifact
was actually deployed.

The installation is entirely user-scoped:

| Surface       | Default location                                                        |
| ------------- | ----------------------------------------------------------------------- |
| Releases      | `$XDG_DATA_HOME/rusty-engine-demo/desktop/releases/<source>-<artifact>` |
| Active/backup | atomic `current` and `previous` symlinks under the desktop install root |
| Launcher      | `$XDG_BIN_HOME/loading-bay`                                             |
| Desktop entry | `$XDG_DATA_HOME/applications/loading-bay.desktop`                       |
| Save data     | `$XDG_DATA_HOME/dev.fuzzyslipper.rusty-engine-demo.loading-bay/saves`   |
| Cache/logs    | the matching platform application directories returned by Tauri         |

`rollback` atomically exchanges the active and previous release. `uninstall` removes only the
managed launcher, desktop entry, icons, and immutable release tree; it preserves application data,
saves, cache, and logs. Data removal requires the explicit `uninstall --purge-data` command. The
installer refuses to remove an entry point that lacks its ownership marker.
It likewise refuses to replace an unmanaged launcher or desktop entry during install.

```bash
pnpm run deploy:tauri -- rollback
pnpm run deploy:tauri -- uninstall
pnpm run deploy:tauri -- uninstall --purge-data # explicit destructive reset
```

`pnpm run certify:tauri-deploy` certifies the active install. It drives the absolute installed
binary through Tauri 2 WebDriver from a temporary working directory, captures full and 960×540
screenshots, checks New Game/first frame, renderer disposal/remount, singleton delegation,
focus loss plus the native show/unminimize/focus activation receipt, WebKit/WebGL identity,
native process-tree RSS and idle activity,
normal/crash cleanup, a visible fail-closed startup screen, and then runs the unchanged complete
campaign against the installed sidecar and its installed Web bundle.
The native callback always requests show, unminimize, native focus, and WebView focus on the existing
window. The bounded cache receipt proves those requests were issued; the evidence records the window
manager's resulting visible/minimized/focus state without treating an OS focus grant as application
authority. The secondary process must terminate within the bounded wait and may not start a host;
its exit code and bounded stdout/stderr are retained because Linux DBus/WebDriver teardown can return
a nonzero plugin-cleanup status after successful delegation.
Set `--skip-campaign` only for focused iteration; that result is explicitly recorded as skipped and
is not release certification. The `verify-tauri` GitHub job performs the exact install and complete
certification after building the Debian package, and uploads the receipts and screenshots.
