# theme-switcher

**Windows dark mode that follows the sun.**

Light theme at sunrise and dark theme at sunset, based on where you actually
are. It's one small exe with no window, tray icon or settings. Run it once and
forget about it.

[![build](https://github.com/dincertekin/theme-switcher/actions/workflows/build.yml/badge.svg)](https://github.com/dincertekin/theme-switcher/actions/workflows/build.yml)
[![release](https://img.shields.io/github/v/release/dincertekin/theme-switcher)](https://github.com/dincertekin/theme-switcher/releases/latest)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![platform: windows](https://img.shields.io/badge/platform-windows-0078D6?logo=windows&logoColor=white)](https://github.com/dincertekin/theme-switcher)

## Why

macOS switches its appearance at sunset. Windows doesn't. It has a manual
toggle and nothing more, even though Night Light already knows when your sun
goes down.

**What about PowerToys?** I know about PowerToys and its Light Switch module.
It's great, but I didn't want to install a whole suite to get one feature. I
believe a program should do one job and do it well. This one does just that.

## Features

- **Accurate to the minute, anywhere.** It uses the Windows Location API, the
  same one Night Light uses, and NOAA's solar equations, computed locally. It
  works in every timezone, including half- and quarter-hour offsets and DST,
  because the math never touches your clock's timezone.
- **Even near the poles.** It handles polar day and polar night, and ignores
  the few-minute sunrises at the edge of polar night instead of flashing your
  screen.
- **No setup.** The first run adds it to startup. That's the whole install.
- **Invisible.** No window, no tray icon, no console, no background service.
- **Works offline.** It remembers your last location, so a boot without
  internet still follows the sun.
- **Leaves your choice alone.** If you switch the theme by hand, it won't
  switch it back until the next sunrise or sunset.
- **Tiny.** One dependency (Microsoft's `windows` crate) and a size-optimized
  release build.

## Install

Works on Windows 10 and 11. No runtime or redistributable needed.

1. Download the exe for your PC from the
   [latest release](https://github.com/dincertekin/theme-switcher/releases/latest):

   | Your PC | File |
   |---|---|
   | Most Intel and AMD PCs | `theme-switcher-x64.exe` |
   | Snapdragon / ARM laptops | `theme-switcher-arm64.exe` |
   | 32-bit Windows 10 | `theme-switcher-x86.exe` |

   Not sure? Open Settings > System > About and check "System type".
2. Move it somewhere permanent, such as `C:\Tools\`. The startup entry points
   at wherever the exe is when you run it.
3. Run it once. If Windows asks for location access, allow it.

That's it. It's running now and will start again every time you sign in. On a
shared PC, each user who runs it gets their own copy, following their own
theme.

The exe isn't code-signed, so SmartScreen may warn you the first time. Click
"More info", then "Run anyway".

## No location on your PC?

Desktops without Wi-Fi, or PCs where location is turned off or blocked by
policy, can set coordinates by hand:

```
theme-switcher.exe --location 41.01,28.98
```

Latitude first, then longitude, in decimal degrees (south and west are
negative). Find yours by right-clicking a spot in Google Maps. To go back to
Windows' location:

```
theme-switcher.exe --location auto
```

If it has no location at all, it falls back to light from 8 AM to 8 PM.

## Uninstall

```
theme-switcher.exe --uninstall
```

This removes the startup entry and any saved location. The copy that's already
running stops when you sign out, or you can end it in Task Manager. Then
delete the exe.

## How it works

Once a minute it:

1. Works out where the sun is in your sky right now.
2. Sets `AppsUseLightTheme` and `SystemUsesLightTheme` in the registry if the
   theme should change.
3. Tells Windows the theme changed, so Explorer and open apps update right
   away.

Your location is refreshed once a day. The switch lands within a minute of
the official sunrise and sunset. The test suite checks this against almanac
times for cities in every kind of timezone, from Kathmandu (UTC+5:45) to
Kiritimati (UTC+14).

**What changes:** the app and system color mode. That covers title bars,
menus, Explorer, Settings and any app that follows the Windows theme, such as
VS Code with `window.autoDetectColorScheme`.

**What doesn't:** wallpaper, the lock screen, and apps with their own theme
setting.

## Build from source

On Windows:

```powershell
cargo build --release
```

Cross-compiling from macOS or Linux:

```bash
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64    # or your distro's mingw-w64 package
cargo build --release --target x86_64-pc-windows-gnu
```

The sun math is plain Rust with no OS dependencies, so `cargo test` runs on any
platform.

## Contributing

Issues and pull requests are welcome. Please keep it small: the goal is one
job, done well.

If it's useful to you, a star helps other people find it.

## License

[MIT](LICENSE)
