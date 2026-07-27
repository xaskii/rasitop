import AppKit

private struct SensorDisplaySnapshot {
  var cpuRatio = 0.0
  var hottestTemperature: Double?
  var averageTemperature: Double?
  var fanSpeed: Double?
  var systemPower: Double?
}

private struct CPUComponentDisplaySample {
  var user = 0.0
  var system = 0.0
  var nice = 0.0
}

private enum CPUComponentColors {
  static let user = NSColor.systemBlue
  static let system = NSColor.systemRed.withAlphaComponent(0.88)
  static let nice = NSColor.systemPurple.withAlphaComponent(0.72)
}

@MainActor
protocol SensorDetailsConsumer: AnyObject {
  func update(
    from snapshot: inout rasitop_engine_snapshot,
    history: UnsafeBufferPointer<rasitop_history_point>?
  )
}

@MainActor
final class StatusMenuController: NSObject, NSMenuDelegate, SensorDetailsConsumer {
  var visibilityDidChange: ((Bool) -> Void)?

  let menu = NSMenu()

  private let summaryPresenter = SensorSummaryPresenter()
  private var menuIsOpen = false

  override init() {
    super.init()

    menu.autoenablesItems = false
    menu.delegate = self

    let summaryItem = NSMenuItem()
    summaryItem.view = summaryPresenter.view
    menu.addItem(summaryItem)
    menu.addItem(.separator())

    let quitItem = NSMenuItem(
      title: "Quit",
      action: #selector(NSApplication.terminate(_:)),
      keyEquivalent: "q"
    )
    quitItem.keyEquivalentModifierMask = [.command]
    quitItem.target = NSApp
    menu.addItem(quitItem)

    summaryPresenter.resizeView()
  }

  var isShown: Bool {
    menuIsOpen
  }

  func close() {
    menu.cancelTracking()
  }

  func update(
    from snapshot: inout rasitop_engine_snapshot,
    history: UnsafeBufferPointer<rasitop_history_point>?
  ) {
    summaryPresenter.update(
      from: &snapshot,
      history: history,
      render: isShown
    )
  }

  func menuWillOpen(_ menu: NSMenu) {
    menuIsOpen = true
    summaryPresenter.render()
    visibilityDidChange?(true)
  }

  func menuDidClose(_ menu: NSMenu) {
    menuIsOpen = false
    visibilityDidChange?(false)
  }
}

@MainActor
private final class SensorSummaryPresenter {
  let view = SensorSummaryView()

  private var latestSnapshot = SensorDisplaySnapshot()
  private var historyPoints = Array(
    repeating: rasitop_history_point(),
    count: Int(rasitop_history_capacity)
  )
  private var historyCount = 0
  private var coreSamples = Array(
    repeating: CPUComponentDisplaySample(),
    count: Int(rasitop_max_logical_cpus)
  )
  private var coreCount = 0

