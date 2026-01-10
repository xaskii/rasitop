use crate::pm::PowermetricsSample;

pub trait OutputFormatter {
    fn format_header(&self) -> Option<String>;
    fn format_sample(&self, sample: &PowermetricsSample) -> String;

    fn print_header(&self) {
        if let Some(header) = self.format_header() {
            println!("{header}");
        }
    }

    fn print_sample(&self, sample: &PowermetricsSample) {
        println!("{}", self.format_sample(sample));
    }
}

pub struct HumanFormatter;

impl HumanFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for HumanFormatter {
    fn format_header(&self) -> Option<String> {
        None
    }

    fn format_sample(&self, sample: &PowermetricsSample) -> String {
        let battery_str = sample
            .battery_percent
            .map(|p| format!(" | Battery: {}%", p))
            .unwrap_or_default();

        format!(
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
        )
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
    fn format_header(&self) -> Option<String> {
        Some(
            "timestamp,cpu_power_w,gpu_power_w,combined_power_w,e_busy_ratio,p_busy_ratio,e_freq_ghz,p_freq_ghz,battery_percent"
                .to_string(),
        )
    }

    fn format_sample(&self, sample: &PowermetricsSample) -> String {
        format!(
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
        )
    }

    fn print_header(&self) {
        if let Some(header) = self.format_header() {
            println!("{header}");
        }
        self.header_printed.set(true);
    }

    fn print_sample(&self, sample: &PowermetricsSample) {
        if !self.header_printed.get() {
            self.print_header();
        }
        println!("{}", self.format_sample(sample));
    }
}

pub struct JsonFormatter;

impl JsonFormatter {
    pub fn new() -> Self {
        Self
    }
}

impl OutputFormatter for JsonFormatter {
    fn format_header(&self) -> Option<String> {
        None
    }

    fn format_sample(&self, sample: &PowermetricsSample) -> String {
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

        format!(
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
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[derive(Debug)]
    #[allow(dead_code)]
    struct FormatterOutputs {
        human: String,
        csv: String,
        json: String,
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    struct PartialClusterOutputs {
        e_cluster: FormatterOutputs,
        p_cluster: FormatterOutputs,
    }

    fn format_outputs(sample: &PowermetricsSample) -> FormatterOutputs {
        let human = HumanFormatter::new().format_sample(sample);
        let csv_formatter = CsvFormatter::new();
        let csv_header = csv_formatter.format_header().unwrap_or_default();
        let csv_sample = csv_formatter.format_sample(sample);
        let csv = if csv_header.is_empty() {
            csv_sample
        } else {
            format!("{csv_header}\n{csv_sample}")
        };
        let json = JsonFormatter::new().format_sample(sample);

        FormatterOutputs { human, csv, json }
    }

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
    fn snapshot_formatters_default_sample() {
        assert_debug_snapshot!(format_outputs(&sample_for_test()));
    }

    #[test]
    fn snapshot_formatters_with_missing_optional_fields() {
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

        assert_debug_snapshot!(format_outputs(&sample));
    }

    #[test]
    fn snapshot_formatters_with_zero_values() {
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

        assert_debug_snapshot!(format_outputs(&sample));
    }

    #[test]
    fn snapshot_formatters_with_high_values() {
        let sample = PowermetricsSample {
            timestamp: Some("2025-12-31T23:59:59Z".to_string()),
            cpu_power_mw: 50000.0,       // 50W
            gpu_power_mw: 150000.0,      // 150W
            combined_power_mw: 200000.0, // 200W
            battery_percent: Some(100),
            cpu_busy_ratio: Some(1.0),
            e_busy_ratio: Some(1.0),
            p_busy_ratio: Some(1.0),
            e_freq_hz: Some(4.0e9), // 4 GHz
            p_freq_hz: Some(5.0e9), // 5 GHz
        };

        assert_debug_snapshot!(format_outputs(&sample));
    }

    #[test]
    fn snapshot_formatters_with_partial_cluster_data() {
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

        let outputs = PartialClusterOutputs {
            e_cluster: format_outputs(&sample1),
            p_cluster: format_outputs(&sample2),
        };

        assert_debug_snapshot!(outputs);
    }
}
