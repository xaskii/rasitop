import AppKit

@MainActor
final class GPUHistoryView: NSView {
  private let gridLayer = CAShapeLayer()
  private let fillLayer = CAShapeLayer()
  private let lineLayer = CAShapeLayer()
  private var values = Array(
    repeating: Double.nan,
    count: Int(rasitop_gpu_history_capacity)
  )
  private var valueCount = 0

  init() {
    super.init(frame: .zero)
    setAccessibilityElement(true)
    setAccessibilityRole(.image)
    setAccessibilityLabel("GPU usage history")
    wantsLayer = true
    layer?.cornerRadius = 6
    layer?.masksToBounds = true
    gridLayer.fillColor = nil
    gridLayer.lineWidth = 1
    gridLayer.lineDashPattern = [2, 3]
    fillLayer.strokeColor = nil
    lineLayer.fillColor = nil
    lineLayer.lineWidth = 1.25
    lineLayer.lineJoin = .round
    layer?.addSublayer(gridLayer)
    layer?.addSublayer(fillLayer)
    layer?.addSublayer(lineLayer)
    updateColors()
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    updatePaths()
  }

  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    updateColors()
  }

  func update(_ history: UnsafeBufferPointer<rasitop_history_point>) {
    valueCount = min(history.count, values.count)
    for index in 0..<valueCount {
      let ratio = history[index].total_ratio
      values[index] = ratio.isFinite ? min(max(ratio, 0), 1) : .nan
    }
    if let current = values.prefix(valueCount).last(where: { $0.isFinite }) {
      setAccessibilityValue(
        String(format: "Current utilization %.0f percent", current * 100)
      )
    } else {
      setAccessibilityValue("GPU utilization unavailable")
    }
    updatePaths()
  }

  private func updatePaths() {
    let content = bounds.insetBy(dx: 6, dy: 5)
    guard content.width > 0, content.height > 0 else { return }

    let gridPath = CGMutablePath()
    gridPath.move(to: CGPoint(x: content.minX, y: content.midY))
    gridPath.addLine(to: CGPoint(x: content.maxX, y: content.midY))
    let linePath = CGMutablePath()
    let fillPath = CGMutablePath()
    let step = content.width / CGFloat(max(values.count - 1, 1))
    let firstX = content.maxX - CGFloat(max(valueCount - 1, 0)) * step
    var runStart: CGPoint?
    var runLast: CGPoint?

    for index in 0..<valueCount {
      let value = values[index]
      guard value.isFinite else {
        closeFillRun(fillPath, start: runStart, last: runLast, baseline: content.minY)
        runStart = nil
        runLast = nil
        continue
      }
      let point = CGPoint(
        x: firstX + CGFloat(index) * step,
        y: content.minY + CGFloat(value) * content.height
      )
      if runStart == nil {
        runStart = point
        linePath.move(to: point)
        fillPath.move(to: CGPoint(x: point.x, y: content.minY))
        fillPath.addLine(to: point)
      } else {
        linePath.addLine(to: point)
        fillPath.addLine(to: point)
      }
      runLast = point
    }
    closeFillRun(fillPath, start: runStart, last: runLast, baseline: content.minY)

    CATransaction.begin()
    CATransaction.setDisableActions(true)
    gridLayer.frame = bounds
    gridLayer.path = gridPath
    fillLayer.frame = bounds
    fillLayer.path = fillPath
    lineLayer.frame = bounds
    lineLayer.path = linePath
    CATransaction.commit()
  }

  private func closeFillRun(
    _ path: CGMutablePath,
    start: CGPoint?,
    last: CGPoint?,
    baseline: CGFloat
  ) {
    guard start != nil, let last else { return }
    path.addLine(to: CGPoint(x: last.x, y: baseline))
    path.closeSubpath()
  }

  private func updateColors() {
    layer?.backgroundColor = NSColor.separatorColor.withAlphaComponent(0.08).cgColor
    gridLayer.strokeColor = NSColor.separatorColor.withAlphaComponent(0.28).cgColor
    fillLayer.fillColor = NSColor.systemGreen.withAlphaComponent(0.15).cgColor
    lineLayer.strokeColor = NSColor.systemGreen.cgColor
  }
}
