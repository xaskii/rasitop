use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct PowermetricsSample {
    pub timestamp: Option<String>,
    pub cpu_power_mw: f64,
    pub gpu_power_mw: f64,
    pub combined_power_mw: f64,
    pub ane_power_mw: f64,
    pub cpu_energy: Option<u64>,
    pub gpu_energy: Option<u64>,
    pub ane_energy: Option<u64>,
    pub battery_percent: Option<u8>,
    #[allow(dead_code)]
    pub cpu_busy_ratio: Option<f64>,
    pub e_busy_ratio: Option<f64>,
    pub p_busy_ratio: Option<f64>,
    pub e_freq_hz: Option<f64>,
    pub p_freq_hz: Option<f64>,
}

impl PowermetricsSample {
    pub fn from_plist(doc: &PowermetricsPlist) -> Option<Self> {
        let timestamp = doc.timestamp.as_ref().map(|v| match v {
            plist::Value::String(s) => s.clone(),
            plist::Value::Date(d) => format!("{:?}", d),
            other => format!("{:?}", other),
        });
        // processor.cpu_power appears to be in the plist as a real; interpret as mW
        let cpu_power_mw = doc
            .processor
            .as_ref()
            .and_then(|p| p.cpu_power)
            .unwrap_or(0.0);
        let gpu_power_mw = doc
            .processor
            .as_ref()
            .and_then(|p| p.gpu_power)
            .unwrap_or(0.0);
        let ane_power_mw = doc
            .processor
            .as_ref()
            .and_then(|p| p.ane_power)
            .unwrap_or(0.0);
        let combined_power_mw = doc
            .processor
            .as_ref()
            .and_then(|p| p.combined_power)
            .unwrap_or(cpu_power_mw + gpu_power_mw + ane_power_mw);
        let cpu_energy = doc.processor.as_ref().and_then(|p| p.cpu_energy);
        let gpu_energy = doc.processor.as_ref().and_then(|p| p.gpu_energy);
        let ane_energy = doc.processor.as_ref().and_then(|p| p.ane_energy);
        let battery_percent = doc.battery.as_ref().and_then(|b| b.percent_charge);

        // Derive busy ratios and cluster freqs if present
        let mut e_busy_ratio: Option<f64> = None;
        let mut p_busy_ratio: Option<f64> = None;
        let mut e_freq_hz: Option<f64> = None;
        let mut p_freq_hz: Option<f64> = None;

        let update_max = |target: &mut Option<f64>, candidate: Option<f64>| {
            if let Some(value) = candidate {
                match target {
                    Some(current) if value > *current => {
                        *current = value;
                    }
                    None => {
                        *target = Some(value);
                    }
                    _ => {}
                }
            }
        };

        let is_e_cluster = |name: &str| name.starts_with('E') && name.ends_with("Cluster");
        let is_p_cluster = |name: &str| name.starts_with('P') && name.ends_with("Cluster");

        if let Some(proc) = &doc.processor
            && let Some(clusters) = &proc.clusters
        {
            for c in clusters {
                let name = match &c.name {
                    Some(name) => name.as_str(),
                    None => continue,
                };
                let busy = c.idle_ratio.map(|r| 1.0 - r);

                if is_e_cluster(name) {
                    update_max(&mut e_busy_ratio, busy);
                    update_max(&mut e_freq_hz, c.freq_hz);
                } else if is_p_cluster(name) {
                    update_max(&mut p_busy_ratio, busy);
                    update_max(&mut p_freq_hz, c.freq_hz);
                }
            }
        }

        let cpu_busy_ratio = match (e_busy_ratio, p_busy_ratio) {
            (Some(e), Some(p)) => Some((e + p) / 2.0),
            (Some(e), None) => Some(e),
            (None, Some(p)) => Some(p),
            _ => None,
        };

        Some(Self {
            timestamp,
            cpu_power_mw,
            gpu_power_mw,
            combined_power_mw,
            ane_power_mw,
            cpu_energy,
            gpu_energy,
            ane_energy,
            battery_percent,
            cpu_busy_ratio,
            e_busy_ratio,
            p_busy_ratio,
            e_freq_hz,
            p_freq_hz,
        })
    }
}

// Deserialize directly from powermetrics plist. We only model the pieces we need.
// TODO: make a separate crate that has complete shcema for the powermetrics
// plist format. There's probably some Apple docs that I can find for this
#[derive(Debug, Deserialize)]
pub struct PowermetricsPlist {
    pub timestamp: Option<plist::Value>,
    pub processor: Option<Processor>,
    pub battery: Option<Battery>,
    // GPU sometimes contains duplicate keys (e.g., idle_ns). Parse as raw value to avoid errors.
    #[allow(dead_code)]
    pub gpu: Option<plist::Value>,
}

