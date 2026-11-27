//! Timer lines for heavy operations. The CLI and the GUI format elapsed time with this one
//! function, so both print the same `timing: <label> <seconds>s` line. The CLI writes it to
//! stderr; the GUI writes it to its log file.

use std::time::Duration;

/// The timer line for `label` that took `elapsed`.
pub fn line(label: &str, elapsed: Duration) -> String {
    format!("timing: {label} {:.3}s", elapsed.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_has_label_and_three_decimals() {
        assert_eq!(
            line("scan", Duration::from_millis(1500)),
            "timing: scan 1.500s"
        );
        assert_eq!(
            line("total", Duration::from_micros(2)),
            "timing: total 0.000s"
        );
    }
}
