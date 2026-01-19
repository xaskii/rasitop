use crate::sources::{
    IOHIDSensors, IOReport, SMC, SocInfo, cfio_get_residencies, cfio_watts, libc_ram, libc_swap,
};
use serde::Serialize;
use std::collections::HashMap;

const CPU_FREQ_CORE_SUBG: &str = "CPU Core Performance States";

#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub timestamp: Option<String>,
    pub cpu_power_mw: f64,
    pub gpu_power_mw: f64,
    pub combined_power_mw: f64,
    pub ane_power_mw: f64,
    pub sys_power_mw: Option<f64>,
    pub cpu_energy: Option<u64>,
    pub gpu_energy: Option<u64>,
    pub ane_energy: Option<u64>,
    pub battery_percent: Option<u8>,
    pub cpu_busy_ratio: Option<f64>,
    pub e_busy_ratio: Option<f64>,
    pub p_busy_ratio: Option<f64>,
    pub e_freq_hz: Option<f64>,
    pub p_freq_hz: Option<f64>,
    pub cpu_temp_c: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub ram_total_bytes: Option<u64>,
    pub ram_usage_bytes: Option<u64>,
    pub swap_total_bytes: Option<u64>,
    pub swap_usage_bytes: Option<u64>,
    pub cpu_cores: Option<Vec<CoreSample>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoreSample {
    pub label: String,
    pub busy_ratio: f64,
}

#[derive(Debug, Default, Clone, Copy)]
struct CoreUsage {
    freq_mhz: f64,
    busy_ratio: f64,
    from_max: f64,
}

#[derive(Debug, Default)]
struct ClusterAgg {
    max_busy_ratio: f64,
    max_freq_mhz: f64,
    max_from_max: f64,
}

#[derive(Debug, Default)]
struct MetricsSample {
    cpu_power_w: f64,
    gpu_power_w: f64,
    ane_power_w: f64,
    e_cluster: Option<ClusterAgg>,
    p_cluster: Option<ClusterAgg>,
}

fn zero_div<T: core::ops::Div<Output = T> + Default + PartialEq>(a: T, b: T) -> T {
    let zero: T = Default::default();
    if b == zero { zero } else { a / b }
}

fn calc_core_usage(
    item: core_foundation::dictionary::CFDictionaryRef,
    freqs: &[u32],
) -> Option<CoreUsage> {
    if freqs.is_empty() {
        return None;
    }

    let items = unsafe { cfio_get_residencies(item) };
    if items.len() <= freqs.len() {
        return None;
    }

    let offset = items
        .iter()
        .position(|x| x.0 != "IDLE" && x.0 != "DOWN" && x.0 != "OFF")?;

    if items.len() < offset + freqs.len() {
        return None;
    }

    let usage = items.iter().map(|x| x.1 as f64).skip(offset).sum::<f64>();
    let total = items.iter().map(|x| x.1 as f64).sum::<f64>();
    if usage <= 0.0 || total <= 0.0 {
        return None;
    }

    let mut avg_freq = 0f64;
    for i in 0..freqs.len() {
        let percent = zero_div(items[i + offset].1 as f64, usage);
        avg_freq += percent * freqs[i] as f64;
    }

    let usage_ratio = zero_div(usage, total);
    let min_freq = *freqs.first().unwrap() as f64;
    let max_freq = *freqs.last().unwrap() as f64;
    let from_max = (avg_freq.max(min_freq) * usage_ratio) / max_freq;

    Some(CoreUsage {
        freq_mhz: avg_freq,
        busy_ratio: usage_ratio,
        from_max,
    })
}

fn core_label_from_channel(channel: &str) -> String {
    let cluster = if channel.contains("ECPU") {
        "E"
    } else if channel.contains("PCPU") {
        "P"
    } else {
        "C"
    };

    let index = channel
        .split(|c: char| !c.is_ascii_digit())
        .rfind(|part| !part.is_empty())
        .and_then(|part| part.parse::<u32>().ok());

    match index {
        Some(idx) => format!("{cluster}{idx}"),
        None => channel.to_string(),
    }
}

