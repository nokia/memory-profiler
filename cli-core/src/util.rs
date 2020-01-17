use ctrlc;
use std::cmp::max;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct Sigint {
    flag: Arc<AtomicBool>,
}

impl Sigint {
    pub fn was_sent(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

pub fn on_ctrlc() -> Sigint {
    let aborted = Arc::new(AtomicBool::new(false));
    {
        let aborted = aborted.clone();
        ctrlc::set_handler(move || {
            aborted.store(true, Ordering::Relaxed);
        })
        .expect("error setting Ctrl-C handler");
    }

    Sigint { flag: aborted }
}

pub struct ReadableDuration(pub u64);

impl fmt::Display for ReadableDuration {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let mut secs = self.0;
        macro_rules! get {
            ($mul:expr) => {{
                let mul = $mul;
                let out = secs / mul;
                secs -= out * mul;
                out
            }};
        }

        let days = get!(60 * 60 * 24);
        let hours = get!(60 * 60);
        let minutes = get!(60);

        let show_days = days > 0;
        let show_hours = show_days || hours > 0;
        let show_minutes = show_hours || minutes > 0;

        if show_days {
            write!(formatter, "{} days ", days)?;
        }

        if show_hours {
            write!(formatter, "{:02}h", hours)?;
        }

        if show_minutes {
            write!(formatter, "{:02}m", minutes)?;
        }

        write!(formatter, "{:02}s", secs)?;
        Ok(())
    }
}

pub struct ReadableSize(pub u64);

impl fmt::Display for ReadableSize {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let bytes = self.0;

        const TB: u64 = 1000 * 1000 * 1000 * 1000;
        const GB: u64 = 1000 * 1000 * 1000;
        const MB: u64 = 1000 * 1000;
        const KB: u64 = 1000;

        fn format(
            formatter: &mut fmt::Formatter,
            bytes: u64,
            multiplier: u64,
            unit: &str,
        ) -> fmt::Result {
            let whole = bytes / multiplier;
            let fract = (bytes - whole * multiplier) / (multiplier / 1000);
            write!(formatter, "{:3}.{:03} {}", whole, fract, unit)
        }

        if bytes >= TB {
            format(formatter, bytes, TB, "TB")
        } else if bytes >= GB {
            format(formatter, bytes, GB, "GB")
        } else if bytes >= MB {
            format(formatter, bytes, MB, "MB")
        } else if bytes >= KB {
            format(formatter, bytes, KB, "KB")
        } else {
            write!(formatter, "{:7}", bytes)
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct ReadableAddress(pub u64);

impl fmt::Display for ReadableAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{:016X}", self.0)
    }
}

pub fn table_to_string(table: &[Vec<String>]) -> String {
    let mut output = String::new();
    let mut widths = Vec::new();
    let mut max_column_count = 0;
    for row in table {
        let column_count = max(widths.len(), row.len());
        max_column_count = max(column_count, max_column_count);

        widths.resize(column_count, 0);
        for (cell, width) in row.iter().zip(widths.iter_mut()) {
            *width = max(cell.chars().count(), *width);
        }
    }

    for row in table {
        for (index, (cell, &width)) in row.iter().zip(widths.iter()).enumerate() {
            if index != 0 {
                output.push_str(" ");
            }

            let mut len = cell.chars().count();
            output.push_str(&cell);
            if index == max_column_count - 1 {
                continue;
            }

            while len < width {
                output.push_str(" ");
                len += 1;
            }
        }

        output.push_str("\n");
    }

    output
}
