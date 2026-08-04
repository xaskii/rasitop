import AppKit

@MainActor
final class SensorSummaryPreviewWindowController: NSWindowController, SensorDetailsConsumer {
  private let summaryPresenter: SensorSummaryPresenter

  init() {
    let summaryPresenter = SensorSummaryPresenter(viewWidth: 320)
    summaryPresenter.resizeView()
    self.summaryPresenter = summaryPresenter

    let summaryView = summaryPresenter.view
    let backgroundView = NSVisualEffectView(frame: summaryView.frame)
    backgroundView.material = .menu
    backgroundView.blendingMode = .behindWindow
    backgroundView.state = .active
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
    history: UnsafeBufferPointer<rasitop_history_point>?,
    gpuHistory: UnsafeBufferPointer<rasitop_history_point>?
  ) {
    summaryPresenter.update(
      from: &snapshot,
      history: history,
      gpuHistory: gpuHistory,
      render: true
    )
    let size = summaryPresenter.view.frame.size
    if window?.contentView?.frame.size != size {
      window?.setContentSize(size)
    }
    summaryPresenter.view.frame = NSRect(origin: .zero, size: size)
  }
}