fn core_sort_key(label: &str) -> (u8, u32, String) {
    let mut chars = label.chars();
    let cluster = match chars.next() {
        Some('E') => 0,
        Some('P') => 1,
        _ => 2,
    };
    let index = chars.as_str().parse::<u32>().unwrap_or(0);
    (cluster, index, label.to_string())
}

fn aggregate_cluster(cores: &[CoreUsage]) -> Option<ClusterAgg> {
    if cores.is_empty() {
        return None;
    }

    let mut agg = ClusterAgg::default();
    for core in cores {
        if core.busy_ratio > agg.max_busy_ratio {
            agg.max_busy_ratio = core.busy_ratio;
        }
        if core.freq_mhz > agg.max_freq_mhz {
            agg.max_freq_mhz = core.freq_mhz;
        }
        if core.from_max > agg.max_from_max {
            agg.max_from_max = core.from_max;
        }
    }

    Some(agg)
}

fn avg_f64(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn avg_f32(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f32>() / values.len() as f32)
    }
}

fn init_smc() -> anyhow::Result<(SMC, Vec<String>, Vec<String>)> {
    let mut smc = SMC::new()?;
    const FLOAT_TYPE: u32 = 1718383648; // FourCC: "flt "

    let mut cpu_sensors = Vec::new();
    let mut gpu_sensors = Vec::new();

    let names = smc.read_all_keys().unwrap_or_default();
    for name in &names {
        let key = match smc.read_key_info(name) {
            Ok(key) => key,
            Err(_) => continue,
        };

        if key.data_size != 4 || key.data_type != FLOAT_TYPE {
            continue;
        }

        if smc.read_val(name).is_err() {
            continue;
        }

        match name {
            name if name.starts_with("Tp") || name.starts_with("Te") => {
                cpu_sensors.push(name.clone())
            }
            name if name.starts_with("Tg") => gpu_sensors.push(name.clone()),
            _ => (),
        }
    }

    Ok((smc, cpu_sensors, gpu_sensors))
}

fn get_temp_smc(
    smc: &mut SMC,
    cpu_keys: &[String],
    gpu_keys: &[String],
) -> anyhow::Result<(Option<f32>, Option<f32>)> {
    let mut cpu_metrics = Vec::new();
    for sensor in cpu_keys {
        let val = smc.read_val(sensor)?;
        let val = f32::from_le_bytes(val.data[0..4].try_into().unwrap());
        if val != 0.0 {
            cpu_metrics.push(val);
        }
    }

    let mut gpu_metrics = Vec::new();
    for sensor in gpu_keys {
        let val = smc.read_val(sensor)?;
        let val = f32::from_le_bytes(val.data[0..4].try_into().unwrap());
        if val != 0.0 {
            gpu_metrics.push(val);
        }
    }

    Ok((avg_f32(&cpu_metrics), avg_f32(&gpu_metrics)))
}

fn get_temp_hid(hid: &IOHIDSensors) -> anyhow::Result<(Option<f32>, Option<f32>)> {
    let metrics = hid.get_metrics();

    let mut cpu_values = Vec::new();
    let mut gpu_values = Vec::new();

    for (name, value) in &metrics {
        if name.starts_with("pACC MTR Temp Sensor") || name.starts_with("eACC MTR Temp Sensor") {
            cpu_values.push(*value);
            continue;
        }

        if name.starts_with("GPU MTR Temp Sensor") {
            gpu_values.push(*value);
            continue;
        }
    }

    Ok((avg_f32(&cpu_values), avg_f32(&gpu_values)))
}

pub struct Sampler {
    soc: SocInfo,
    ior: IOReport,
    hid: IOHIDSensors,
    smc: SMC,
    smc_cpu_keys: Vec<String>,
    smc_gpu_keys: Vec<String>,
}

