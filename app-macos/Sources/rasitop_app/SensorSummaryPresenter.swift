import AppKit

struct SensorDisplaySnapshot {
  var cpuRatio = 0.0
  var hottestTemperature: Double?
  var averageTemperature: Double?
  var fanSpeed: Double?
  var systemPower: Double?
  var gpuRatio: Double?
  var gpuSupported = false
}

@MainActor
final class SensorSummaryPresenter {
  let view = SensorSummaryView()
  private let viewWidth: CGFloat

  private var latestSnapshot = SensorDisplaySnapshot()
  private var historyPoints = Array(
    repeating: rasitop_history_point(),
    count: Int(rasitop_history_capacity)
  )
  private var historyCount = 0
  private var gpuHistoryPoints = Array(
    repeating: rasitop_history_point(),
    count: Int(rasitop_gpu_history_capacity)
  )
  private var gpuHistoryCount = 0
  private var coreSamples = Array(
    repeating: rasitop_cpu_sample(),
    count: Int(rasitop_max_logical_cpus)
  )
  private var coreCount = 0

  init(viewWidth: CGFloat = SensorSummaryView.width) {
    self.viewWidth = viewWidth
  }

  func update(
    from snapshot: inout rasitop_engine_snapshot,
    history: UnsafeBufferPointer<rasitop_history_point>?,
    gpuHistory: UnsafeBufferPointer<rasitop_history_point>?,
    render shouldRender: Bool
  ) {
    latestSnapshot = SensorDisplaySnapshot(
      cpuRatio: clamped(snapshot.aggregate.total_ratio),
      hottestTemperature: finite(snapshot.sensors.cpu_temp_max_c),
      averageTemperature: finite(snapshot.sensors.cpu_temp_avg_c),
      fanSpeed: finite(snapshot.sensors.fan_rpm),
      systemPower: finite(snapshot.sensors.system_power_w),
      gpuRatio: finite(snapshot.gpu.busy_ratio),
      gpuSupported:
        snapshot.gpu.capability_flags
        & UInt64(rasitop_gpu_capability_utilization) != 0
    )
    coreCount = min(
      Int(snapshot.per_core_count),
      coreSamples.count
    )
    withUnsafePointer(to: &snapshot) { snapshotPointer in
      for index in 0..<coreCount {
        guard
          let usage = rasitop_snapshot_core(
            snapshotPointer,
            UInt32(index)
          )?.pointee.usage
        else {
          continue
        }
        coreSamples[index] = usage
      }
    }
    if let history {
      historyCount = min(history.count, historyPoints.count)
      for index in 0..<historyCount {
        historyPoints[index] = history[index]
      }
    }
    if let gpuHistory {
      gpuHistoryCount = min(gpuHistory.count, gpuHistoryPoints.count)
      for index in 0..<gpuHistoryCount {
        gpuHistoryPoints[index] = gpuHistory[index]
      }
    }
    if shouldRender {
      render()
    }
  }

  func render() {
    historyPoints.withUnsafeBufferPointer { historyBuffer in
      gpuHistoryPoints.withUnsafeBufferPointer { gpuHistoryBuffer in
        coreSamples.withUnsafeBufferPointer { coreBuffer in
          view.update(
            with: latestSnapshot,
            history: UnsafeBufferPointer(
              start: historyBuffer.baseAddress,
              count: historyCount
            ),
            gpuHistory: UnsafeBufferPointer(
              start: gpuHistoryBuffer.baseAddress,
              count: gpuHistoryCount
            ),
            cores: UnsafeBufferPointer(
              start: coreBuffer.baseAddress,
              count: coreCount
            )
          )
        }
      }
    }
    resizeView()
  }

  func resizeView() {
    let size = NSSize(
      width: viewWidth,
      height: view.preferredHeight
    )
    if view.frame.size != size {
      view.setFrameSize(size)
    }
  }

  private func finite(_ value: Double) -> Double? {
    value.isFinite ? value : nil
  }

  private func clamped(_ ratio: Double) -> Double {
    ratio.isFinite ? min(max(ratio, 0), 1) : 0
  }
}
