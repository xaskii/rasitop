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

    #[test]
    fn test_formatters_with_missing_optional_fields() {
        let sample = PowermetricsSample {
            timestamp: None,
            cpu_power_mw: 1000.0,
            gpu_power_mw: 500.0,
            combined_power_mw: 1500.0,
            battery_percent: None,
            cpu_busy_ratio: None,
            e_busy_ratio: None,
            p_busy_ratio: None,
            e_freq_hz: None,
            p_freq_hz: None,
        };

        // All formatters should handle missing fields gracefully
        let human = HumanFormatter::new();
        human.print_sample(&sample);

        let csv = CsvFormatter::new();
        csv.print_sample(&sample);

        let json = JsonFormatter::new();
        json.print_sample(&sample);
    }

    #[test]
    fn test_formatters_with_zero_values() {
        let sample = PowermetricsSample {
            timestamp: Some("2025-01-01T00:00:00Z".to_string()),
            cpu_power_mw: 0.0,
            gpu_power_mw: 0.0,
            combined_power_mw: 0.0,
            battery_percent: Some(0),
            cpu_busy_ratio: Some(0.0),
            e_busy_ratio: Some(0.0),
            p_busy_ratio: Some(0.0),
            e_freq_hz: Some(0.0),
            p_freq_hz: Some(0.0),
        };

        let human = HumanFormatter::new();
        human.print_sample(&sample);

        let csv = CsvFormatter::new();
        csv.print_sample(&sample);

        let json = JsonFormatter::new();
        json.print_sample(&sample);
    }

    #[test]
    fn test_formatters_with_high_values() {
        let sample = PowermetricsSample {
            timestamp: Some("2025-12-31T23:59:59Z".to_string()),
            cpu_power_mw: 50000.0, // 50W
            gpu_power_mw: 150000.0, // 150W
            combined_power_mw: 200000.0, // 200W
            battery_percent: Some(100),
            cpu_busy_ratio: Some(1.0),
            e_busy_ratio: Some(1.0),
            p_busy_ratio: Some(1.0),
            e_freq_hz: Some(4.0e9), // 4 GHz
            p_freq_hz: Some(5.0e9), // 5 GHz
        };

        let human = HumanFormatter::new();
        human.print_sample(&sample);

        let csv = CsvFormatter::new();
        csv.print_sample(&sample);

        let json = JsonFormatter::new();
        json.print_sample(&sample);
    }

    #[test]
    fn test_formatters_with_partial_cluster_data() {
        // E-cluster data only
        let sample1 = PowermetricsSample {
            timestamp: Some("2025-01-15T12:00:00Z".to_string()),
            cpu_power_mw: 1200.0,
            gpu_power_mw: 0.0,
            combined_power_mw: 1200.0,
            battery_percent: Some(50),
            cpu_busy_ratio: None,
            e_busy_ratio: Some(0.5),
            p_busy_ratio: None,
            e_freq_hz: Some(1.5e9),
            p_freq_hz: None,
        };

        let human = HumanFormatter::new();
        human.print_sample(&sample1);

        let csv = CsvFormatter::new();
        csv.print_sample(&sample1);

        let json = JsonFormatter::new();
        json.print_sample(&sample1);

        // P-cluster data only
        let sample2 = PowermetricsSample {
            timestamp: Some("2025-01-15T12:00:01Z".to_string()),
            cpu_power_mw: 3500.0,
            gpu_power_mw: 200.0,
            combined_power_mw: 3700.0,
            battery_percent: Some(49),
            cpu_busy_ratio: None,
            e_busy_ratio: None,
            p_busy_ratio: Some(0.8),
            e_freq_hz: None,
            p_freq_hz: Some(3.2e9),
        };

        human.print_sample(&sample2);
        csv.print_sample(&sample2);
        json.print_sample(&sample2);
    }
}