impl Sampler {
    pub fn new() -> anyhow::Result<Self> {
        let channels = vec![
            ("Energy Model", None),
            ("CPU Stats", Some(CPU_FREQ_CORE_SUBG)),
        ];

        let soc = SocInfo::new()?;
        let ior = IOReport::new(channels)?;
        let hid = IOHIDSensors::new()?;
        let (smc, smc_cpu_keys, smc_gpu_keys) = init_smc()?;

        Ok(Self {
            soc,
            ior,
            hid,
            smc,
            smc_cpu_keys,
            smc_gpu_keys,
        })
    }

    fn get_temp(&mut self) -> anyhow::Result<(Option<f32>, Option<f32>)> {
        if !self.smc_cpu_keys.is_empty() {
            get_temp_smc(&mut self.smc, &self.smc_cpu_keys, &self.smc_gpu_keys)
        } else {
            get_temp_hid(&self.hid)
        }
    }

    fn get_mem(&self) -> anyhow::Result<(u64, u64, u64, u64)> {
        let (ram_usage, ram_total) = libc_ram()?;
        let (swap_usage, swap_total) = libc_swap()?;
        Ok((ram_usage, ram_total, swap_usage, swap_total))
    }

    fn get_sys_power(&mut self) -> anyhow::Result<f64> {
        let val = self.smc.read_val("PSTR")?;
        let val = f32::from_le_bytes(val.data.clone().try_into().unwrap());
        Ok(val as f64)
    }

