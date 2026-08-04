import AppKit

@MainActor
final class CPUHistoryView: NSView {
  private let gridLayer = CAShapeLayer()
  private let fillLayer = CAShapeLayer()
  private let lineLayer = CAShapeLayer()
  private var values = Array(
    repeating: 0.0,
    count: Int(rasitop_history_capacity)
  )
  private var valueCount = 0
  private var accessibilityPercent: Int?

  init() {
    super.init(frame: .zero)

    setAccessibilityElement(true)
    setAccessibilityRole(.image)
    setAccessibilityLabel("CPU usage history")

    wantsLayer = true
    layer?.cornerRadius = 7
    layer?.masksToBounds = true
    gridLayer.fillColor = nil
    gridLayer.lineWidth = 1
    gridLayer.lineDashPattern = [2, 3]
    fillLayer.strokeColor = nil
    lineLayer.fillColor = nil
    lineLayer.lineWidth = 1.5
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
      values[index] = min(max(history[index].total_ratio, 0), 1)
    }
    if valueCount > 0 {
      let percent = Int((values[valueCount - 1] * 100).rounded())
      if accessibilityPercent != percent {
        accessibilityPercent = percent
        setAccessibilityValue(
          String(format: "Current utilization %d percent", percent)
        )
      }
    }
    updatePaths()
  }

  private func updatePaths() {
    let content = bounds.insetBy(dx: 6, dy: 6)
    guard content.width > 0, content.height > 0 else {
      return
    }

    let gridPath = CGMutablePath()
    gridPath.move(to: CGPoint(x: content.minX, y: content.midY))
    gridPath.addLine(to: CGPoint(x: content.maxX, y: content.midY))

    guard valueCount > 0 else {
      CATransaction.begin()
      CATransaction.setDisableActions(true)
      gridLayer.frame = bounds
      gridLayer.path = gridPath
      fillLayer.path = nil
      lineLayer.path = nil
      CATransaction.commit()
      return
    }

    let step = content.width / CGFloat(max(values.count - 1, 1))
    let firstX = content.maxX - CGFloat(valueCount - 1) * step
    let linePath = CGMutablePath()
    let fillPath = CGMutablePath()

    for index in 0..<valueCount {
      let point = CGPoint(
        x: firstX + CGFloat(index) * step,
        y: content.minY + CGFloat(values[index]) * content.height
      )
      if index == 0 {
        linePath.move(to: point)
        fillPath.move(to: CGPoint(x: point.x, y: content.minY))
        fillPath.addLine(to: point)
      } else {
        linePath.addLine(to: point)
        fillPath.addLine(to: point)
      }
    }
    fillPath.addLine(to: CGPoint(x: content.maxX, y: content.minY))
    fillPath.closeSubpath()

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

  private func updateColors() {
    layer?.backgroundColor = NSColor.separatorColor.withAlphaComponent(0.08).cgColor
    gridLayer.strokeColor = NSColor.separatorColor.withAlphaComponent(0.28).cgColor
    fillLayer.fillColor = NSColor.systemBlue.withAlphaComponent(0.16).cgColor
    lineLayer.strokeColor = NSColor.systemBlue.cgColor
  }
}
