use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow, bail, ensure};
use quick_xml::Reader;
use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesStart, Event};
use serde::Serialize;

const ACTIVITY_SCHEMA: &str = "activity-monitor-process-live";

#[derive(Debug, Serialize)]
pub struct ActivitySummary {
    schema_version: u32,
    process: ProcessSummary,
    measurement: MeasurementSummary,
    cpu: CpuSummary,
    idle_wakeups: CounterSummary,
    memory: MemorySummary,
    threads: GaugeSummary,
    ports: GaugeSummary,
    disk_io: DiskIoSummary,
}

#[derive(Debug, Serialize)]
struct ProcessSummary {
    name: String,
    pid: u32,
}

#[derive(Debug, Serialize)]
struct MeasurementSummary {
    samples: usize,
    start_ns: u64,
    end_ns: u64,
    duration_ns: u64,
    duration_seconds: f64,
}

#[derive(Debug, Serialize)]
struct CpuSummary {
    time_start_ns: u64,
    time_end_ns: u64,
    time_delta_ns: u64,
    time_delta_seconds: f64,
    average_percent: f64,
}

#[derive(Debug, Serialize)]
struct CounterSummary {
    start: u64,
    end: u64,
    delta: u64,
    per_second: f64,
}

#[derive(Debug, Serialize)]
struct GaugeSummary {
    start: u64,
    end: u64,
    delta: i64,
    min: u64,
    max: u64,
}

#[derive(Debug, Serialize)]
struct MemorySummary {
    physical_footprint_bytes: GaugeSummary,
    private_bytes: GaugeSummary,
}

#[derive(Debug, Serialize)]
struct DiskIoSummary {
    bytes_read: CounterSummary,
    bytes_written: CounterSummary,
}

#[derive(Debug)]
struct Sample {
    start_ns: u64,
    process_name: String,
    pid: u32,
    cpu_total_ns: u64,
    thread_count: u64,
    port_count: u64,
    physical_footprint_bytes: u64,
    private_bytes: u64,
    idle_wakeups: u64,
    disk_bytes_written: u64,
    disk_bytes_read: u64,
}

#[derive(Debug)]
struct Element {
    name: String,
    attributes: HashMap<String, String>,
    text: String,
    children: Vec<Element>,
}

impl Element {
    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }

    fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.name == name)
    }
}

/// Summarize rows exported from xctrace's `activity-monitor-process-live` schema.
///
/// xctrace deduplicates XML values by replacing later elements with `ref`
/// attributes. This parser resolves those references before calculating deltas.
pub fn summarize_activity_xml(xml: &str) -> Result<ActivitySummary> {
    let root = parse_xml(xml).context("parse Activity Monitor XML")?;
    let node = find_activity_node(&root)
        .ok_or_else(|| anyhow!("export does not contain the {ACTIVITY_SCHEMA} schema"))?;
    let schema = node
        .child("schema")
        .context("Activity Monitor export is missing its schema")?;
    let columns = schema_columns(schema)?;
    let ids = collect_ids(&root)?;

    let samples = node
        .children
        .iter()
        .filter(|element| element.name == "row")
        .map(|row| parse_sample(row, &columns, &ids))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        samples.len() >= 2,
        "Activity Monitor export needs at least two samples, found {}",
        samples.len()
    );

    let first = &samples[0];
    let last = samples.last().expect("sample length checked above");
    ensure!(
        samples
            .iter()
            .all(|sample| sample.pid == first.pid && sample.process_name == first.process_name),
        "Activity Monitor export contains more than one process"
    );
    ensure!(
        samples
            .windows(2)
            .all(|pair| pair[0].start_ns < pair[1].start_ns),
        "Activity Monitor sample times are not strictly increasing"
    );

    let duration_ns = last
        .start_ns
        .checked_sub(first.start_ns)
        .context("Activity Monitor sample time decreased")?;
    ensure!(
        duration_ns > 0,
        "Activity Monitor observation duration is zero"
    );
    let duration_seconds = duration_ns as f64 / 1_000_000_000.0;
    let cpu_delta_ns = monotonic_delta(first.cpu_total_ns, last.cpu_total_ns, "CPU time")?;

    Ok(ActivitySummary {
        schema_version: 1,
        process: ProcessSummary {
            name: first.process_name.clone(),
            pid: first.pid,
        },
        measurement: MeasurementSummary {
            samples: samples.len(),
            start_ns: first.start_ns,
            end_ns: last.start_ns,
            duration_ns,
            duration_seconds,
        },
        cpu: CpuSummary {
            time_start_ns: first.cpu_total_ns,
            time_end_ns: last.cpu_total_ns,
            time_delta_ns: cpu_delta_ns,
            time_delta_seconds: cpu_delta_ns as f64 / 1_000_000_000.0,
            average_percent: cpu_delta_ns as f64 / duration_ns as f64 * 100.0,
        },
        idle_wakeups: counter_summary(
            first.idle_wakeups,
            last.idle_wakeups,
            duration_seconds,
            "idle wakeups",
        )?,
        memory: MemorySummary {
            physical_footprint_bytes: gauge_summary(
                samples.iter().map(|sample| sample.physical_footprint_bytes),
            )?,
            private_bytes: gauge_summary(samples.iter().map(|sample| sample.private_bytes))?,
        },
        threads: gauge_summary(samples.iter().map(|sample| sample.thread_count))?,
        ports: gauge_summary(samples.iter().map(|sample| sample.port_count))?,
        disk_io: DiskIoSummary {
            bytes_read: counter_summary(
                first.disk_bytes_read,
                last.disk_bytes_read,
                duration_seconds,
                "disk bytes read",
            )?,
            bytes_written: counter_summary(
                first.disk_bytes_written,
                last.disk_bytes_written,
                duration_seconds,
                "disk bytes written",
            )?,
        },
    })
}

