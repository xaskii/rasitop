import AppKit

@MainActor
protocol SensorDetailsConsumer: AnyObject {
  func update(
    from snapshot: inout rasitop_engine_snapshot,
    history: UnsafeBufferPointer<rasitop_history_point>?,
    gpuHistory: UnsafeBufferPointer<rasitop_history_point>?
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
    history: UnsafeBufferPointer<rasitop_history_point>?,
    gpuHistory: UnsafeBufferPointer<rasitop_history_point>?
  ) {
    summaryPresenter.update(
      from: &snapshot,
      history: history,
      gpuHistory: gpuHistory,
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