#[derive(Debug, Deserialize)]
pub struct Battery {
    pub percent_charge: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct Processor {
    pub cpu_power: Option<f64>,
    pub gpu_power: Option<f64>,
    pub combined_power: Option<f64>,
    #[allow(dead_code)]
    pub cpu_energy: Option<u64>,
    #[allow(dead_code)]
    pub gpu_energy: Option<u64>,
    #[allow(dead_code)]
    pub ane_energy: Option<u64>,
    #[allow(dead_code)]
    pub ane_power: Option<f64>,
    pub clusters: Option<Vec<Cluster>>,
}

#[derive(Debug, Deserialize)]
pub struct Cluster {
    pub name: Option<String>,
    #[allow(dead_code)]
    pub hw_resid_counters: Option<bool>,
    pub freq_hz: Option<f64>,
    #[allow(dead_code)]
    pub idle_ns: Option<u64>,
    pub idle_ratio: Option<f64>,
    #[allow(dead_code)]
    pub dvfm_states: Option<Vec<DvfmState>>,
    #[allow(dead_code)]
    pub cpus: Option<Vec<CpuCore>>,
}

#[derive(Debug, Deserialize)]
pub struct CpuCore {
    #[allow(dead_code)]
    pub cpu: Option<u32>,
    #[allow(dead_code)]
    pub freq_hz: Option<f64>,
    #[allow(dead_code)]
    pub idle_ns: Option<u64>,
    #[allow(dead_code)]
    pub idle_ratio: Option<f64>,
    #[allow(dead_code)]
    pub dvfm_states: Option<Vec<DvfmState>>,
}

#[derive(Debug, Deserialize)]
pub struct DvfmState {
    #[allow(dead_code)]
    pub freq: Option<u64>,
    #[allow(dead_code)]
    pub used_ns: Option<u64>,
    #[allow(dead_code)]
    pub used_ratio: Option<f64>,
}

/*
Intentionally not modeling GPU as a struct due to duplicate keys observed in
some powermetrics builds.
TODO: figure out how to read this in a decent way?
Maybe I make custom deserializer, and read into vector?  There's probably a
serde extension that can do this though.
*/
#[cfg(test)]
mod tests {
    use super::*;

    #[inline]
    fn assert_close(a: f64, b: f64, eps: f64) {
        let diff = (a - b).abs();
        assert!(
            diff <= eps,
            "expected {:?} ~= {:?} (diff={}, eps={})",
            a,
            b,
            diff,
            eps
        );
    }

