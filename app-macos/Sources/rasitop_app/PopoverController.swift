import AppKit

private struct SensorDisplaySnapshot {
  var cpuRatio = 0.0
  var hottestTemperature: Double?
  var averageTemperature: Double?
  var fanSpeed: Double?
  var systemPower: Double?
}

@MainActor
final class PopoverController: NSObject, NSPopoverDelegate {
  var visibilityDidChange: ((Bool) -> Void)?

  private weak var anchorButton: NSStatusBarButton?
  private var popover: NSPopover?
  private var summaryView: SensorSummaryView?
  private var latestSnapshot = SensorDisplaySnapshot()
  private var historyPoints = Array(
    repeating: rasitop_history_point(),
    count: Int(rasitop_history_capacity)
  )
  private var historyCount = 0

  var isShown: Bool {
    popover?.isShown == true
  }

  func toggle(relativeTo button: NSStatusBarButton) {
    if isShown {
      close()
    } else {
      show(relativeTo: button)
    }
  }

  func close() {
    popover?.performClose(nil)
  }

  func update(
    from snapshot: inout rasitop_engine_snapshot,
    history: UnsafeBufferPointer<rasitop_history_point>?
  ) {
    latestSnapshot = SensorDisplaySnapshot(
      cpuRatio: clamped(snapshot.aggregate.total_ratio),
      hottestTemperature: finite(snapshot.sensors.cpu_temp_max_c),
      averageTemperature: finite(snapshot.sensors.cpu_temp_avg_c),
      fanSpeed: finite(snapshot.sensors.fan_rpm),
      systemPower: finite(snapshot.sensors.system_power_w)
    )
    if let history {
      historyCount = min(history.count, historyPoints.count)
      for index in 0..<historyCount {
        historyPoints[index] = history[index]
      }
    }
    if isShown {
      updateVisibleContent()
    }
  }

  func popoverDidClose(_ notification: Notification) {
    anchorButton?.highlight(false)
    anchorButton = nil
    visibilityDidChange?(false)
  }

  private func show(relativeTo button: NSStatusBarButton) {
    let popover = self.popover ?? makePopover()
    updateVisibleContent()
    anchorButton = button
    button.highlight(true)
    popover.show(
      relativeTo: button.bounds,
      of: button,
      preferredEdge: .minY
    )
    visibilityDidChange?(true)
  }

  private func makePopover() -> NSPopover {
    let summaryView = SensorSummaryView()
    self.summaryView = summaryView

    let viewController = NSViewController()
    viewController.view = summaryView

    let popover = NSPopover()
    popover.behavior = .transient
    popover.animates = !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
    popover.delegate = self
    popover.contentViewController = viewController
    self.popover = popover

    updateVisibleContent()
    return popover
  }

  private func updateVisibleContent() {
    guard let summaryView, let popover else {
      return
    }

    historyPoints.withUnsafeBufferPointer { buffer in
      summaryView.update(
        with: latestSnapshot,
        history: UnsafeBufferPointer(
          start: buffer.baseAddress,
          count: historyCount
        )
      )
    }
    let contentSize = NSSize(
      width: SensorSummaryView.width,
      height: summaryView.preferredHeight
    )
    popover.contentSize = contentSize
  }

  private func finite(_ value: Double) -> Double? {
    value.isFinite ? value : nil
  }

  private func clamped(_ ratio: Double) -> Double {
    min(max(ratio, 0), 1)
  }
}

@MainActor
private final class SensorSummaryView: NSView {
  static let width = 264.0

  private let historyView = CPUHistoryView()
  private let statsTable = StatsTableView()
  private var tableHeightConstraint: NSLayoutConstraint?

  var preferredHeight: CGFloat {
    142 + statsTable.preferredHeight
  }

  init() {
    super.init(frame: NSRect(x: 0, y: 0, width: Self.width, height: 252))

    let iconView = NSImageView()
    iconView.image = NSImage(
      systemSymbolName: "chart.bar.fill",
      accessibilityDescription: nil
    )
    iconView.contentTintColor = .secondaryLabelColor

    let titleLabel = NSTextField(labelWithString: "CPU")
    titleLabel.font = .systemFont(ofSize: 13, weight: .semibold)

    let headerSpacer = NSView()
    headerSpacer.setContentHuggingPriority(.defaultLow, for: .horizontal)

    let quitButton = NSButton(
      title: "Quit",
      target: NSApp,
      action: #selector(NSApplication.terminate(_:))
    )
    quitButton.isBordered = false
    quitButton.focusRingType = .none
    quitButton.font = .systemFont(ofSize: 11)
    quitButton.toolTip = "Quit rasitop"
    quitButton.setAccessibilityLabel("Quit rasitop")

    let headerStack = NSStackView(
      views: [iconView, titleLabel, headerSpacer, quitButton]
    )
    headerStack.orientation = .horizontal
    headerStack.alignment = .centerY
    headerStack.spacing = 6

    let historyLabel = NSTextField(labelWithString: "CPU HISTORY")
    historyLabel.font = .systemFont(ofSize: 9, weight: .semibold)
    historyLabel.textColor = .secondaryLabelColor

    let historyStack = NSStackView(views: [historyLabel, historyView])
    historyStack.orientation = .vertical
    historyStack.alignment = .width
    historyStack.spacing = 5

    let contentStack = NSStackView(
      views: [
        headerStack,
        historyStack,
        statsTable,
      ]
    )
    contentStack.translatesAutoresizingMaskIntoConstraints = false
    contentStack.orientation = .vertical
    contentStack.alignment = .width
    contentStack.distribution = .fill
    contentStack.spacing = 10
    addSubview(contentStack)

    let tableHeightConstraint = statsTable.heightAnchor.constraint(
      equalToConstant: statsTable.preferredHeight
    )
    tableHeightConstraint.priority = .defaultHigh
    self.tableHeightConstraint = tableHeightConstraint

    NSLayoutConstraint.activate([
      contentStack.topAnchor.constraint(equalTo: topAnchor, constant: 12),
      contentStack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 14),
      contentStack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
      contentStack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -12),
      iconView.widthAnchor.constraint(equalToConstant: 14),
      iconView.heightAnchor.constraint(equalToConstant: 14),
      headerStack.heightAnchor.constraint(equalToConstant: 18),
      headerStack.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
      historyStack.heightAnchor.constraint(equalToConstant: 81),
      historyStack.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
      historyLabel.widthAnchor.constraint(equalTo: historyStack.widthAnchor),
      historyView.heightAnchor.constraint(equalToConstant: 65),
      historyView.widthAnchor.constraint(equalTo: historyStack.widthAnchor),
      statsTable.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
      tableHeightConstraint,
    ])
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) is not supported")
  }

  func update(
    with snapshot: SensorDisplaySnapshot,
    history: UnsafeBufferPointer<rasitop_history_point>
  ) {
    historyView.update(history)
    statsTable.update(with: snapshot)
    tableHeightConstraint?.constant = statsTable.preferredHeight
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
      setAccessibilityValue(
        String(format: "Current utilization %.0f percent", values[valueCount - 1] * 100)
      )
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
    valueLabel.stringValue = value ?? ""
    isHidden = value == nil
  }

  private var backingScale: CGFloat {
    max(window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 1, 1)
  }

  private func updateSeparatorColor() {
    separatorLayer.backgroundColor = NSColor.separatorColor.withAlphaComponent(0.45).cgColor
  }
}
