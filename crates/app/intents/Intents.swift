// Lokyr's actions in Shortcuts and Spotlight, of the system's App Intents:
// their declarations alone, each of which makes a request of the app's
// automation through the app's function `lokyr_intent_request`, of C's
// calling convention, and waits for its answer.

import AppIntents

/// The function that takes an answer: the context that the request gave,
/// the status, 0 of an answer or the error's code, and the answer's text or
/// the error's message.
typealias LokyrAnswer = @convention(c) (UnsafeMutableRawPointer?, Int32, UnsafePointer<CChar>?) -> Void

/// Makes the automation's request of `kind`, of two ids and two texts, whose
/// answer comes once through `answer`: whether the app took it.
@_silgen_name("lokyr_intent_request")
private func lokyrIntentRequest(
    _ kind: Int32,
    _ first: UInt64,
    _ second: UInt64,
    _ text: UnsafePointer<CChar>?,
    _ more: UnsafePointer<CChar>?,
    _ context: UnsafeMutableRawPointer?,
    _ answer: LokyrAnswer
) -> Int32

/// The kinds of the requests, as the app's function takes them.
private enum Kind: Int32 {
    case newWindow = 1, newTab, split, sendText, run, paneText, workspace
}

/// An error that the app answered, of its message.
struct LokyrError: Error, CustomLocalizedStringResourceConvertible {
    let message: String
    var localizedStringResource: LocalizedStringResource { "\(message)" }
}

/// The continuation that an answer resumes, carried through the app.
private final class Waiting {
    let continuation: CheckedContinuation<String, Error>
    init(_ continuation: CheckedContinuation<String, Error>) { self.continuation = continuation }
}

/// Calls `body` of `text`'s C string, or of none.
private func withText<T>(_ text: String?, _ body: (UnsafePointer<CChar>?) -> T) -> T {
    guard let text else { return body(nil) }
    return text.withCString { body($0) }
}

/// The app's answer to the request of `kind`, its text, or its error.
@MainActor
private func ask(
    _ kind: Kind, first: UInt64 = 0, second: UInt64 = 0, text: String? = nil, more: String? = nil
) async throws -> String {
    try await withCheckedThrowingContinuation { continuation in
        let context = Unmanaged.passRetained(Waiting(continuation)).toOpaque()
        _ = withText(text) { text in
            withText(more) { more in
                lokyrIntentRequest(kind.rawValue, first, second, text, more, context) { context, status, text in
                    guard let context else { return }
                    let waiting = Unmanaged<Waiting>.fromOpaque(context).takeRetainedValue()
                    let message = text.map { String(cString: $0) } ?? ""
                    if status == 0 {
                        waiting.continuation.resume(returning: message)
                    } else {
                        waiting.continuation.resume(throwing: LokyrError(message: message))
                    }
                }
            }
        }
    }
}

/// A terminal of Lokyr's, a pane, by the app's id.
struct TerminalEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Terminal"
    static let defaultQuery = TerminalQuery()

    let id: String
    let title: String
    let folder: String

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(title)", subtitle: "\(folder)")
    }

    /// The app's id of the pane.
    var pane: UInt64 { UInt64(id) ?? 0 }
}

/// Lokyr's terminals, as the app lists them.
struct TerminalQuery: EntityQuery {
    @MainActor
    func entities(for identifiers: [String]) async throws -> [TerminalEntity] {
        try await terminals().filter { identifiers.contains($0.id) }
    }

    @MainActor
    func suggestedEntities() async throws -> [TerminalEntity] {
        try await terminals()
    }
}

/// Every terminal, of the app's answer: a line each of its id, its title and
/// its folder, apart by tabs.
@MainActor
private func terminals() async throws -> [TerminalEntity] {
    let answer = try await ask(.workspace)
    return answer.split(separator: "\n").compactMap { line in
        let fields = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        guard fields.count == 3 else { return nil }
        return TerminalEntity(id: fields[0], title: fields[1], folder: fields[2])
    }
}

/// Where a split's new terminal goes.
enum SplitDirection: String, AppEnum {
    case right, left, down, up

    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Direction"
    static let caseDisplayRepresentations: [SplitDirection: DisplayRepresentation] = [
        .right: "Right", .left: "Left", .down: "Down", .up: "Up",
    ]

    /// The app's code of the edge.
    var code: UInt64 {
        switch self {
        case .right: 0
        case .left: 1
        case .down: 2
        case .up: 3
        }
    }
}

