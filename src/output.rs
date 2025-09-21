use crate::pm::PowermetricsSample;

pub trait OutputFormatter {
    fn print_header(&self);
    fn print_sample(&self, sample: &PowermetricsSample);
}

pub struct HumanFormatter;

impl HumanFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for HumanFormatter {
    fn print_header(&self) {
        // No header for human format
    }

    fn print_sample(&self, sample: &PowermetricsSample) {
        let battery_str = sample
            .battery_percent
            .map(|p| format!(" | Battery: {}%", p))
            .unwrap_or_default();

        println!(
            "[{}] CPU: {:.2}W  GPU: {:.2}W  Combined: {:.2}W | E-busy: {:.1}%  P-busy: {:.1}%  E-freq: {:.2}GHz  P-freq: {:.2}GHz{}",
            sample.timestamp.as_deref().unwrap_or("?"),
            sample.cpu_power_mw / 1000.0,
            sample.gpu_power_mw / 1000.0,
            sample.combined_power_mw / 1000.0,
            sample.e_busy_ratio.unwrap_or(0.0) * 100.0,
            sample.p_busy_ratio.unwrap_or(0.0) * 100.0,
            sample.e_freq_hz.unwrap_or(0.0) / 1e9,
            sample.p_freq_hz.unwrap_or(0.0) / 1e9,
            battery_str,
        );
    }
}

pub struct CsvFormatter {
    header_printed: std::cell::Cell<bool>,
}

impl CsvFormatter {
    pub fn new() -> Self {
        Self {
            header_printed: std::cell::Cell::new(false),
        }
    }
}

impl OutputFormatter for CsvFormatter {
    fn print_header(&self) {
        println!("timestamp,cpu_power_w,gpu_power_w,combined_power_w,e_busy_ratio,p_busy_ratio,e_freq_ghz,p_freq_ghz,battery_percent");
    }

    fn print_sample(&self, sample: &PowermetricsSample) {
        if !self.header_printed.get() {
            self.print_header();
            self.header_printed.set(true);
        }
        println!(
            "{},{:.2},{:.2},{:.2},{:.4},{:.4},{:.2},{:.2},{}",
            sample.timestamp.as_deref().unwrap_or(""),
            sample.cpu_power_mw / 1000.0,
            sample.gpu_power_mw / 1000.0,
            sample.combined_power_mw / 1000.0,
            sample.e_busy_ratio.unwrap_or(0.0),
            sample.p_busy_ratio.unwrap_or(0.0),
            sample.e_freq_hz.unwrap_or(0.0) / 1e9,
            sample.p_freq_hz.unwrap_or(0.0) / 1e9,
            sample
                .battery_percent
                .map(|p| p.to_string())
                .unwrap_or_default(),
        );
    }
}

pub struct JsonFormatter;

impl JsonFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for JsonFormatter {
    fn print_header(&self) {
        // No header for JSON
    }

    fn print_sample(&self, sample: &PowermetricsSample) {
        // Manually construct JSON to avoid adding serde_json dependency
        let timestamp = sample
            .timestamp
            .as_ref()
            .map(|s| format!("\"{}\"", s))
            .unwrap_or_else(|| "null".to_string());

        let e_busy = sample
            .e_busy_ratio
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string());

        let p_busy = sample
            .p_busy_ratio
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string());

        let e_freq = sample
            .e_freq_hz
            .map(|v| format!("{:.2}", v / 1e9))
            .unwrap_or_else(|| "null".to_string());

        let p_freq = sample
            .p_freq_hz
            .map(|v| format!("{:.2}", v / 1e9))
            .unwrap_or_else(|| "null".to_string());

        let battery = sample
            .battery_percent
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string());

        println!(
            r#"{{"timestamp":{},"cpu_power_w":{:.2},"gpu_power_w":{:.2},"combined_power_w":{:.2},"e_busy_ratio":{},"p_busy_ratio":{},"e_freq_ghz":{},"p_freq_ghz":{},"battery_percent":{}}}"#,
            timestamp,
            sample.cpu_power_mw / 1000.0,
            sample.gpu_power_mw / 1000.0,
            sample.combined_power_mw / 1000.0,
            e_busy,
            p_busy,
            e_freq,
            p_freq,
            battery,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_for_test() -> PowermetricsSample {
        PowermetricsSample {
            timestamp: Some("2025-04-26T21:49:40Z".to_string()),
            cpu_power_mw: 1941.82,
            gpu_power_mw: 0.0,
            combined_power_mw: 1941.82,
            battery_percent: Some(69),
            cpu_busy_ratio: Some(0.5),
            e_busy_ratio: Some(0.388264),
            p_busy_ratio: Some(0.692109),
            e_freq_hz: Some(972_013_000.0),
            p_freq_hz: Some(3_089_180_000.0),
        }
    }

    #[test]
    fn test_human_formatter() {
        let formatter = HumanFormatter::new();
        let sample = sample_for_test();
        // Just verify it doesn't panic
        formatter.print_sample(&sample);
    }

    #[test]
    fn test_csv_formatter() {
        let formatter = CsvFormatter::new();
        let sample = sample_for_test();
        formatter.print_header();
        formatter.print_sample(&sample);
    }

    #[test]
    fn test_json_formatter() {
        let formatter = JsonFormatter::new();
        let sample = sample_for_test();
        formatter.print_sample(&sample);
    }
}
