use crate::metrics::Sample;

pub trait OutputFormatter {
    fn format_header(&self) -> Option<String>;
    fn format_sample(&self, sample: &Sample) -> String;

    fn print_header(&self) {
        if let Some(header) = self.format_header() {
            println!("{header}");
        }
    }

    fn print_sample(&self, sample: &Sample) {
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

    fn format_sample(&self, sample: &Sample) -> String {
        let temp_str = match (sample.cpu_temp_c, sample.gpu_temp_c) {
            (Some(cpu), Some(gpu)) => format!(" | Temp: CPU {:.1}C GPU {:.1}C", cpu, gpu),
            (Some(cpu), None) => format!(" | Temp: CPU {:.1}C", cpu),
            (None, Some(gpu)) => format!(" | Temp: GPU {:.1}C", gpu),
            _ => String::new(),
        };

        let mem_str = match (sample.ram_usage_bytes, sample.ram_total_bytes) {
            (Some(usage), Some(total)) if total > 0 => {
                let usage_gb = usage as f64 / (1024.0 * 1024.0 * 1024.0);
                let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
                let swap_str = match (sample.swap_usage_bytes, sample.swap_total_bytes) {
                    (Some(swap_usage), Some(swap_total)) if swap_total > 0 => {
                        let swap_usage_gb = swap_usage as f64 / (1024.0 * 1024.0 * 1024.0);
                        let swap_total_gb = swap_total as f64 / (1024.0 * 1024.0 * 1024.0);
                        format!(" Swap {:.2}/{:.2}GB", swap_usage_gb, swap_total_gb)
                    }
                    _ => String::new(),
                };
                format!(" | RAM {:.2}/{:.2}GB{}", usage_gb, total_gb, swap_str)
            }
            _ => String::new(),
        };

        let sys_power_str = sample
            .sys_power_mw
            .map(|w| format!(" | Total: {:.2}W", w / 1000.0))
            .unwrap_or_default();

        let battery_str = sample
            .battery_percent
            .map(|p| format!(" | Battery: {}%", p))
            .unwrap_or_default();

        format!(
            "[{}] CPU: {:.2}W  GPU: {:.2}W  ANE: {:.2}W  Combined: {:.2}W{} | E-busy: {:.1}%  P-busy: {:.1}%  E-freq: {:.2}GHz  P-freq: {:.2}GHz{}{}{}",
            sample.timestamp.as_deref().unwrap_or("?"),
            sample.cpu_power_mw / 1000.0,
            sample.gpu_power_mw / 1000.0,
            sample.ane_power_mw / 1000.0,
            sample.combined_power_mw / 1000.0,
            sys_power_str,
            sample.e_busy_ratio.unwrap_or(0.0) * 100.0,
            sample.p_busy_ratio.unwrap_or(0.0) * 100.0,
            sample.e_freq_hz.unwrap_or(0.0) / 1e9,
            sample.p_freq_hz.unwrap_or(0.0) / 1e9,
            temp_str,
            mem_str,
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
            "timestamp,cpu_power_w,gpu_power_w,ane_power_w,combined_power_w,sys_power_w,cpu_energy,gpu_energy,ane_energy,e_busy_ratio,p_busy_ratio,e_freq_ghz,p_freq_ghz,cpu_temp_c,gpu_temp_c,ram_total_bytes,ram_usage_bytes,swap_total_bytes,swap_usage_bytes,battery_percent"
                .to_string(),
        )
    }

    fn format_sample(&self, sample: &Sample) -> String {
        format!(
            "{},{:.2},{:.2},{:.2},{:.2},{:.2},{},{},{},{:.4},{:.4},{:.2},{:.2},{},{},{},{},{},{},{}",
            sample.timestamp.as_deref().unwrap_or(""),
            sample.cpu_power_mw / 1000.0,
            sample.gpu_power_mw / 1000.0,
            sample.ane_power_mw / 1000.0,
            sample.combined_power_mw / 1000.0,
            sample.sys_power_mw.unwrap_or(0.0) / 1000.0,
            sample.cpu_energy.map(|e| e.to_string()).unwrap_or_default(),
            sample.gpu_energy.map(|e| e.to_string()).unwrap_or_default(),
            sample.ane_energy.map(|e| e.to_string()).unwrap_or_default(),
            sample.e_busy_ratio.unwrap_or(0.0),
            sample.p_busy_ratio.unwrap_or(0.0),
            sample.e_freq_hz.unwrap_or(0.0) / 1e9,
            sample.p_freq_hz.unwrap_or(0.0) / 1e9,
            sample.cpu_temp_c.map(|t| format!("{:.2}", t)).unwrap_or_default(),
            sample.gpu_temp_c.map(|t| format!("{:.2}", t)).unwrap_or_default(),
            sample.ram_total_bytes.map(|v| v.to_string()).unwrap_or_default(),
            sample.ram_usage_bytes.map(|v| v.to_string()).unwrap_or_default(),
            sample.swap_total_bytes.map(|v| v.to_string()).unwrap_or_default(),
            sample.swap_usage_bytes.map(|v| v.to_string()).unwrap_or_default(),
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

    fn print_sample(&self, sample: &Sample) {
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

    fn format_sample(&self, sample: &Sample) -> String {
        let round_two = |value: f64| (value * 100.0).round() / 100.0;
        let round_three = |value: f32| (value * 1000.0).round() / 1000.0;

        let payload = serde_json::json!({
            "timestamp": sample.timestamp,
            "cpu_power_w": round_two(sample.cpu_power_mw / 1000.0),
            "gpu_power_w": round_two(sample.gpu_power_mw / 1000.0),
            "ane_power_w": round_two(sample.ane_power_mw / 1000.0),
            "combined_power_w": round_two(sample.combined_power_mw / 1000.0),
            "sys_power_w": sample.sys_power_mw.map(|v| round_two(v / 1000.0)),
            "cpu_energy": sample.cpu_energy,
            "gpu_energy": sample.gpu_energy,
            "ane_energy": sample.ane_energy,
            "e_busy_ratio": sample.e_busy_ratio,
            "p_busy_ratio": sample.p_busy_ratio,
            "e_freq_ghz": sample.e_freq_hz.map(|v| round_two(v / 1e9)),
            "p_freq_ghz": sample.p_freq_hz.map(|v| round_two(v / 1e9)),
            "cpu_temp_c": sample.cpu_temp_c.map(round_three),
            "gpu_temp_c": sample.gpu_temp_c.map(round_three),
            "ram_total_bytes": sample.ram_total_bytes,
            "ram_usage_bytes": sample.ram_usage_bytes,
            "swap_total_bytes": sample.swap_total_bytes,
            "swap_usage_bytes": sample.swap_usage_bytes,
            "battery_percent": sample.battery_percent,
        });

        serde_json::to_string(&payload).expect("json serialize")
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

    fn format_outputs(sample: &Sample) -> FormatterOutputs {
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

    fn sample_for_test() -> Sample {
        Sample {
            timestamp: Some("2025-04-26T21:49:40Z".to_string()),
            cpu_power_mw: 1941.82,
            gpu_power_mw: 0.0,
            ane_power_mw: 0.0,
            combined_power_mw: 1941.82,
            sys_power_mw: Some(4123.5),
            cpu_energy: Some(500),
            gpu_energy: Some(0),
            ane_energy: Some(0),
            battery_percent: Some(69),
            cpu_busy_ratio: Some(0.5),
            e_busy_ratio: Some(0.388264),
            p_busy_ratio: Some(0.692109),
            e_freq_hz: Some(972_013_000.0),
            p_freq_hz: Some(3_089_180_000.0),
            cpu_temp_c: Some(48.25),
            gpu_temp_c: Some(42.75),
            ram_total_bytes: Some(25_769_803_776),
            ram_usage_bytes: Some(20_985_479_168),
            swap_total_bytes: Some(4_294_967_296),
            swap_usage_bytes: Some(2_602_434_560),
        }
    }

    #[test]
    fn snapshot_formatters_default_sample() {
        assert_debug_snapshot!(format_outputs(&sample_for_test()));
    }

    #[test]
    fn snapshot_formatters_with_missing_optional_fields() {
        let sample = Sample {
            timestamp: None,
            cpu_power_mw: 1000.0,
            gpu_power_mw: 500.0,
            ane_power_mw: 0.0,
            combined_power_mw: 1500.0,
            sys_power_mw: None,
            cpu_energy: None,
            gpu_energy: None,
            ane_energy: None,
            battery_percent: None,
            cpu_busy_ratio: None,
            e_busy_ratio: None,
            p_busy_ratio: None,
            e_freq_hz: None,
            p_freq_hz: None,
            cpu_temp_c: None,
            gpu_temp_c: None,
            ram_total_bytes: None,
            ram_usage_bytes: None,
            swap_total_bytes: None,
            swap_usage_bytes: None,
        };

        assert_debug_snapshot!(format_outputs(&sample));
    }

    #[test]
    fn snapshot_formatters_with_zero_values() {
        let sample = Sample {
            timestamp: Some("2025-01-01T00:00:00Z".to_string()),
            cpu_power_mw: 0.0,
            gpu_power_mw: 0.0,
            ane_power_mw: 0.0,
            combined_power_mw: 0.0,
            sys_power_mw: Some(0.0),
            cpu_energy: Some(0),
            gpu_energy: Some(0),
            ane_energy: Some(0),
            battery_percent: Some(0),
            cpu_busy_ratio: Some(0.0),
            e_busy_ratio: Some(0.0),
            p_busy_ratio: Some(0.0),
            e_freq_hz: Some(0.0),
            p_freq_hz: Some(0.0),
            cpu_temp_c: Some(0.0),
            gpu_temp_c: Some(0.0),
            ram_total_bytes: Some(0),
            ram_usage_bytes: Some(0),
            swap_total_bytes: Some(0),
            swap_usage_bytes: Some(0),
        };

        assert_debug_snapshot!(format_outputs(&sample));
    }

    #[test]
    fn snapshot_formatters_with_high_values() {
        let sample = Sample {
            timestamp: Some("2025-12-31T23:59:59Z".to_string()),
            cpu_power_mw: 50000.0,       // 50W
            gpu_power_mw: 150000.0,      // 150W
            ane_power_mw: 10000.0,       // 10W
            combined_power_mw: 200000.0, // 200W
            sys_power_mw: Some(250000.0),
            cpu_energy: Some(25000),
            gpu_energy: Some(75000),
            ane_energy: Some(5000),
            battery_percent: Some(100),
            cpu_busy_ratio: Some(1.0),
            e_busy_ratio: Some(1.0),
            p_busy_ratio: Some(1.0),
            e_freq_hz: Some(4.0e9), // 4 GHz
            p_freq_hz: Some(5.0e9), // 5 GHz
            cpu_temp_c: Some(95.0),
            gpu_temp_c: Some(88.5),
            ram_total_bytes: Some(68_719_476_736),
            ram_usage_bytes: Some(61_000_000_000),
            swap_total_bytes: Some(8_589_934_592),
            swap_usage_bytes: Some(4_200_000_000),
        };

        assert_debug_snapshot!(format_outputs(&sample));
    }

    #[test]
    fn snapshot_formatters_with_partial_cluster_data() {
        let sample1 = Sample {
            timestamp: Some("2025-01-15T12:00:00Z".to_string()),
            cpu_power_mw: 1200.0,
            gpu_power_mw: 0.0,
            ane_power_mw: 0.0,
            combined_power_mw: 1200.0,
            sys_power_mw: None,
            cpu_energy: Some(600),
            gpu_energy: Some(0),
            ane_energy: Some(0),
            battery_percent: Some(50),
            cpu_busy_ratio: None,
            e_busy_ratio: Some(0.5),
            p_busy_ratio: None,
            e_freq_hz: Some(1.5e9),
            p_freq_hz: None,
            cpu_temp_c: Some(47.0),
            gpu_temp_c: None,
            ram_total_bytes: Some(8_589_934_592),
            ram_usage_bytes: Some(4_200_000_000),
            swap_total_bytes: None,
            swap_usage_bytes: None,
        };

        let sample2 = Sample {
            timestamp: Some("2025-01-15T12:00:01Z".to_string()),
            cpu_power_mw: 3500.0,
            gpu_power_mw: 200.0,
            ane_power_mw: 0.0,
            combined_power_mw: 3700.0,
            sys_power_mw: Some(4200.0),
            cpu_energy: Some(1750),
            gpu_energy: Some(100),
            ane_energy: Some(0),
            battery_percent: Some(49),
            cpu_busy_ratio: None,
            e_busy_ratio: None,
            p_busy_ratio: Some(0.8),
            e_freq_hz: None,
            p_freq_hz: Some(3.2e9),
            cpu_temp_c: None,
            gpu_temp_c: Some(50.5),
            ram_total_bytes: Some(8_589_934_592),
            ram_usage_bytes: Some(4_300_000_000),
            swap_total_bytes: Some(4_294_967_296),
            swap_usage_bytes: Some(1_000_000_000),
        };

        let outputs = PartialClusterOutputs {
            e_cluster: format_outputs(&sample1),
            p_cluster: format_outputs(&sample2),
        };

        assert_debug_snapshot!(outputs);
    }
}
