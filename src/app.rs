use std::os::windows::ffi::OsStrExt;
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator};
use windows::Foundation::TimeSpan;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, LPARAM, WPARAM};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegDeleteTreeW,
    RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use windows::core::{HSTRING, w};

use crate::solar;

const CHECK_INTERVAL: Duration = Duration::from_secs(60);
const LOCATION_REFRESH: Duration = Duration::from_secs(60 * 60 * 24);
// While we have no location at all (e.g. no network yet right after login).
const LOCATION_RETRY: Duration = Duration::from_secs(60 * 5);
const LOCATION_TIMEOUT: Duration = Duration::from_secs(30);

// Used only if we have never had a location (permission denied, no signal).
const FALLBACK_LIGHT_HOUR: u16 = 8;
const FALLBACK_DARK_HOUR: u16 = 20;

const APP_NAME: &str = "ThemeSwitcher";
const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const PERSONALIZE_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const APP_KEY_PATH: &str = r"Software\ThemeSwitcher";
const LOCATION_VALUE: &str = "LastLocation";
const MANUAL_LOCATION_VALUE: &str = "ManualLocation";

#[derive(Clone, Copy)]
struct Location {
    lat: f64,
    lon: f64,
}

fn set_dword(subkey: &str, value_name: &str, value: u32) -> windows::core::Result<()> {
    let data = value.to_le_bytes();
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            &HSTRING::from(value_name),
            REG_DWORD.0,
            Some(data.as_ptr() as *const _),
            data.len() as u32,
        )
    }
    .ok()
}

fn set_string(subkey: &str, value_name: &str, value: &str) -> windows::core::Result<()> {
    set_wide_string(subkey, value_name, value.encode_utf16())
}

// Takes raw UTF-16, so a path that isn't valid Unicode (Windows allows that)
// still round-trips exactly.
fn set_wide_string(
    subkey: &str,
    value_name: &str,
    value: impl Iterator<Item = u16>,
) -> windows::core::Result<()> {
    let wide: Vec<u16> = value.chain(std::iter::once(0)).collect();
    let byte_len = wide.len() * 2;
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            &HSTRING::from(value_name),
            REG_SZ.0,
            Some(wide.as_ptr() as *const _),
            byte_len as u32,
        )
    }
    .ok()
}

fn get_string(subkey: &str, value_name: &str) -> Option<String> {
    let mut buf = [0u16; 128];
    let mut byte_len = (buf.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            &HSTRING::from(value_name),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut byte_len),
        )
    }
    .ok()
    .ok()?;
    // byte_len includes the terminating null.
    let chars = (byte_len as usize / 2).saturating_sub(1);
    String::from_utf16(&buf[..chars]).ok()
}

fn delete_value(subkey: &str, value_name: &str) {
    unsafe {
        let _ = RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            &HSTRING::from(value_name),
        );
    }
}

fn register_autostart() {
    if let Ok(exe_path) = std::env::current_exe() {
        let quote = u16::from(b'"');
        let quoted = std::iter::once(quote)
            .chain(exe_path.as_os_str().encode_wide())
            .chain(std::iter::once(quote));
        let _ = set_wide_string(RUN_KEY_PATH, APP_NAME, quoted);
    }
}

fn uninstall() {
    delete_value(RUN_KEY_PATH, APP_NAME);
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(APP_KEY_PATH));
    }
}

// Returns false if another copy is already running. The mutex handle is
// intentionally leaked so it lives as long as the process.
fn acquire_single_instance() -> bool {
    match unsafe { CreateMutexW(None, false, w!("Local\\ThemeSwitcher.SingleInstance")) } {
        Ok(_) => unsafe { GetLastError() != ERROR_ALREADY_EXISTS },
        Err(_) => true,
    }
}

fn set_theme(light: bool) -> windows::core::Result<()> {
    let value: u32 = if light { 1 } else { 0 };
    set_dword(PERSONALIZE_KEY_PATH, "AppsUseLightTheme", value)?;
    set_dword(PERSONALIZE_KEY_PATH, "SystemUsesLightTheme", value)?;
    broadcast_theme_change();
    Ok(())
}

// Tells Explorer and other apps to refresh immediately instead of waiting
// on their own poll cycle.
fn broadcast_theme_change() {
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            5000,
            None,
        );
    }
}

