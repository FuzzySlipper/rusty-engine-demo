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

DT2 owns the user-scope installation and rollback scripts, launcher integration, cold/restart
campaign, native resource measurements, and final screenshots. It consumes the exact reviewed DT1
artifact identities rather than rebuilding from mutable source during installation.
