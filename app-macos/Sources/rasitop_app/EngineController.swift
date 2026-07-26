import Foundation

enum EngineError: Error, CustomStringConvertible {
  case creationFailed(Int32)
  case samplingFailed(Int32)

  var description: String {
    switch self {
    case .creationFailed(let code):
      "CPU engine creation failed with status \(code)"
    case .samplingFailed(let code):
      "CPU engine sampling failed with status \(code)"
    }
  }
}

@MainActor
final class EngineController: NSObject {
  private weak var graphView: CPUStatusGraphView?
  private weak var statusMenuController: StatusMenuController?
  private var engine: OpaquePointer?
  private var timer: Timer?
  private var snapshot = rasitop_engine_snapshot()
  private var historyPoints = Array(
    repeating: rasitop_history_point(),
    count: Int(rasitop_history_capacity)
  )
  private var sensorTicksRemaining = 0
  private var sensorDetailsVisible = false

  init(
    graphView: CPUStatusGraphView,
    statusMenuController: StatusMenuController
  ) throws {
    self.graphView = graphView
    self.statusMenuController = statusMenuController
    super.init()

    var handle: OpaquePointer?
    let status = rasitop_engine_create(&handle)
    guard status == rasitop_ok, let handle else {
      throw EngineError.creationFailed(status)
    }
    engine = handle
  }

  deinit {
    timer?.invalidate()
    if let engine {
      let status = rasitop_engine_destroy(engine)
      if status != rasitop_ok {
        NSLog("rasitop engine destroy failed with status %d", status)
      }
    }
  }

  func start() {
    guard timer == nil else {
      return
    }

    let timer = Timer(
      timeInterval: 1.0,
      target: self,
      selector: #selector(sample),
      userInfo: nil,
      repeats: true
    )
    timer.tolerance = 0.25
    RunLoop.main.add(timer, forMode: .common)
    self.timer = timer
  }

  func stop() {
    timer?.invalidate()
    timer = nil
  }

  func prepareForSystemSleep() {
    stop()
  }

  func resumeAfterSystemWake() {
    guard let engine else {
      return
    }

    let status = rasitop_engine_reset_cpu_baselines(engine)
    if status != rasitop_ok {
      NSLog("rasitop CPU baseline reset failed with status %d", status)
    }
    sensorTicksRemaining = 0
    start()
  }

  func setSensorDetailsVisible(_ isVisible: Bool) {
    sensorDetailsVisible = isVisible
    if isVisible {
      sensorTicksRemaining = 0
    }
  }

  @objc
  private func sample() {
    guard let engine else {
      return
    }

    var requestFlags = UInt32(rasitop_request_per_core)
    if sensorTicksRemaining == 0 {
      requestFlags |= UInt32(rasitop_request_sensors)
      sensorTicksRemaining = sensorDetailsVisible ? 1 : 4
    } else {
      sensorTicksRemaining -= 1
    }

    let status = rasitop_engine_sample(
      engine,
      requestFlags,
      &snapshot
    )
    switch status {
    case rasitop_sample_ready:
      graphView?.update(from: &snapshot)
      if sensorDetailsVisible {
        historyPoints.withUnsafeMutableBufferPointer { buffer in
          let count = rasitop_engine_history(
            engine,
            buffer.baseAddress,
            buffer.count
          )
          let history = UnsafeBufferPointer(
            start: buffer.baseAddress,
            count: count
          )
          statusMenuController?.update(from: &snapshot, history: history)
        }
      } else {
        statusMenuController?.update(from: &snapshot, history: nil)
      }
    case rasitop_ok:
      break
    default:
      NSLog("%@", EngineError.samplingFailed(status).description)
    }
  }
}
