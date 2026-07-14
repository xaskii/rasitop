import AppKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
  private let statusItemWidth = 80.0
  private var statusItem: NSStatusItem?
  private var engineController: EngineController?

  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApp.setActivationPolicy(.accessory)

    let item = NSStatusBar.system.statusItem(
      withLength: statusItemWidth
    )
    guard let button = item.button else {
      NSLog("rasitop could not create an NSStatusItem button")
      NSApp.terminate(nil)
      return
    }

    button.title = ""
    button.setAccessibilityLabel("CPU utilization per logical core")

    let graphSize = CPUStatusGraphView.graphSize
    let graphFrame = NSRect(
      x: (button.bounds.width - graphSize.width) / 2,
      y: (button.bounds.height - graphSize.height) / 2,
      width: graphSize.width,
      height: graphSize.height
    )
    let graphView = CPUStatusGraphView(frame: graphFrame)
    graphView.autoresizingMask = [
      .minXMargin,
      .maxXMargin,
      .minYMargin,
      .maxYMargin,
    ]
    button.addSubview(graphView)

    let menu = NSMenu()
    let quitItem = NSMenuItem(
      title: "Quit rasitop",
      action: #selector(NSApplication.terminate(_:)),
      keyEquivalent: "q"
    )
    quitItem.target = NSApp
    menu.addItem(quitItem)
    item.menu = menu
    statusItem = item

    do {
      let controller = try EngineController(graphView: graphView)
      controller.start()
      engineController = controller
    } catch {
      NSLog("rasitop startup failed: %@", String(describing: error))
    }
  }

  func applicationWillTerminate(_ notification: Notification) {
    engineController?.stop()
    engineController = nil
    statusItem = nil
  }
}
