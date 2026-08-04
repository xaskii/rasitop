import AppKit

@MainActor
final class SensorSummaryView: NSView {
  static let width = 264.0

  private let historyView = CPUHistoryView()
  private let cpuValueLabel = NSTextField(labelWithString: "—")
  private let gpuHistoryView = GPUHistoryView()
  private let gpuValueLabel = NSTextField(labelWithString: "—")
  private let gpuStack = NSStackView()
  private let statsTable = StatsTableView()
  private let coreComponentsView = CPUCoreComponentsView()
  private var tableHeightConstraint: NSLayoutConstraint?

  var preferredHeight: CGFloat {
    181 + statsTable.preferredHeight + (gpuStack.isHidden ? 0 : 64)
  }

  init() {
    super.init(frame: NSRect(x: 0, y: 0, width: Self.width, height: 252))

    let historyLabel = NSTextField(labelWithString: "CPU")
    historyLabel.font = .systemFont(ofSize: 9, weight: .semibold)
    historyLabel.textColor = .secondaryLabelColor

    let historySpacer = NSView()
    historySpacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
    cpuValueLabel.font = .monospacedDigitSystemFont(ofSize: 10, weight: .medium)
    cpuValueLabel.textColor = .secondaryLabelColor
    let historyHeader = NSStackView(
      views: [historyLabel, historySpacer, cpuValueLabel]
    )
    historyHeader.orientation = .horizontal
    historyHeader.alignment = .centerY

    let historyStack = NSStackView(views: [historyHeader, historyView])
    historyStack.orientation = .vertical
    historyStack.alignment = .width
    historyStack.spacing = 5

    let gpuLabel = NSTextField(labelWithString: "GPU")
    gpuLabel.font = .systemFont(ofSize: 9, weight: .semibold)
    gpuLabel.textColor = .secondaryLabelColor
    let gpuSpacer = NSView()
    gpuSpacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
    gpuValueLabel.font = .monospacedDigitSystemFont(ofSize: 10, weight: .medium)
    gpuValueLabel.textColor = .secondaryLabelColor
    let gpuHeader = NSStackView(views: [gpuLabel, gpuSpacer, gpuValueLabel])
    gpuHeader.orientation = .horizontal
    gpuHeader.alignment = .centerY
    gpuStack.setViews([gpuHeader, gpuHistoryView], in: .top)
    gpuStack.orientation = .vertical
    gpuStack.alignment = .width
    gpuStack.spacing = 5
    gpuStack.isHidden = true

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
        gpuStack,
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
      historyHeader.widthAnchor.constraint(equalTo: historyStack.widthAnchor),
      historyView.heightAnchor.constraint(equalToConstant: 65),
      historyView.widthAnchor.constraint(equalTo: historyStack.widthAnchor),
      gpuStack.heightAnchor.constraint(equalToConstant: 57),
      gpuStack.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
      gpuHeader.widthAnchor.constraint(equalTo: gpuStack.widthAnchor),
      gpuHistoryView.heightAnchor.constraint(equalToConstant: 41),
      gpuHistoryView.widthAnchor.constraint(equalTo: gpuStack.widthAnchor),
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
    gpuHistory: UnsafeBufferPointer<rasitop_history_point>,
    cores: UnsafeBufferPointer<rasitop_cpu_sample>
  ) {
    historyView.update(history)
    cpuValueLabel.stringValue = String(
      format: "%.0f%%",
      snapshot.cpuRatio * 100
    )
    gpuStack.isHidden = !snapshot.gpuSupported
    if snapshot.gpuSupported {
      gpuHistoryView.update(gpuHistory)
      gpuValueLabel.stringValue =
        snapshot.gpuRatio.map {
          String(format: "%.0f%%", $0 * 100)
        } ?? "—"
    }
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
