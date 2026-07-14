import AppKit

@MainActor
final class CPUStatusGraphView: NSView {
  private struct CoreUsage {
    var user = 0.0
    var system = 0.0
    var nice = 0.0
  }

  static let graphSize = NSSize(width: 76, height: 18)

  private let coreGap = 0.5
  private let borderColor = NSColor.white.cgColor
  private let busyColor = NSColor.systemBlue.cgColor
  private var coreCount = 0
  private var cores = Array(
    repeating: CoreUsage(),
    count: Int(rasitop_max_logical_cpus)
  )

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    setAccessibilityElement(true)
    setAccessibilityRole(.image)
    setAccessibilityLabel("CPU utilization per logical core")
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    nil
  }

  func update(from snapshot: inout rasitop_engine_snapshot) {
    let count = min(
      Int(snapshot.per_core_count),
      Int(rasitop_max_logical_cpus)
    )

    withUnsafePointer(to: &snapshot) { snapshotPointer in
      for index in 0..<count {
        guard
          let sample = rasitop_snapshot_core(
            snapshotPointer,
            UInt32(index)
          )?.pointee.usage
        else {
          continue
        }
        cores[index] = CoreUsage(
          user: sample.user_ratio,
          system: sample.system_ratio,
          nice: sample.nice_ratio
        )
      }
    }

    coreCount = count
    needsDisplay = true
  }

  override func draw(_ dirtyRect: NSRect) {
    guard let context = NSGraphicsContext.current?.cgContext else {
      return
    }

    let scale =
      window?.backingScaleFactor
      ?? NSScreen.main?.backingScaleFactor
      ?? 1
    let borderWidth = 1 / scale
    let borderOffset = borderWidth / 2
    let content = bounds.insetBy(dx: borderOffset, dy: borderOffset)

    if coreCount > 0 {
      let barWidth = max(
        borderWidth,
        (content.width / CGFloat(coreCount)) - coreGap
      )

      drawBars(
        context: context,
        content: content,
        barWidth: barWidth
      )
    }

    context.addPath(
      Self.makeBorderPath(bounds: bounds, offset: borderOffset)
    )
    context.setStrokeColor(borderColor)
    context.setLineWidth(borderWidth)
    context.strokePath()
  }

  private func drawBars(
    context: CGContext,
    content: CGRect,
    barWidth: CGFloat
  ) {
    context.setFillColor(busyColor)

    for index in 0..<coreCount {
      let usage = cores[index]
      let totalBusy = usage.user + usage.system + usage.nice
      let height = clamped(totalBusy) * content.height
      guard height > 0 else {
        continue
      }

      let x = content.minX + CGFloat(index) * (barWidth + coreGap)
      context.fill(
        CGRect(
          x: x,
          y: content.minY,
          width: barWidth,
          height: height
        )
      )
    }
  }

  private func clamped(_ ratio: Double) -> CGFloat {
    CGFloat(min(max(ratio, 0), 1))
  }

  private static func makeBorderPath(
    bounds: NSRect,
    offset: CGFloat
  ) -> CGPath {
    let rect = bounds.insetBy(dx: offset, dy: offset)
    return CGPath(
      roundedRect: rect,
      cornerWidth: 3,
      cornerHeight: 3,
      transform: nil
    )
  }
}