  func update(
    from snapshot: inout rasitop_engine_snapshot,
    history: UnsafeBufferPointer<rasitop_history_point>?,
    render shouldRender: Bool
  ) {
    latestSnapshot = SensorDisplaySnapshot(
      cpuRatio: clamped(snapshot.aggregate.total_ratio),
      hottestTemperature: finite(snapshot.sensors.cpu_temp_max_c),
      averageTemperature: finite(snapshot.sensors.cpu_temp_avg_c),
      fanSpeed: finite(snapshot.sensors.fan_rpm),
      systemPower: finite(snapshot.sensors.system_power_w)
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
        coreSamples[index] = CPUComponentDisplaySample(
          user: clamped(usage.user_ratio),
          system: clamped(usage.system_ratio),
          nice: clamped(usage.nice_ratio)
        )
      }
    }
    if let history {
      historyCount = min(history.count, historyPoints.count)
      for index in 0..<historyCount {
        historyPoints[index] = history[index]
      }
    }
    if shouldRender {
      render()
    }
  }

  func render() {
    historyPoints.withUnsafeBufferPointer { historyBuffer in
      coreSamples.withUnsafeBufferPointer { coreBuffer in
        view.update(
          with: latestSnapshot,
          history: UnsafeBufferPointer(
            start: historyBuffer.baseAddress,
            count: historyCount
          ),
          cores: UnsafeBufferPointer(
            start: coreBuffer.baseAddress,
            count: coreCount
          )
        )
      }
    }
    resizeView()
  }

  func resizeView() {
    let size = NSSize(
      width: SensorSummaryView.width,
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

@MainActor
final class SensorSummaryPreviewWindowController: NSWindowController, SensorDetailsConsumer {
  private let summaryPresenter: SensorSummaryPresenter

  init() {
    let summaryPresenter = SensorSummaryPresenter()
    summaryPresenter.resizeView()
    self.summaryPresenter = summaryPresenter

    let summaryView = summaryPresenter.view
    let backgroundView = NSVisualEffectView(frame: summaryView.frame)
    backgroundView.material = .menu
    backgroundView.blendingMode = .behindWindow
    backgroundView.state = .active
    summaryView.autoresizingMask = [.width, .height]
    backgroundView.addSubview(summaryView)

    let window = NSWindow(
      contentRect: backgroundView.frame,
      styleMask: [.titled, .closable, .miniaturizable],
      backing: .buffered,
      defer: false
    )
    window.title = "rasitop UI Preview"
    window.identifier = NSUserInterfaceItemIdentifier("rasitop-ui-preview")
    window.isReleasedWhenClosed = false
    window.contentView = backgroundView
    window.center()

    super.init(window: window)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  func update(
    from snapshot: inout rasitop_engine_snapshot,
    history: UnsafeBufferPointer<rasitop_history_point>?
  ) {
    summaryPresenter.update(
      from: &snapshot,
      history: history,
      render: true
    )
    let size = summaryPresenter.view.frame.size
    if window?.contentView?.frame.size != size {
      window?.setContentSize(size)
    }
  }
}

@MainActor
private final class SensorSummaryView: NSView {
  static let width = 264.0

  private let historyView = CPUHistoryView()
  private let statsTable = StatsTableView()
  private let coreComponentsView = CPUCoreComponentsView()
  private var tableHeightConstraint: NSLayoutConstraint?

  var preferredHeight: CGFloat {
    181 + statsTable.preferredHeight
  }

  init() {
    super.init(frame: NSRect(x: 0, y: 0, width: Self.width, height: 252))

    let historyLabel = NSTextField(labelWithString: "HISTORY")
    historyLabel.font = .systemFont(ofSize: 9, weight: .semibold)
    historyLabel.textColor = .secondaryLabelColor

    let historyStack = NSStackView(views: [historyLabel, historyView])
    historyStack.orientation = .vertical
    historyStack.alignment = .width
    historyStack.spacing = 5

    let coresLabel = NSTextField(labelWithString: "CORES")
    coresLabel.font = .systemFont(ofSize: 9, weight: .semibold)
    coresLabel.textColor = .secondaryLabelColor

    let coresSpacer = NSView()
    coresSpacer.setContentHuggingPriority(.defaultLow, for: .horizontal)

    let legend = NSStackView(
      views: [
        Self.makeLegendItem(title: "User", color: CPUComponentColors.user),
        Self.makeLegendItem(title: "System", color: CPUComponentColors.system),
        Self.makeLegendItem(title: "Nice", color: CPUComponentColors.nice),
      ]
    )
    legend.orientation = .horizontal
    legend.alignment = .centerY
    legend.spacing = 7

    let coresHeader = NSStackView(views: [coresLabel, coresSpacer, legend])
    coresHeader.orientation = .horizontal
    coresHeader.alignment = .centerY
    coresHeader.spacing = 6

    let coresStack = NSStackView(views: [coresHeader, coreComponentsView])
    coresStack.orientation = .vertical
    coresStack.alignment = .width
    coresStack.spacing = 5

    let contentStack = NSStackView(
      views: [
        historyStack,
        statsTable,
        coresStack,
      ]
    )
    contentStack.translatesAutoresizingMaskIntoConstraints = false
    contentStack.orientation = .vertical
    contentStack.alignment = .width
    contentStack.distribution = .fill
    contentStack.spacing = 7
    addSubview(contentStack)

    let tableHeightConstraint = statsTable.heightAnchor.constraint(
      equalToConstant: statsTable.preferredHeight
    )
    tableHeightConstraint.priority = .defaultHigh
    self.tableHeightConstraint = tableHeightConstraint

    NSLayoutConstraint.activate([
      contentStack.topAnchor.constraint(equalTo: topAnchor, constant: 10),
      contentStack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
      contentStack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
      contentStack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
      historyStack.heightAnchor.constraint(equalToConstant: 81),
      historyStack.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
      historyLabel.widthAnchor.constraint(equalTo: historyStack.widthAnchor),
      historyView.heightAnchor.constraint(equalToConstant: 65),
      historyView.widthAnchor.constraint(equalTo: historyStack.widthAnchor),
      statsTable.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
      tableHeightConstraint,
      coresStack.heightAnchor.constraint(equalToConstant: 66),
      coresStack.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
      coresHeader.widthAnchor.constraint(equalTo: coresStack.widthAnchor),
      coreComponentsView.heightAnchor.constraint(equalToConstant: 50),
      coreComponentsView.widthAnchor.constraint(equalTo: coresStack.widthAnchor),
    ])
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  func update(
    with snapshot: SensorDisplaySnapshot,
    history: UnsafeBufferPointer<rasitop_history_point>,
    cores: UnsafeBufferPointer<CPUComponentDisplaySample>
  ) {
    historyView.update(history)
    statsTable.update(with: snapshot)
    coreComponentsView.update(cores)
    let tableHeight = statsTable.preferredHeight
    if tableHeightConstraint?.constant != tableHeight {
      tableHeightConstraint?.constant = tableHeight
    }
  }

  private static func makeLegendItem(
    title: String,
    color: NSColor
  ) -> NSStackView {
    let swatch = NSView()
    swatch.translatesAutoresizingMaskIntoConstraints = false
    swatch.wantsLayer = true
    swatch.layer?.backgroundColor = color.cgColor
    swatch.layer?.cornerRadius = 2

    let label = NSTextField(labelWithString: title)
    label.font = .systemFont(ofSize: 9)
    label.textColor = .secondaryLabelColor

    let item = NSStackView(views: [swatch, label])
    item.orientation = .horizontal
    item.alignment = .centerY
    item.spacing = 3

    NSLayoutConstraint.activate([
      swatch.widthAnchor.constraint(equalToConstant: 6),
      swatch.heightAnchor.constraint(equalToConstant: 6),
    ])
    return item
  }
}

@MainActor
private final class CPUHistoryView: NSView {
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

@MainActor
private final class CPUCoreComponentsView: NSView {
  private let barGap = 2.0
  private let barRadius = 2.5
  private let idleLayer = CAShapeLayer()
  private let componentsLayer = CALayer()
  private let componentsMaskLayer = CAShapeLayer()
  private let userLayer = CAShapeLayer()
  private let systemLayer = CAShapeLayer()
  private let niceLayer = CAShapeLayer()
  private var samples = Array(
    repeating: CPUComponentDisplaySample(),
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

  func update(_ cores: UnsafeBufferPointer<CPUComponentDisplaySample>) {
    sampleCount = min(cores.count, samples.count)
    var busyTotal = 0.0
    for index in 0..<sampleCount {
      samples[index] = cores[index]
      busyTotal += min(
        cores[index].user + cores[index].system + cores[index].nice,
        1
      )
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
      let user = clamped(sample.user)
      let system = min(clamped(sample.system), 1 - user)
      let nice = min(clamped(sample.nice), 1 - user - system)
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

@MainActor
private final class StatsTableView: NSView {
  private let total = StatsRow(title: "Total utilization")
  private let hottestTemperature = StatsRow(title: "Hottest CPU")
  private let averageTemperature = StatsRow(title: "Average CPU")
  private let fanSpeed = StatsRow(title: "Fan")
  private let systemPower = StatsRow(title: "System power")
  private let rows: [StatsRow]

  var preferredHeight: CGFloat {
    CGFloat(rows.count { !$0.isHidden } * 22)
  }

  init() {
    rows = [total, hottestTemperature, averageTemperature, fanSpeed, systemPower]
    super.init(frame: .zero)

    let stack = NSStackView(views: rows)
    stack.translatesAutoresizingMaskIntoConstraints = false
    stack.orientation = .vertical
    stack.alignment = .width
    stack.distribution = .fill
    stack.spacing = 0
    addSubview(stack)

    for row in rows {
      row.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
    }

    NSLayoutConstraint.activate([
      stack.topAnchor.constraint(equalTo: topAnchor),
      stack.leadingAnchor.constraint(equalTo: leadingAnchor),
      stack.trailingAnchor.constraint(equalTo: trailingAnchor),
      stack.bottomAnchor.constraint(equalTo: bottomAnchor),
    ])
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  func update(with snapshot: SensorDisplaySnapshot) {
    total.update(String(format: "%.0f%%", snapshot.cpuRatio * 100))
    hottestTemperature.update(
      snapshot.hottestTemperature.map { String(format: "%.0f °C", $0) }
    )
    averageTemperature.update(
      snapshot.averageTemperature.map { String(format: "%.1f °C", $0) }
    )
    fanSpeed.update(
      snapshot.fanSpeed.map { String(format: "%.0f RPM", $0) }
    )
    systemPower.update(
      snapshot.systemPower.map { String(format: "%.1f W", $0) }
    )
  }
}

@MainActor
private final class StatsRow: NSView {
  private let titleLabel: NSTextField
  private let valueLabel = NSTextField(labelWithString: "")
  private let separatorLayer = CALayer()

  override var intrinsicContentSize: NSSize {
    NSSize(width: NSView.noIntrinsicMetric, height: 22)
  }

  init(title: String) {
    titleLabel = NSTextField(labelWithString: title)
    super.init(frame: .zero)

    titleLabel.font = .systemFont(ofSize: 11)
    titleLabel.textColor = .secondaryLabelColor

    valueLabel.font = .monospacedDigitSystemFont(ofSize: 11, weight: .medium)
    valueLabel.alignment = .right

    wantsLayer = true
    layer?.addSublayer(separatorLayer)
    addSubview(titleLabel)
    addSubview(valueLabel)
    updateSeparatorColor()
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  override func layout() {
    super.layout()
    let valueWidth = 92.0
    titleLabel.frame = NSRect(
      x: 0,
      y: 3,
      width: max(bounds.width - valueWidth - 8, 0),
      height: 16
    )
    valueLabel.frame = NSRect(
      x: max(bounds.width - valueWidth, 0),
      y: 3,
      width: valueWidth,
      height: 16
    )
    separatorLayer.frame = NSRect(
      x: 0,
      y: 0,
      width: bounds.width,
      height: 1 / backingScale
    )
  }

  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    updateSeparatorColor()
  }

  func update(_ value: String?) {
    let stringValue = value ?? ""
    if valueLabel.stringValue != stringValue {
      valueLabel.stringValue = stringValue
    }
    let shouldHide = value == nil
    if isHidden != shouldHide {
      isHidden = shouldHide
    }
  }

  private var backingScale: CGFloat {
    max(window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 1, 1)
  }

  private func updateSeparatorColor() {
    separatorLayer.backgroundColor = NSColor.separatorColor.withAlphaComponent(0.45).cgColor
  }
}