/// Opens a window, in a folder and running a command line if given.
struct NewWindowIntent: AppIntent {
    static let title: LocalizedStringResource = "New Window"
    static let description = IntentDescription("Opens a new Lokyr window, in a folder and running a command line if given.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Folder") var folder: String?
    @Parameter(title: "Command") var command: String?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.newWindow, text: folder, more: command)
        return .result()
    }
}

/// Opens a tab in the window in front.
struct NewTabIntent: AppIntent {
    static let title: LocalizedStringResource = "New Tab"
    static let description = IntentDescription("Opens a tab in Lokyr's window in front, in a folder and running a command line if given.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Folder") var folder: String?
    @Parameter(title: "Command") var command: String?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.newTab, text: folder, more: command)
        return .result()
    }
}

/// Splits a terminal, the focused one unless another is given.
struct SplitPaneIntent: AppIntent {
    static let title: LocalizedStringResource = "Split Pane"
    static let description = IntentDescription("Splits a terminal of Lokyr's, the focused one unless another is given, of a new terminal beside it.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Terminal") var terminal: TerminalEntity?
    @Parameter(title: "Direction", default: .right) var direction: SplitDirection
    @Parameter(title: "Folder") var folder: String?
    @Parameter(title: "Command") var command: String?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.split, first: terminal?.pane ?? 0, second: direction.code, text: folder, more: command)
        return .result()
    }
}

/// Sends text to a terminal, as if typed.
struct SendTextIntent: AppIntent {
    static let title: LocalizedStringResource = "Send Text"
    static let description = IntentDescription("Sends text to a terminal of Lokyr's, the focused one unless another is given, as if typed.")
    static let openAppWhenRun: Bool = false

    @Parameter(title: "Text") var text: String
    @Parameter(title: "Terminal") var terminal: TerminalEntity?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.sendText, first: terminal?.pane ?? 0, text: text)
        return .result()
    }
}

/// Runs one of Lokyr's commands by its name.
struct RunCommandIntent: AppIntent {
    static let title: LocalizedStringResource = "Run Command"
    static let description = IntentDescription("Runs one of Lokyr's commands by its name, as the command palette runs it, such as tabs.next.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Command") var command: String

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.run, text: command)
        return .result()
    }
}

/// Switches to a session of a name, opening it first if it is not there.
struct SwitchSessionIntent: AppIntent {
    static let title: LocalizedStringResource = "Switch Session"
    static let description = IntentDescription("Switches Lokyr to a session of a name, which it opens first if there is none.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Session") var name: String

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.run, text: "window.switch-session", more: "name=\(name)")
        return .result()
    }
}

/// Opens a saved layout as a new session.
struct OpenLayoutIntent: AppIntent {
    static let title: LocalizedStringResource = "Open Layout"
    static let description = IntentDescription("Opens one of Lokyr's saved layouts, by its name, as a new session.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Layout") var name: String

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.run, text: "window.open-layout", more: "name=\(name)")
        return .result()
    }
}

/// The text that a terminal's screen shows.
struct GetTerminalTextIntent: AppIntent {
    static let title: LocalizedStringResource = "Get Terminal Text"
    static let description = IntentDescription("The text that a terminal of Lokyr's shows, the focused one unless another is given. Lokyr asks you first whether Shortcuts may read it.")
    static let openAppWhenRun: Bool = false

    @Parameter(title: "Terminal") var terminal: TerminalEntity?

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        .result(value: try await ask(.paneText, first: terminal?.pane ?? 0))
    }
}

/// The text of a terminal's selection.
struct GetSelectedTextIntent: AppIntent {
    static let title: LocalizedStringResource = "Get Selected Text"
    static let description = IntentDescription("The text that is selected in a terminal of Lokyr's, the focused one unless another is given. Lokyr asks you first whether Shortcuts may read it.")
    static let openAppWhenRun: Bool = false

    @Parameter(title: "Terminal") var terminal: TerminalEntity?

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        .result(value: try await ask(.paneText, first: terminal?.pane ?? 0, second: 1))
    }
}

/// The phrases of Lokyr's actions in Spotlight and Siri.
struct LokyrShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: NewWindowIntent(),
            phrases: ["New \(.applicationName) window", "Open a \(.applicationName) window"],
            shortTitle: "New Window",
            systemImageName: "macwindow"
        )
        AppShortcut(
            intent: NewTabIntent(),
            phrases: ["New \(.applicationName) tab"],
            shortTitle: "New Tab",
            systemImageName: "plus.square.on.square"
        )
    }
}