// Multithreaded apartment: we have no window or message loop, so blocking on
// async WinRT calls with `.join()` is only safe outside a single-threaded one.
fn init_winrt() {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
}

fn to_timespan(d: Duration) -> TimeSpan {
    TimeSpan {
        Duration: (d.as_nanos() / 100) as i64,
    }
}

// First call ever triggers Windows' native location consent dialog, the same
// one Night Light uses. After that, Windows remembers the choice.
fn fetch_location() -> Option<Location> {
    let access = Geolocator::RequestAccessAsync().ok()?.join().ok()?;
    if access != GeolocationAccessStatus::Allowed {
        return None;
    }

    let locator = Geolocator::new().ok()?;
    let position = locator
        .GetGeopositionAsyncWithAgeAndTimeout(
            to_timespan(LOCATION_REFRESH),
            to_timespan(LOCATION_TIMEOUT),
        )
        .ok()?
        .join()
        .ok()?;
    let basic = position.Coordinate().ok()?.Point().ok()?.Position().ok()?;

    Some(Location {
        lat: basic.Latitude,
        lon: basic.Longitude,
    })
}

// The last known location is kept so that a boot without network still
// follows the sun instead of dropping to the fixed fallback schedule.
fn parse_location(s: &str) -> Option<Location> {
    let (lat, lon) = s.split_once(',')?;
    let lat: f64 = lat.trim().parse().ok()?;
    let lon: f64 = lon.trim().parse().ok()?;
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon))
        .then_some(Location { lat, lon })
}

fn load_location(value_name: &str) -> Option<Location> {
    parse_location(&get_string(APP_KEY_PATH, value_name)?)
}

fn save_location(value_name: &str, loc: Location) {
    let _ = set_string(
        APP_KEY_PATH,
        value_name,
        &format!("{},{}", loc.lat, loc.lon),
    );
}

// `--location 41.01,28.98` pins the coordinates, for PCs with location
// turned off or blocked by policy. `--location auto` goes back to Windows'
// location. Read again every minute, so a copy that's already running picks
// it up.
fn apply_location_arg(args: &[String]) {
    let Some(i) = args.iter().position(|a| a == "--location") else {
        return;
    };
    match args.get(i + 1).map(String::as_str) {
        Some("auto") => delete_value(APP_KEY_PATH, MANUAL_LOCATION_VALUE),
        Some(value) => {
            if let Some(loc) = parse_location(value) {
                save_location(MANUAL_LOCATION_VALUE, loc);
            }
        }
        None => {}
    }
}

fn should_be_light(loc: Option<Location>, current: Option<bool>) -> bool {
    if let Some(loc) = loc {
        let st = unsafe { GetSystemTime() };
        let now = solar::julian_date(
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond,
        );
        return solar::settled_daytime(loc.lat, loc.lon, now, current);
    }

    let st = unsafe { GetLocalTime() };
    (FALLBACK_LIGHT_HOUR..FALLBACK_DARK_HOUR).contains(&st.wHour)
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--uninstall") {
        uninstall();
        return;
    }
    apply_location_arg(&args);

    if !acquire_single_instance() {
        return;
    }

    register_autostart();
    init_winrt();

    let mut location = load_location(LOCATION_VALUE);
    let mut last_fetch: Option<Instant> = None;
    let mut last_state: Option<bool> = None;

    loop {
        let manual = load_location(MANUAL_LOCATION_VALUE);

        // Only write on a change, so a manual switch is left alone until the
        // next sunrise or sunset.
        let want_light = should_be_light(manual.or(location), last_state);
        if last_state != Some(want_light) && set_theme(want_light).is_ok() {
            last_state = Some(want_light);
        }

        // Fetched after applying the theme: this can block on the consent
        // prompt or a slow fix, and login shouldn't wait on it.
        let fetch_due = manual.is_none()
            && match last_fetch {
                None => true,
                Some(t) if location.is_none() => t.elapsed() >= LOCATION_RETRY,
                Some(t) => t.elapsed() >= LOCATION_REFRESH,
            };
        if fetch_due {
            if let Some(loc) = fetch_location() {
                save_location(LOCATION_VALUE, loc);
                location = Some(loc);
            }
            last_fetch = Some(Instant::now());
        }

        sleep(CHECK_INTERVAL);
    }
}