fn parse_sample<'a>(
    row: &'a Element,
    columns: &HashMap<&str, usize>,
    ids: &HashMap<&'a str, &'a Element>,
) -> Result<Sample> {
    let value = |mnemonic: &str| -> Result<&Element> {
        let index = columns
            .get(mnemonic)
            .copied()
            .with_context(|| format!("Activity Monitor schema is missing {mnemonic}"))?;
        let element = row
            .children
            .get(index)
            .with_context(|| format!("Activity Monitor row has no value for {mnemonic}"))?;
        resolve_reference(element, ids)
            .with_context(|| format!("resolve Activity Monitor value for {mnemonic}"))
    };

    let pid = parse_number(value("pid")?, "pid")?;
    let pid = u32::try_from(pid).context("Activity Monitor pid does not fit in u32")?;
    let process = value("process")?;
    let process_name = process
        .attribute("fmt")
        .filter(|name| !name.is_empty())
        .unwrap_or(process.text.trim());
    ensure!(
        !process_name.is_empty(),
        "Activity Monitor process name is empty"
    );
    let pid_suffix = format!(" ({pid})");
    let process_name = process_name
        .strip_suffix(&pid_suffix)
        .unwrap_or(process_name)
        .to_owned();

    Ok(Sample {
        start_ns: parse_number(value("start")?, "start")?,
        process_name,
        pid,
        cpu_total_ns: parse_number(value("cpu-total")?, "cpu-total")?,
        thread_count: parse_number(value("thread-count")?, "thread-count")?,
        port_count: parse_number(value("mach-port-count")?, "mach-port-count")?,
        physical_footprint_bytes: parse_number(
            value("memory-physical-footprint")?,
            "memory-physical-footprint",
        )?,
        private_bytes: parse_number(value("memory-real-private")?, "memory-real-private")?,
        idle_wakeups: parse_number(value("idle-wakeups")?, "idle-wakeups")?,
        disk_bytes_written: parse_number(value("disk-bytes-written")?, "disk-bytes-written")?,
        disk_bytes_read: parse_number(value("disk-bytes-read")?, "disk-bytes-read")?,
    })
}

fn parse_number(element: &Element, mnemonic: &str) -> Result<u64> {
    ensure!(
        element.name != "sentinel",
        "Activity Monitor value for {mnemonic} is unavailable"
    );
    element
        .text
        .trim()
        .parse()
        .with_context(|| format!("parse Activity Monitor value for {mnemonic}"))
}

fn counter_summary(
    start: u64,
    end: u64,
    duration_seconds: f64,
    name: &str,
) -> Result<CounterSummary> {
    let delta = monotonic_delta(start, end, name)?;
    Ok(CounterSummary {
        start,
        end,
        delta,
        per_second: delta as f64 / duration_seconds,
    })
}

fn monotonic_delta(start: u64, end: u64, name: &str) -> Result<u64> {
    end.checked_sub(start)
        .with_context(|| format!("Activity Monitor {name} counter decreased from {start} to {end}"))
}

