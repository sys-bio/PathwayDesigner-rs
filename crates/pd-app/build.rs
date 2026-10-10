//! Records the build date for the About box (S §13.1).

use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    println!("cargo:rustc-env=PD_BUILD_DATE={y:04}-{m:02}-{d:02}");
    // The date is a property of the build, not of any file: no rerun-if lines,
    // so it updates whenever the crate is rebuilt.
}

/// Days since 1970-01-01 to a (year, month, day) date, by Howard Hinnant's
/// algorithm for the proleptic Gregorian calendar.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
