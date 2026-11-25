// Lokyr's actions in Shortcuts and Spotlight, of the system's App Intents:
// their declarations alone, each of which hands the app a command through
// the app's function `lokyr_intent_command`, of C's calling convention.

import AppIntents

/// Hands the running app the command `command`, as its menus do: 1 where
/// an app ran to take it.
@_silgen_name("lokyr_intent_command")
private func lokyrIntentCommand(_ command: UnsafePointer<CChar>) -> Int32

/// Runs the app's command `command`.
@MainActor
private func run(_ command: String) -> Bool {
    command.withCString { lokyrIntentCommand($0) } == 1
}

/// Opens a new window of the default shell, as the File menu's New Window.
struct NewWindowIntent: AppIntent {
    static let title: LocalizedStringResource = "New Window"
    static let description = IntentDescription("Opens a new Lokyr window.")
    static let openAppWhenRun: Bool = true

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = run("window.new")
        return .result()
    }
}
