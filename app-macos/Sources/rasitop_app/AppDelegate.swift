import AppKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
  private let statusItemWidth = 80.0
  private var statusItem: NSStatusItem?
  private var engineController: EngineController?
  private var profilingTerminationTimer: Timer?

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
      observeSystemSleepAndWake()
    } catch {
      NSLog("rasitop startup failed: %@", String(describing: error))
    }

    scheduleProfilingTerminationIfRequested()
  }

  func applicationWillTerminate(_ notification: Notification) {
    NSWorkspace.shared.notificationCenter.removeObserver(self)
    profilingTerminationTimer?.invalidate()
    profilingTerminationTimer = nil
    engineController?.stop()
    engineController = nil
    statusItem = nil
  }

  private func observeSystemSleepAndWake() {
    let center = NSWorkspace.shared.notificationCenter
    center.addObserver(
      self,
      selector: #selector(systemWillSleep),
      name: NSWorkspace.willSleepNotification,
      object: nil
    )
    center.addObserver(
      self,
      selector: #selector(systemDidWake),
      name: NSWorkspace.didWakeNotification,
      object: nil
    )
  }

  @objc
  private func systemWillSleep(_ notification: Notification) {
    engineController?.prepareForSystemSleep()
  }

  @objc
  private func systemDidWake(_ notification: Notification) {
    engineController?.resumeAfterSystemWake()
  }

  private func scheduleProfilingTerminationIfRequested() {
    let argument = "--profile-duration-seconds"
    let arguments = CommandLine.arguments
    guard let argumentIndex = arguments.firstIndex(of: argument) else {
      return
    }

    let valueIndex = arguments.index(after: argumentIndex)
    guard
      valueIndex < arguments.endIndex,
      let duration = TimeInterval(arguments[valueIndex]),
      duration > 0
    else {
      NSLog("rasitop requires a positive number after %@", argument)
      NSApp.terminate(nil)
      return
    }

    let timer = Timer(
      timeInterval: duration,
      target: self,
      selector: #selector(finishTimedProfile),
      userInfo: nil,
      repeats: false
    )
    RunLoop.main.add(timer, forMode: .common)
    profilingTerminationTimer = timer
  }

  @objc
  private func finishTimedProfile() {
    NSApp.terminate(nil)
  }
}
