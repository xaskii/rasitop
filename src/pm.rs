use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct PowermetricsSample {
    pub timestamp: Option<String>,
    pub cpu_power_mw: f64,
    pub gpu_power_mw: f64,
    pub combined_power_mw: f64,
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
        let combined_power_mw = doc
            .processor
            .as_ref()
            .and_then(|p| p.combined_power)
            .unwrap_or(cpu_power_mw + gpu_power_mw);
        let battery_percent = doc.battery.as_ref().and_then(|b| b.percent_charge);

        // Derive busy ratios and cluster freqs if present
        let mut e_busy_ratio: Option<f64> = None;
        let mut p_busy_ratio: Option<f64> = None;
        let mut e_freq_hz: Option<f64> = None;
        let mut p_freq_hz: Option<f64> = None;

        if let Some(proc) = &doc.processor
            && let Some(clusters) = &proc.clusters
        {
            for c in clusters {
                if let Some(name) = &c.name {
                    let busy = c.idle_ratio.map(|r| 1.0 - r);
                    match name.as_str() {
                        "E-Cluster" => {
                            e_busy_ratio = busy;
                            e_freq_hz = c.freq_hz;
                        }
                        "P-Cluster" => {
                            p_busy_ratio = busy;
                            p_freq_hz = c.freq_hz;
                        }
                        _ => {}
                    }
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
        let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/powermetrics.xml"));
        let doc: PowermetricsPlist = plist::from_bytes(bytes).expect("plist parse");
        let sample = PowermetricsSample::from_plist(&doc).expect("to sample");

        assert_close(sample.cpu_power_mw, 1941.82, 0.05);
        assert_close(sample.gpu_power_mw, 0.0, 1e-6);
        assert_close(sample.combined_power_mw, 1941.82, 0.05);
        assert_eq!(sample.battery_percent, Some(69));

        assert_close(sample.e_busy_ratio.unwrap(), 1.0 - 0.611736, 1e-6);
        assert_close(sample.p_busy_ratio.unwrap(), 1.0 - 0.307891, 1e-6);

        assert_close(sample.e_freq_hz.unwrap(), 972_013_000.0, 1_000.0);
        assert_close(sample.p_freq_hz.unwrap(), 3_089_180_000.0, 1_000.0);

        assert!(sample.timestamp.is_some());
    }
}
