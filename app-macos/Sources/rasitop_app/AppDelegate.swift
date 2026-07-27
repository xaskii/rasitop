import AppKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
  private let statusItemWidth = 80.0
  private var statusItem: NSStatusItem?
  private var engineController: EngineController?
  private var statusMenuController: StatusMenuController?
  private var previewWindowController: SensorSummaryPreviewWindowController?
  private var profilingTerminationTimer: Timer?

  func applicationDidFinishLaunching(_ notification: Notification) {
    if CommandLine.arguments.contains("--ui-preview") {
      showUIPreview()
      scheduleProfilingTerminationIfRequested()
      return
    }

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

    let statusMenuController = StatusMenuController()
    self.statusMenuController = statusMenuController
    statusItem = item
    item.menu = statusMenuController.menu

    do {
      let controller = try EngineController(
        graphView: graphView,
        sensorDetailsConsumer: statusMenuController
      )
      statusMenuController.visibilityDidChange = { [weak controller] isVisible in
        controller?.setSensorDetailsVisible(isVisible)
      }
      controller.start()
      engineController = controller
      observeSystemSleepAndWake()
      openProfilingMenuIfRequested()
    } catch {
      NSLog("rasitop startup failed: %@", String(describing: error))
    }

    scheduleProfilingTerminationIfRequested()
  }

  func applicationShouldHandleReopen(
    _ sender: NSApplication,
    hasVisibleWindows flag: Bool
  ) -> Bool {
    if let previewWindowController {
      previewWindowController.showWindow(nil)
      NSApp.activate(ignoringOtherApps: true)
      return false
    }

    openStatusMenu()
    return false
  }

  func applicationWillTerminate(_ notification: Notification) {
    NSWorkspace.shared.notificationCenter.removeObserver(self)
    profilingTerminationTimer?.invalidate()
    profilingTerminationTimer = nil
    previewWindowController?.close()
    previewWindowController = nil
    engineController?.stop()
    engineController = nil
    statusMenuController?.close()
    statusItem?.menu = nil
    statusMenuController = nil
    statusItem = nil
  }

  private func showUIPreview() {
    NSApp.setActivationPolicy(.regular)
    let previewController = SensorSummaryPreviewWindowController()
    previewWindowController = previewController
    previewController.showWindow(nil)
    NSApp.activate(ignoringOtherApps: true)

    do {
      let controller = try EngineController(
        graphView: nil,
        sensorDetailsConsumer: previewController
      )
      controller.setSensorDetailsVisible(true)
      controller.start()
      engineController = controller
      observeSystemSleepAndWake()
    } catch {
      NSLog("rasitop preview startup failed: %@", String(describing: error))
    }
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

  private func openProfilingMenuIfRequested() {
    guard CommandLine.arguments.contains("--profile-open-popover") else {
      return
    }
    openStatusMenu()
  }

  private func openStatusMenu() {
    guard statusMenuController?.isShown == false else {
      return
    }
    DispatchQueue.main.async { [weak self] in
      guard let self, statusMenuController?.isShown == false else {
        return
      }
      statusItem?.button?.performClick(nil)
    }
  }

  @objc
  private func finishTimedProfile() {
    NSApp.terminate(nil)
  }
}
