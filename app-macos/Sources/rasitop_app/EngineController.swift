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
  private var engine: OpaquePointer?
  private var timer: Timer?
  private var snapshot = rasitop_engine_snapshot()
  private var sensorTicksRemaining = 0

  init(graphView: CPUStatusGraphView) throws {
    self.graphView = graphView
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

  @objc
  private func sample() {
    guard let engine else {
      return
    }

    var requestFlags = UInt32(rasitop_request_per_core)
    if sensorTicksRemaining == 0 {
      requestFlags |= UInt32(rasitop_request_sensors)
      sensorTicksRemaining = 4
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
    case rasitop_ok:
      break
    default:
      NSLog("%@", EngineError.samplingFailed(status).description)
    }
  }
}
