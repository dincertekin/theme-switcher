// Where the sun is. Pure functions with no OS dependencies, so the tests run
// on any platform.
//
// Everything works on UTC and coordinates only. The local timezone, DST, and
// half- or quarter-hour offsets never enter the math, so they can't throw it
// off.

/// Sun elevation at sunrise/sunset: the center of the disc sits 0.833 degrees
/// below the horizon (refraction plus the disc's radius) when its top edge
/// touches it. The same convention almanacs use.
const HORIZON_DEG: f64 = -0.833;

// Julian Day Number at Greenwich noon for a Gregorian calendar date
// (Fliegel & Van Flandern algorithm).
fn julian_day_number(year: i64, month: i64, day: i64) -> i64 {
    let a = (14 - month) / 12;
    let y = year + 4800 - a;
    let m = month + 12 * a - 3;
    day + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045
}

/// Julian date for a UTC timestamp.
pub fn julian_date(year: u16, month: u16, day: u16, hour: u16, minute: u16, second: u16) -> f64 {
    let jdn = julian_day_number(year as i64, month as i64, day as i64) as f64;
    let day_fraction = (hour as f64 * 3600.0 + minute as f64 * 60.0 + second as f64) / 86400.0;
    jdn - 0.5 + day_fraction
}

/// Sun elevation in degrees above the horizon at a UTC Julian date, from
/// NOAA's solar position equations. Longitude is east-positive (New York is
/// about -74.0).
pub fn sun_elevation(lat_deg: f64, lon_east_deg: f64, jd: f64) -> f64 {
    // Julian centuries since J2000.0.
    let t = (jd - 2451545.0) / 36525.0;

    let mean_long = (280.46646 + t * (36000.76983 + t * 0.0003032)).rem_euclid(360.0);
    let mean_anomaly = (357.52911 + t * (35999.05029 - 0.0001537 * t)).to_radians();
    let eccentricity = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);

    let center = mean_anomaly.sin() * (1.914602 - t * (0.004817 + 0.000014 * t))
        + (2.0 * mean_anomaly).sin() * (0.019993 - 0.000101 * t)
        + (3.0 * mean_anomaly).sin() * 0.000289;
    let omega = (125.04 - 1934.136 * t).to_radians();
    let apparent_long = (mean_long + center - 0.00569 - 0.00478 * omega.sin()).to_radians();

    let mean_obliquity =
        23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60.0) / 60.0;
    let obliquity = (mean_obliquity + 0.00256 * omega.cos()).to_radians();

    let declination = (obliquity.sin() * apparent_long.sin()).asin();

    let y = (obliquity / 2.0).tan().powi(2);
    let l0 = mean_long.to_radians();
    let e = eccentricity;
    let m = mean_anomaly;
    let equation_of_time_min = 4.0
        * (y * (2.0 * l0).sin() - 2.0 * e * m.sin() + 4.0 * e * y * m.sin() * (2.0 * l0).cos()
            - 0.5 * y * y * (4.0 * l0).sin()
            - 1.25 * e * e * (2.0 * m).sin())
        .to_degrees();

    let utc_min = (jd + 0.5).rem_euclid(1.0) * 1440.0;
    let true_solar_min = (utc_min + equation_of_time_min + 4.0 * lon_east_deg).rem_euclid(1440.0);
    let hour_angle = (true_solar_min / 4.0 - 180.0).to_radians();

    let phi = lat_deg.to_radians();
    let cos_zenith =
        phi.sin() * declination.sin() + phi.cos() * declination.cos() * hour_angle.cos();
    90.0 - cos_zenith.clamp(-1.0, 1.0).acos().to_degrees()
}

/// Whether the sun is up at `jd` (UTC Julian date) at the given location.
/// Polar day and night need no special case: the sun just stays above or
/// below the horizon.
pub fn is_daytime(lat_deg: f64, lon_east_deg: f64, jd: f64) -> bool {
    sun_elevation(lat_deg, lon_east_deg, jd) > HORIZON_DEG
}

/// A new theme has to last at least this long before we switch to it.
const MIN_RUN_MIN: u32 = 60;

