import AppKit

@MainActor
final class CPUStatusGraphView: NSView {
  static let graphSize = NSSize(width: 76, height: 18)

  private let coreGap = 0.5
  private let barsLayer = CAShapeLayer()
  private let borderLayer = CAShapeLayer()
  private var coreCount = 0
  private var busyRatios = Array(
    repeating: 0.0,
    count: Int(rasitop_max_logical_cpus)
  )

  override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    setAccessibilityElement(true)
    setAccessibilityRole(.image)
    setAccessibilityLabel("CPU utilization per logical core")

    wantsLayer = true
    let rootLayer = CALayer()
    layer = rootLayer

    barsLayer.fillColor = NSColor.systemBlue.cgColor
    borderLayer.fillColor = nil
    borderLayer.strokeColor = NSColor.white.cgColor
    rootLayer.addSublayer(barsLayer)
    rootLayer.addSublayer(borderLayer)
    updateLayerGeometry()
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func hitTest(_ point: NSPoint) -> NSView? {
    nil
  }

  override func layout() {
    super.layout()
    updateLayerGeometry()
  }

  override func viewDidChangeBackingProperties() {
    super.viewDidChangeBackingProperties()
    updateLayerGeometry()
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
        busyRatios[index] =
          sample.user_ratio
          + sample.system_ratio
          + sample.nice_ratio
      }
    }

    coreCount = count
    updateBarsPath()
  }

  private var backingScale: CGFloat {
    let scale =
      window?.backingScaleFactor
      ?? NSScreen.main?.backingScaleFactor
      ?? 1
    return max(scale, 1)
  }

  private func updateLayerGeometry() {
    let scale = backingScale
    let borderWidth = 1 / scale
    let borderOffset = borderWidth / 2

    CATransaction.begin()
    CATransaction.setDisableActions(true)
    layer?.contentsScale = scale
    barsLayer.contentsScale = scale
    barsLayer.frame = bounds
    borderLayer.contentsScale = scale
    borderLayer.frame = bounds
    borderLayer.lineWidth = borderWidth
    borderLayer.path = Self.makeBorderPath(
      bounds: bounds,
      offset: borderOffset
    )
    updateBarsPath(scale: scale)
    CATransaction.commit()
  }

  private func updateBarsPath(scale: CGFloat? = nil) {
    guard coreCount > 0 else {
      barsLayer.path = nil
      return
    }

    let borderWidth = 1 / (scale ?? backingScale)
    let borderOffset = borderWidth / 2
    let content = bounds.insetBy(dx: borderOffset, dy: borderOffset)
    let barWidth = max(
      borderWidth,
      (content.width / CGFloat(coreCount)) - coreGap
    )
    let path = CGMutablePath()

    for index in 0..<coreCount {
      let height = clamped(busyRatios[index]) * content.height
      guard height > 0 else {
        continue
      }

      let x = content.minX + CGFloat(index) * (barWidth + coreGap)
      path.addRect(
        CGRect(
          x: x,
          y: content.minY,
          width: barWidth,
          height: height
        )
      )
    }

    CATransaction.begin()
    CATransaction.setDisableActions(true)
    barsLayer.path = path
    CATransaction.commit()
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