    pub fn sample(&mut self, duration_ms: u32) -> anyhow::Result<Sample> {
        let measures: usize = 4;
        let mut results: Vec<MetricsSample> = Vec::with_capacity(measures);
        let mut core_busy: HashMap<String, (f64, u32)> = HashMap::new();

        for (sample, dt) in self.ior.get_samples(duration_ms as u64, measures) {
            let mut ecpu_usages = Vec::new();
            let mut pcpu_usages = Vec::new();
            let mut rs = MetricsSample::default();

            for x in sample {
                if x.group == "CPU Stats" && x.subgroup == CPU_FREQ_CORE_SUBG {
                    if x.channel.contains("ECPU") {
                        if let Some(usage) = calc_core_usage(x.item, &self.soc.ecpu_freqs) {
                            let label = core_label_from_channel(&x.channel);
                            let entry = core_busy.entry(label).or_insert((0.0, 0));
                            entry.0 += usage.busy_ratio;
                            entry.1 += 1;
                            ecpu_usages.push(usage);
                        }
                        continue;
                    }

                    if x.channel.contains("PCPU") {
                        if let Some(usage) = calc_core_usage(x.item, &self.soc.pcpu_freqs) {
                            let label = core_label_from_channel(&x.channel);
                            let entry = core_busy.entry(label).or_insert((0.0, 0));
                            entry.0 += usage.busy_ratio;
                            entry.1 += 1;
                            pcpu_usages.push(usage);
                        }
                        continue;
                    }
                }

                if x.group == "Energy Model" {
                    match x.channel.as_str() {
                        "GPU Energy" => {
                            rs.gpu_power_w += unsafe { cfio_watts(x.item, &x.unit, dt)? } as f64
                        }
                        c if c.ends_with("CPU Energy") => {
                            rs.cpu_power_w += unsafe { cfio_watts(x.item, &x.unit, dt)? } as f64
                        }
                        c if c.starts_with("ANE") => {
                            rs.ane_power_w += unsafe { cfio_watts(x.item, &x.unit, dt)? } as f64
                        }
                        _ => {}
                    }
                }
            }

            rs.e_cluster = aggregate_cluster(&ecpu_usages);
            rs.p_cluster = aggregate_cluster(&pcpu_usages);
            results.push(rs);
        }

        let cpu_power_w = results.iter().map(|x| x.cpu_power_w).sum::<f64>() / measures as f64;
        let gpu_power_w = results.iter().map(|x| x.gpu_power_w).sum::<f64>() / measures as f64;
        let ane_power_w = results.iter().map(|x| x.ane_power_w).sum::<f64>() / measures as f64;

        let mut e_busy_vals = Vec::new();
        let mut p_busy_vals = Vec::new();
        let mut e_freq_vals = Vec::new();
        let mut p_freq_vals = Vec::new();

        for item in &results {
            if let Some(agg) = &item.e_cluster {
                e_busy_vals.push(agg.max_busy_ratio);
                e_freq_vals.push(agg.max_freq_mhz);
            }
            if let Some(agg) = &item.p_cluster {
                p_busy_vals.push(agg.max_busy_ratio);
                p_freq_vals.push(agg.max_freq_mhz);
            }
        }

        let e_busy_ratio = avg_f64(&e_busy_vals);
        let p_busy_ratio = avg_f64(&p_busy_vals);
        let e_freq_hz = avg_f64(&e_freq_vals).map(|v| v * 1e6);
        let p_freq_hz = avg_f64(&p_freq_vals).map(|v| v * 1e6);

        let cpu_busy_ratio = match (e_busy_ratio, p_busy_ratio) {
            (Some(e), Some(p)) => Some((e + p) / 2.0),
            (Some(e), None) => Some(e),
            (None, Some(p)) => Some(p),
            _ => None,
        };

        let cpu_cores = if core_busy.is_empty() {
            None
        } else {
            let mut cores: Vec<CoreSample> = core_busy
                .into_iter()
                .filter_map(|(label, (sum, count))| {
                    if count == 0 {
                        None
                    } else {
                        Some(CoreSample {
                            label,
                            busy_ratio: sum / count as f64,
                        })
                    }
                })
                .collect();
            cores.sort_by_key(|core| core_sort_key(&core.label));
            Some(cores)
        };

        let (cpu_temp_c, gpu_temp_c) = self.get_temp().unwrap_or((None, None));
        let (ram_usage, ram_total, swap_usage, swap_total) = self.get_mem().unwrap_or((0, 0, 0, 0));

        let sys_power_mw = match self.get_sys_power() {
            Ok(val) if val > 0.0 => Some(val * 1000.0),
            _ => None,
        };

        let combined_power_mw = (cpu_power_w + gpu_power_w + ane_power_w) * 1000.0;

        Ok(Sample {
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            cpu_power_mw: cpu_power_w * 1000.0,
            gpu_power_mw: gpu_power_w * 1000.0,
            combined_power_mw,
            ane_power_mw: ane_power_w * 1000.0,
            sys_power_mw,
            cpu_energy: None,
            gpu_energy: None,
            ane_energy: None,
            battery_percent: None,
            cpu_busy_ratio,
            e_busy_ratio,
            p_busy_ratio,
            e_freq_hz,
            p_freq_hz,
            cpu_temp_c,
            gpu_temp_c,
            ram_total_bytes: if ram_total > 0 { Some(ram_total) } else { None },
            ram_usage_bytes: if ram_total > 0 { Some(ram_usage) } else { None },
            swap_total_bytes: if swap_total > 0 {
                Some(swap_total)
            } else {
                None
            },
            swap_usage_bytes: if swap_total > 0 {
                Some(swap_usage)
            } else {
                None
            },
            cpu_cores,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ClusterAgg, CoreUsage, aggregate_cluster, avg_f32, avg_f64};

    #[test]
    fn aggregate_cluster_tracks_max_values() {
        let items = vec![
            CoreUsage {
                freq_mhz: 1200.0,
                busy_ratio: 0.4,
                from_max: 0.35,
            },
            CoreUsage {
                freq_mhz: 1800.0,
                busy_ratio: 0.7,
                from_max: 0.62,
            },
            CoreUsage {
                freq_mhz: 1600.0,
                busy_ratio: 0.5,
                from_max: 0.55,
            },
        ];

        let agg = aggregate_cluster(&items).unwrap_or(ClusterAgg::default());
        assert!((agg.max_busy_ratio - 0.7).abs() < 1e-6);
        assert!((agg.max_freq_mhz - 1800.0).abs() < 1e-6);
        assert!((agg.max_from_max - 0.62).abs() < 1e-6);
    }

    #[test]
    fn averages_handle_empty() {
        assert_eq!(avg_f64(&[]), None);
        assert_eq!(avg_f32(&[]), None);
    }
}