/// Like `is_daytime`, but ignores days or nights shorter than an hour. Inside
/// the polar circles, around the start and end of polar night, the sun can
/// peek over the horizon for only a few minutes. That shouldn't flash the
/// theme. Ordinary sunrises and sunsets are never delayed, since what follows
/// them lasts for hours.
///
/// `current` is the theme in effect, or `None` if nothing is applied yet.
pub fn settled_daytime(lat_deg: f64, lon_east_deg: f64, jd: f64, current: Option<bool>) -> bool {
    let now = is_daytime(lat_deg, lon_east_deg, jd);
    match current {
        Some(current) if current != now => {
            let lasts = (5..=MIN_RUN_MIN)
                .step_by(5)
                .all(|m| is_daytime(lat_deg, lon_east_deg, jd + m as f64 / 1440.0) == now);
            if lasts { now } else { current }
        }
        _ => now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: f64 = 1.0 / 1440.0;

    fn jd_from_iso(s: &str) -> f64 {
        // "2026-06-21T13:17:25"
        let n = |r: std::ops::Range<usize>| s[r].parse::<u16>().unwrap();
        julian_date(n(0..4), n(5..7), n(8..10), n(11..13), n(14..16), n(17..19))
    }

    // Reference sunrise and sunset in UTC from the sunrise-sunset.org almanac,
    // chosen for awkward timezones: quarter- and half-hour offsets, both sides
    // of the date line, the equator and high latitudes. The label is the local
    // UTC offset on that date.
    #[rustfmt::skip]
    const ALMANAC: &[(&str, f64, f64, &str, &str)] = &[
        ("Kathmandu +5:45",    27.7172,   85.3240, "2026-06-20T23:23:32", "2026-06-21T13:17:25"),
        ("Kathmandu +5:45",    27.7172,   85.3240, "2026-12-21T01:04:59", "2026-12-21T11:28:18"),
        ("Mumbai +5:30",       19.0760,   72.8777, "2026-12-21T01:36:57", "2026-12-21T12:35:57"),
        ("St. John's -2:30",   47.5615,  -52.7126, "2026-06-21T07:33:15", "2026-06-21T23:32:09"),
        ("Adelaide +10:30",   -34.9285,  138.6007, "2026-01-14T19:47:33", "2026-01-15T10:02:10"),
        ("Chatham +13:45",    -43.9535, -176.5597, "2026-01-15T16:23:33", "2026-01-16T07:28:04"),
        ("Kiritimati +14",      1.8721, -157.4278, "2026-03-20T16:33:36", "2026-03-21T04:40:27"),
        ("Pago Pago -11",     -14.2756, -170.7020, "2026-07-01T17:47:51", "2026-07-02T05:05:45"),
        ("Reykjavik +0",       64.1466,  -21.9426, "2026-06-21T02:54:57", "2026-06-22T00:04:15"),
        ("Reykjavik +0",       64.1466,  -21.9426, "2026-12-21T11:21:55", "2026-12-21T15:29:48"),
        ("Istanbul +3",        41.0082,   28.9784, "2026-12-21T05:25:22", "2026-12-21T14:38:50"),
        ("Los Angeles -7",     34.0522, -118.2437, "2026-07-01T12:45:25", "2026-07-02T03:08:28"),
        ("Sydney +11",        -33.8688,  151.2093, "2026-01-15T19:00:37", "2026-01-16T09:08:55"),
        ("Quito -5",           -0.1807,  -78.4678, "2026-09-23T11:02:45", "2026-09-23T23:09:33"),
        ("Anchorage -8",       61.2181, -149.9003, "2026-03-08T16:36:50", "2026-03-09T03:43:39"),
    ];

    #[test]
    fn julian_date_epoch() {
        // J2000.0 is 2000-01-01 12:00 UTC.
        assert_eq!(julian_date(2000, 1, 1, 12, 0, 0), 2451545.0);
    }

    // Dark one minute before sunrise, light one minute after, and the reverse
    // at sunset. So every switch lands within a minute of the almanac.
    #[test]
    fn matches_almanac_to_the_minute() {
        for &(city, lat, lon, rise, set) in ALMANAC {
            let (rise, set) = (jd_from_iso(rise), jd_from_iso(set));
            assert!(
                !is_daytime(lat, lon, rise - MINUTE),
                "{city}: light before sunrise"
            );
            assert!(
                is_daytime(lat, lon, rise + MINUTE),
                "{city}: dark after sunrise"
            );
            assert!(
                is_daytime(lat, lon, set - MINUTE),
                "{city}: dark before sunset"
            );
            assert!(
                !is_daytime(lat, lon, set + MINUTE),
                "{city}: light after sunset"
            );
        }
    }

    #[test]
    fn polar_night_and_day() {
        let tromso = (69.65, 18.96);
        let december = julian_date(2026, 12, 21, 0, 0, 0);
        let june = julian_date(2026, 6, 21, 0, 0, 0);
        for minute in 0..1440 {
            let offset = minute as f64 * MINUTE;
            assert!(!is_daytime(tromso.0, tromso.1, december + offset));
            assert!(is_daytime(tromso.0, tromso.1, june + offset));
        }
    }

    // Ordinary sunrises and sunsets switch right on time.
    #[test]
    fn settling_never_delays_a_normal_switch() {
        for &(city, lat, lon, rise, set) in ALMANAC {
            let (rise, set) = (jd_from_iso(rise), jd_from_iso(set));
            assert!(
                settled_daytime(lat, lon, rise + MINUTE, Some(false)),
                "{city}: sunrise"
            );
            assert!(
                !settled_daytime(lat, lon, set + MINUTE, Some(true)),
                "{city}: sunset"
            );
        }
    }

    // Anywhere on Earth, from pole to pole, a full year checked minute by
    // minute the way the app runs: once the theme flips, it stays put for at
    // least an hour. No flicker, even at the edge of polar night.
    #[test]
    fn no_flicker_anywhere() {
        let year_start = julian_date(2026, 1, 1, 0, 0, 0);
        for lat in (-85i32..=85).step_by(5) {
            for lon in (-180i32..=180).step_by(45) {
                let (lat, lon) = (lat as f64, lon as f64);
                let mut state = settled_daytime(lat, lon, year_start, None);
                let mut last_flip: Option<u32> = None;
                for minute in 1..(365 * 1440) {
                    let jd = year_start + minute as f64 * MINUTE;
                    let next = settled_daytime(lat, lon, jd, Some(state));
                    if next != state {
                        if let Some(last) = last_flip {
                            let gap = minute - last;
                            assert!(
                                gap >= 60,
                                "lat {lat}, lon {lon}: flipped twice in {gap} min"
                            );
                        }
                        last_flip = Some(minute);
                        state = next;
                    }
                }
            }
        }
    }
}