    #[test]
    fn parses_sample_powermetrics_xml() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/powermetrics.xml"
        ));
        let doc: PowermetricsPlist = plist::from_bytes(bytes).expect("plist parse");
        let sample = PowermetricsSample::from_plist(&doc).expect("to sample");

        insta::assert_debug_snapshot!(&sample);
    }

    #[test]
    fn handles_missing_battery() {
        let doc = PowermetricsPlist {
            timestamp: None,
            processor: Some(Processor {
                cpu_power: Some(1000.0),
                gpu_power: Some(200.0),
                combined_power: Some(1200.0),
                cpu_energy: None,
                gpu_energy: None,
                ane_energy: None,
                ane_power: None,
                clusters: None,
            }),
            battery: None,
            gpu: None,
        };

        let sample = PowermetricsSample::from_plist(&doc).expect("should parse");
        assert_close(sample.cpu_power_mw, 1000.0, 0.01);
        assert_close(sample.gpu_power_mw, 200.0, 0.01);
        assert_close(sample.ane_power_mw, 0.0, 0.01);
        assert_close(sample.combined_power_mw, 1200.0, 0.01);
        assert_eq!(sample.battery_percent, None);
        assert_eq!(sample.cpu_energy, None);
        assert_eq!(sample.gpu_energy, None);
        assert_eq!(sample.ane_energy, None);
    }

    #[test]
    fn handles_missing_clusters() {
        let doc = PowermetricsPlist {
            timestamp: None,
            processor: Some(Processor {
                cpu_power: Some(500.0),
                gpu_power: Some(100.0),
                combined_power: None, // Will compute from cpu + gpu
                cpu_energy: None,
                gpu_energy: None,
                ane_energy: None,
                ane_power: None,
                clusters: None,
            }),
            battery: Some(Battery {
                percent_charge: Some(50),
            }),
            gpu: None,
        };

        let sample = PowermetricsSample::from_plist(&doc).expect("should parse");
        assert_close(sample.cpu_power_mw, 500.0, 0.01);
        assert_close(sample.gpu_power_mw, 100.0, 0.01);
        assert_close(sample.combined_power_mw, 600.0, 0.01); // Computed
        assert_eq!(sample.battery_percent, Some(50));
        assert_eq!(sample.e_busy_ratio, None);
        assert_eq!(sample.p_busy_ratio, None);
        assert_eq!(sample.e_freq_hz, None);
        assert_eq!(sample.p_freq_hz, None);
    }

    #[test]
    fn handles_single_cluster() {
        let doc = PowermetricsPlist {
            timestamp: None,
            processor: Some(Processor {
                cpu_power: Some(1500.0),
                gpu_power: None,
                combined_power: None,
                cpu_energy: None,
                gpu_energy: None,
                ane_energy: None,
                ane_power: None,
                clusters: Some(vec![Cluster {
                    name: Some("E0-Cluster".to_string()),
                    hw_resid_counters: None,
                    freq_hz: Some(1.2e9),
                    idle_ns: None,
                    idle_ratio: Some(0.6),
                    dvfm_states: None,
                    cpus: None,
                }]),
            }),
            battery: None,
            gpu: None,
        };

        let sample = PowermetricsSample::from_plist(&doc).expect("should parse");
        assert_close(sample.e_busy_ratio.unwrap(), 0.4, 0.01);
        assert_eq!(sample.p_busy_ratio, None);
        assert_close(sample.e_freq_hz.unwrap(), 1.2e9, 1.0);
        assert_eq!(sample.p_freq_hz, None);
    }

    #[test]
    fn aggregates_multiple_p_clusters_with_max() {
        let doc = PowermetricsPlist {
            timestamp: None,
            processor: Some(Processor {
                cpu_power: Some(2500.0),
                gpu_power: None,
                combined_power: None,
                cpu_energy: None,
                gpu_energy: None,
                ane_energy: None,
                ane_power: None,
                clusters: Some(vec![
                    Cluster {
                        name: Some("P0-Cluster".to_string()),
                        hw_resid_counters: None,
                        freq_hz: Some(2.2e9),
                        idle_ns: None,
                        idle_ratio: Some(0.2),
                        dvfm_states: None,
                        cpus: None,
                    },
                    Cluster {
                        name: Some("P1-Cluster".to_string()),
                        hw_resid_counters: None,
                        freq_hz: Some(3.1e9),
                        idle_ns: None,
                        idle_ratio: Some(0.4),
                        dvfm_states: None,
                        cpus: None,
                    },
                ]),
            }),
            battery: None,
            gpu: None,
        };

        let sample = PowermetricsSample::from_plist(&doc).expect("should parse");
        assert_close(sample.p_busy_ratio.unwrap(), 0.8, 1e-6);
        assert_close(sample.p_freq_hz.unwrap(), 3.1e9, 1.0);
        assert_close(sample.cpu_busy_ratio.unwrap(), 0.8, 1e-6);
    }

    #[test]
    fn handles_zero_power() {
        let doc = PowermetricsPlist {
            timestamp: None,
            processor: Some(Processor {
                cpu_power: Some(0.0),
                gpu_power: Some(0.0),
                combined_power: Some(0.0),
                cpu_energy: None,
                gpu_energy: None,
                ane_energy: None,
                ane_power: None,
                clusters: None,
            }),
            battery: Some(Battery {
                percent_charge: Some(100),
            }),
            gpu: None,
        };

        let sample = PowermetricsSample::from_plist(&doc).expect("should parse");
        assert_close(sample.cpu_power_mw, 0.0, 1e-6);
        assert_close(sample.gpu_power_mw, 0.0, 1e-6);
        assert_close(sample.combined_power_mw, 0.0, 1e-6);
    }

    #[test]
    fn handles_missing_processor() {
        let doc = PowermetricsPlist {
            timestamp: None,
            processor: None,
            battery: Some(Battery {
                percent_charge: Some(75),
            }),
            gpu: None,
        };

        let sample = PowermetricsSample::from_plist(&doc).expect("should parse");
        // Without processor, power values default to 0.0
        assert_close(sample.cpu_power_mw, 0.0, 1e-6);
        assert_close(sample.gpu_power_mw, 0.0, 1e-6);
        assert_close(sample.combined_power_mw, 0.0, 1e-6);
        assert_eq!(sample.battery_percent, Some(75));
        assert_eq!(sample.e_busy_ratio, None);
        assert_eq!(sample.p_busy_ratio, None);
    }
}