fn gauge_summary(values: impl Iterator<Item = u64>) -> Result<GaugeSummary> {
    let values = values.collect::<Vec<_>>();
    let start = *values.first().context("Activity Monitor gauge is empty")?;
    let end = *values.last().context("Activity Monitor gauge is empty")?;
    let delta = i64::try_from(i128::from(end) - i128::from(start))
        .context("Activity Monitor gauge delta does not fit in i64")?;
    Ok(GaugeSummary {
        start,
        end,
        delta,
        min: *values.iter().min().expect("gauge is nonempty"),
        max: *values.iter().max().expect("gauge is nonempty"),
    })
}

fn schema_columns(schema: &Element) -> Result<HashMap<&str, usize>> {
    let mut columns = HashMap::new();
    for (index, column) in schema
        .children
        .iter()
        .filter(|element| element.name == "col")
        .enumerate()
    {
        let mnemonic = column
            .child("mnemonic")
            .map(|element| element.text.trim())
            .filter(|mnemonic| !mnemonic.is_empty())
            .context("Activity Monitor schema column has no mnemonic")?;
        if columns.insert(mnemonic, index).is_some() {
            bail!("Activity Monitor schema contains duplicate column {mnemonic}");
        }
    }
    ensure!(
        !columns.is_empty(),
        "Activity Monitor schema has no columns"
    );
    Ok(columns)
}

fn find_activity_node(element: &Element) -> Option<&Element> {
    if element.name == "node"
        && element
            .child("schema")
            .and_then(|schema| schema.attribute("name"))
            == Some(ACTIVITY_SCHEMA)
    {
        return Some(element);
    }
    element.children.iter().find_map(find_activity_node)
}

fn collect_ids(root: &Element) -> Result<HashMap<&str, &Element>> {
    fn visit<'a>(element: &'a Element, ids: &mut HashMap<&'a str, &'a Element>) -> Result<()> {
        if let Some(id) = element.attribute("id")
            && ids.insert(id, element).is_some()
        {
            bail!("Activity Monitor export contains duplicate id {id}");
        }
        for child in &element.children {
            visit(child, ids)?;
        }
        Ok(())
    }

    let mut ids = HashMap::new();
    visit(root, &mut ids)?;
    Ok(ids)
}

fn resolve_reference<'a>(
    mut element: &'a Element,
    ids: &HashMap<&'a str, &'a Element>,
) -> Result<&'a Element> {
    let mut visited = HashSet::new();
    while let Some(reference) = element.attribute("ref") {
        ensure!(
            visited.insert(reference),
            "Activity Monitor export contains a reference cycle at {reference}"
        );
        element = ids.get(reference).copied().with_context(|| {
            format!("Activity Monitor export references unknown id {reference}")
        })?;
    }
    Ok(element)
}

fn parse_xml(xml: &str) -> Result<Element> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack = Vec::new();
    let mut root = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                stack.push(new_element(&start, reader.decoder())?);
            }
            Ok(Event::Empty(start)) => {
                let element = new_element(&start, reader.decoder())?;
                add_element(element, &mut stack, &mut root)?;
            }
            Ok(Event::Text(text)) => {
                let value = text.decode().context("decode XML text")?;
                if let Some(element) = stack.last_mut() {
                    element.text.push_str(&value);
                }
            }
            Ok(Event::CData(text)) => {
                let value = text.decode().context("decode XML CDATA")?;
                if let Some(element) = stack.last_mut() {
                    element.text.push_str(&value);
                }
            }
            Ok(Event::End(end)) => {
                let element = stack.pop().context("XML closing tag has no opening tag")?;
                ensure!(
                    element.name.as_bytes() == end.name().as_ref(),
                    "XML closing tag does not match {}",
                    element.name
                );
                add_element(element, &mut stack, &mut root)?;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("read XML near byte {}", reader.error_position()));
            }
        }
    }

    ensure!(
        stack.is_empty(),
        "XML ended before all elements were closed"
    );
    root.context("XML has no root element")
}

fn new_element(start: &BytesStart<'_>, decoder: Decoder) -> Result<Element> {
    let name = std::str::from_utf8(start.name().as_ref())
        .context("XML element name is not UTF-8")?
        .to_owned();
    let mut attributes = HashMap::new();
    for attribute in start.attributes() {
        let attribute = attribute.context("parse XML attribute")?;
        let key = std::str::from_utf8(attribute.key.as_ref())
            .context("XML attribute name is not UTF-8")?
            .to_owned();
        let value = attribute
            .decode_and_unescape_value(decoder)
            .context("decode XML attribute value")?
            .into_owned();
        if attributes.insert(key.clone(), value).is_some() {
            bail!("XML element {name} contains duplicate attribute {key}");
        }
    }
    Ok(Element {
        name,
        attributes,
        text: String::new(),
        children: Vec::new(),
    })
}

