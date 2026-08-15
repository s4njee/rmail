# Linux packaging (E2.1)

Tauri produces the `.deb` and AppImage bundles automatically during
`tauri build` on Linux (see `.github/workflows/release.yml`); they need the
WebKitGTK system packages listed in that workflow.

**Flathub** is the real Linux channel, so beyond the AppImage/deb there is a
Flatpak submission manifest at `flatpak/app.quill.Quill.json`. It is a **WIP
skeleton** — the module must build `quill` from source inside the GNOME SDK
(cargo + pnpm + WebKitGTK) rather than ship a prebuilt binary, and it must be
validated with `flatpak-builder` before submitting:

```
flatpak install -y flathub org.gnome.Sdk//48 org.gnome.Platform//48
flatpak-builder --user --install build-dir flatpak/app.quill.Quill.json
```

Until the manifest is finished, the AppImage + `.deb` from CI are the
distributable Linux artifacts.
