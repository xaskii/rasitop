import AppKit

@_cdecl("rasitop_app_main")
public func rasitopAppMain() {
  MainActor.assumeIsolated {
    let application = NSApplication.shared
    let delegate = AppDelegate()
    application.delegate = delegate
    application.run()
  }
}