fn add_element(element: Element, stack: &mut [Element], root: &mut Option<Element>) -> Result<()> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(element);
    } else {
        ensure!(root.is_none(), "XML has more than one root element");
        *root = Some(element);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0"?>
<trace-query-result><node><schema name="activity-monitor-process-live">
<col><mnemonic>process</mnemonic></col>
<col><mnemonic>start</mnemonic></col>
<col><mnemonic>pid</mnemonic></col>
<col><mnemonic>cpu-total</mnemonic></col>
<col><mnemonic>thread-count</mnemonic></col>
<col><mnemonic>mach-port-count</mnemonic></col>
<col><mnemonic>memory-physical-footprint</mnemonic></col>
<col><mnemonic>memory-real-private</mnemonic></col>
<col><mnemonic>idle-wakeups</mnemonic></col>
<col><mnemonic>disk-bytes-written</mnemonic></col>
<col><mnemonic>disk-bytes-read</mnemonic></col>
</schema>
<row><process id="1" fmt="rasitop &amp; helper (42)"/><start-time>0</start-time><pid id="2">42</pid><duration-on-core>50000000</duration-on-core><event-count id="3">4</event-count><event-count id="4">100</event-count><size-in-bytes>1000</size-in-bytes><size-in-bytes>500</size-in-bytes><event-count>10</event-count><disk-size-in-bytes>100</disk-size-in-bytes><disk-size-in-bytes>200</disk-size-in-bytes></row>
<row><process ref="1"/><start-time>1000000000</start-time><pid ref="2"/><duration-on-core>100000000</duration-on-core><event-count>6</event-count><event-count>102</event-count><size-in-bytes>1100</size-in-bytes><size-in-bytes>550</size-in-bytes><event-count>12</event-count><disk-size-in-bytes>125</disk-size-in-bytes><disk-size-in-bytes>220</disk-size-in-bytes></row>
<row><process ref="1"/><start-time>2000000000</start-time><pid ref="2"/><duration-on-core>150000000</duration-on-core><event-count>5</event-count><event-count>99</event-count><size-in-bytes>900</size-in-bytes><size-in-bytes>600</size-in-bytes><event-count>14</event-count><disk-size-in-bytes>150</disk-size-in-bytes><disk-size-in-bytes>260</disk-size-in-bytes></row>
</node></trace-query-result>"#;

    #[test]
    fn resolves_refs_and_calculates_steady_state_deltas() {
        let summary = summarize_activity_xml(XML).unwrap();
        let json = serde_json::to_value(summary).unwrap();

        assert_eq!(json["process"]["name"], "rasitop & helper");
        assert_eq!(json["process"]["pid"], 42);
        assert_eq!(json["measurement"]["samples"], 3);
        assert_eq!(json["measurement"]["duration_ns"], 2_000_000_000_u64);
        assert_eq!(json["cpu"]["time_delta_ns"], 100_000_000_u64);
        assert_eq!(json["cpu"]["average_percent"], 5.0);
        assert_eq!(json["idle_wakeups"]["delta"], 4);
        assert_eq!(json["idle_wakeups"]["per_second"], 2.0);
        assert_eq!(json["memory"]["physical_footprint_bytes"]["delta"], -100);
        assert_eq!(json["memory"]["physical_footprint_bytes"]["max"], 1100);
        assert_eq!(json["memory"]["private_bytes"]["delta"], 100);
        assert_eq!(json["threads"]["min"], 4);
        assert_eq!(json["threads"]["max"], 6);
        assert_eq!(json["ports"]["delta"], -1);
        assert_eq!(json["disk_io"]["bytes_written"]["delta"], 50);
        assert_eq!(json["disk_io"]["bytes_read"]["per_second"], 30.0);
    }

    #[test]
    fn rejects_decreasing_counters() {
        let xml = XML.replace(
            "<duration-on-core>150000000</duration-on-core>",
            "<duration-on-core>40000000</duration-on-core>",
        );
        let error = summarize_activity_xml(&xml).unwrap_err();
        assert!(error.to_string().contains("CPU time counter decreased"));
    }

    #[test]
    fn rejects_unknown_refs() {
        let xml = XML.replace("<pid ref=\"2\"/>", "<pid ref=\"missing\"/>");
        let error = summarize_activity_xml(&xml).unwrap_err();
        assert!(format!("{error:#}").contains("unknown id missing"));
    }
}
