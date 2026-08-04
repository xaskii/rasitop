import AppKit

enum CPUComponentColors {
  static let user = NSColor.systemBlue
  static let system = NSColor.systemRed.withAlphaComponent(0.88)
  static let nice = NSColor.systemPurple.withAlphaComponent(0.72)
}

@MainActor
final class CPUCoreComponentsView: NSView {
  private let barGap = 2.0
  private let barRadius = 2.5
  private let idleLayer = CAShapeLayer()
  private let componentsLayer = CALayer()
  private let componentsMaskLayer = CAShapeLayer()
  private let userLayer = CAShapeLayer()
  private let systemLayer = CAShapeLayer()
  private let niceLayer = CAShapeLayer()
  private var samples = Array(
    repeating: rasitop_cpu_sample(),
    count: Int(rasitop_max_logical_cpus)
  )
  private var sampleCount = 0
  private var accessibilityCoreCount: Int?
  private var accessibilityPercent: Int?

  init() {
    super.init(frame: .zero)

    setAccessibilityElement(true)
    setAccessibilityRole(.image)
    setAccessibilityLabel("CPU components per logical core")

    wantsLayer = true
    layer?.cornerRadius = 7
    layer?.masksToBounds = true
    layer?.addSublayer(idleLayer)
    layer?.addSublayer(componentsLayer)
    componentsLayer.mask = componentsMaskLayer
    componentsLayer.addSublayer(userLayer)
    componentsLayer.addSublayer(systemLayer)
    componentsLayer.addSublayer(niceLayer)
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

  func update(_ cores: UnsafeBufferPointer<rasitop_cpu_sample>) {
    sampleCount = min(cores.count, samples.count)
    var busyTotal = 0.0
    for index in 0..<sampleCount {
      samples[index] = cores[index]
      busyTotal += Double(clamped(cores[index].total_ratio))
    }
    if sampleCount > 0 {
      let percent = Int(
        (busyTotal / Double(sampleCount) * 100).rounded()
      )
      if accessibilityCoreCount != sampleCount || accessibilityPercent != percent {
        accessibilityCoreCount = sampleCount
        accessibilityPercent = percent
        setAccessibilityValue(
          String(
            format: "%d logical cores, %d percent average utilization",
            sampleCount,
            percent
          )
        )
      }
    }
    updatePaths()
  }

  private func updatePaths() {
    let content = bounds.insetBy(dx: 6, dy: 5)

    CATransaction.begin()
    CATransaction.setDisableActions(true)
    idleLayer.frame = bounds
    componentsLayer.frame = bounds
    componentsMaskLayer.frame = bounds
    userLayer.frame = bounds
    systemLayer.frame = bounds
    niceLayer.frame = bounds

    guard sampleCount > 0, content.width > 0, content.height > 0 else {
      idleLayer.path = nil
      componentsMaskLayer.path = nil
      userLayer.path = nil
      systemLayer.path = nil
      niceLayer.path = nil
      CATransaction.commit()
      return
    }

    let scale = backingScale
    let contentMinXPixel = Int((content.minX * scale).rounded())
    let contentMaxXPixel = Int((content.maxX * scale).rounded())
    let contentMinYPixel = Int((content.minY * scale).rounded())
    let contentMaxYPixel = Int((content.maxY * scale).rounded())
    let contentWidthPixels = contentMaxXPixel - contentMinXPixel
    let contentHeightPixels = contentMaxYPixel - contentMinYPixel
    let gapPixels = max(Int((barGap * scale).rounded()), 1)
    let availableWidthPixels =
      contentWidthPixels - gapPixels * max(sampleCount - 1, 0)

    guard
      availableWidthPixels >= sampleCount,
      contentHeightPixels > 0
    else {
      idleLayer.path = nil
      componentsMaskLayer.path = nil
      userLayer.path = nil
      systemLayer.path = nil
      niceLayer.path = nil
      CATransaction.commit()
      return
    }

    let baseBarWidthPixels = availableWidthPixels / sampleCount
    let extraWidthPixels = availableWidthPixels % sampleCount
    let idlePath = CGMutablePath()
    let userPath = CGMutablePath()
    let systemPath = CGMutablePath()
    let nicePath = CGMutablePath()
    var xPixel = contentMinXPixel

    for index in 0..<sampleCount {
      let barWidthPixels =
        baseBarWidthPixels + (index < extraWidthPixels ? 1 : 0)
      let x = CGFloat(xPixel) / scale
      let barWidth = CGFloat(barWidthPixels) / scale
      let trackRect = CGRect(
        x: x,
        y: CGFloat(contentMinYPixel) / scale,
        width: barWidth,
        height: CGFloat(contentHeightPixels) / scale
      )
      let radius = min(
        barRadius,
        trackRect.width / 2,
        trackRect.height / 2
      )
      idlePath.addPath(
        CGPath(
          roundedRect: trackRect,
          cornerWidth: radius,
          cornerHeight: radius,
          transform: nil
        )
      )

      let sample = samples[index]
      let user = clamped(sample.user_ratio)
      let system = min(clamped(sample.system_ratio), 1 - user)
      let nice = min(clamped(sample.nice_ratio), 1 - user - system)
      let userEndPixel = min(
        Int((user * CGFloat(contentHeightPixels)).rounded()),
        contentHeightPixels
      )
      let systemEndPixel = min(
        Int(((user + system) * CGFloat(contentHeightPixels)).rounded()),
        contentHeightPixels
      )
      let niceEndPixel = min(
        Int(((user + system + nice) * CGFloat(contentHeightPixels)).rounded()),
        contentHeightPixels
      )

      addSegment(
        fromPixel: 0,
        toPixel: userEndPixel,
        contentMinYPixel: contentMinYPixel,
        scale: scale,
        x: x,
        width: barWidth,
        path: userPath
      )
      addSegment(
        fromPixel: userEndPixel,
        toPixel: systemEndPixel,
        contentMinYPixel: contentMinYPixel,
        scale: scale,
        x: x,
        width: barWidth,
        path: systemPath
      )
      addSegment(
        fromPixel: systemEndPixel,
        toPixel: niceEndPixel,
        contentMinYPixel: contentMinYPixel,
        scale: scale,
        x: x,
        width: barWidth,
        path: nicePath
      )
      xPixel += barWidthPixels + gapPixels
    }

    idleLayer.path = idlePath
    componentsMaskLayer.path = idlePath
    userLayer.path = userPath
    systemLayer.path = systemPath
    niceLayer.path = nicePath
    CATransaction.commit()
  }

  private func addSegment(
    fromPixel: Int,
    toPixel: Int,
    contentMinYPixel: Int,
    scale: CGFloat,
    x: CGFloat,
    width: CGFloat,
    path: CGMutablePath
  ) {
    guard toPixel > fromPixel else {
      return
    }
    let rect = CGRect(
      x: x,
      y: CGFloat(contentMinYPixel + fromPixel) / scale,
      width: width,
      height: CGFloat(toPixel - fromPixel) / scale
    )
    path.addRect(rect)
  }

  private var backingScale: CGFloat {
    max(window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 1, 1)
  }

  private func clamped(_ ratio: Double) -> CGFloat {
    ratio.isFinite ? CGFloat(min(max(ratio, 0), 1)) : 0
  }

  private func updateColors() {
    layer?.backgroundColor = NSColor.separatorColor.withAlphaComponent(0.06).cgColor
    idleLayer.fillColor = NSColor.separatorColor.withAlphaComponent(0.10).cgColor
    componentsMaskLayer.fillColor = NSColor.black.cgColor
    userLayer.fillColor = CPUComponentColors.user.cgColor
    systemLayer.fillColor = CPUComponentColors.system.cgColor
    niceLayer.fillColor = CPUComponentColors.nice.cgColor
  }
}
