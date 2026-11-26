// Lokyr's actions in Shortcuts and Spotlight, of the system's App Intents:
// their declarations alone, each of which makes a request of the app's
// automation through the app's function `lokyr_intent_request`, of C's
// calling convention, and waits for its answer.

import AppIntents

/// The function that takes an answer: the context that the request gave,
/// the status, 0 of an answer or the error's code, and the answer's text or
/// the error's message.
typealias LokyrAnswer = @convention(c) (UnsafeMutableRawPointer?, Int32, UnsafePointer<CChar>?) -> Void

/// Makes the automation's request of `kind`, of two ids, two texts and the
/// environment's variables, a line each, whose answer comes once through
/// `answer`: whether the app took it.
@_silgen_name("lokyr_intent_request")
private func lokyrIntentRequest(
    _ kind: Int32,
    _ first: UInt64,
    _ second: UInt64,
    _ text: UnsafePointer<CChar>?,
    _ more: UnsafePointer<CChar>?,
    _ variables: UnsafePointer<CChar>?,
    _ context: UnsafeMutableRawPointer?,
    _ answer: LokyrAnswer
) -> Int32

/// The kinds of the requests, as the app's function takes them.
private enum Kind: Int32 {
    case newWindow = 1, newTab, split, sendText, run, paneText, workspace, switchSession, close, focus,
        commands, sendKey, runLine
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
    _ kind: Kind, first: UInt64 = 0, second: UInt64 = 0, text: String? = nil, more: String? = nil,
    variables: [String]? = nil
) async throws -> String {
    try await withCheckedThrowingContinuation { continuation in
        let context = Unmanaged.passRetained(Waiting(continuation)).toOpaque()
        let lines = variables.map { $0.joined(separator: "\n") }
        _ = withText(text) { text in
            withText(more) { more in
              withText(lines) { lines in
                lokyrIntentRequest(kind.rawValue, first, second, text, more, lines, context) { context, status, text in
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
}

/// A terminal of Lokyr's, a pane, by the app's id, of the properties that
/// Find Terminal filters on and Get Details of Terminal gives.
struct TerminalEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Terminal"
    static let defaultQuery = TerminalQuery()

    let id: String

    @Property(title: "Title")
    var title: String

    @Property(title: "Folder")
    var folder: String

    @Property(title: "Tab ID")
    var tab: Int

    @Property(title: "Window ID")
    var window: Int

    init(id: String, title: String, folder: String, tab: Int, window: Int) {
        self.id = id
        self.title = title
        self.folder = folder
        self.tab = tab
        self.window = window
    }

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(title)", subtitle: "\(folder)")
    }

    /// The app's id of the pane.
    var pane: UInt64 { UInt64(id) ?? 0 }
}

/// Lokyr's terminals, as the app lists them, every one of them, so that
/// Shortcuts finds them by their properties.
struct TerminalQuery: EnumerableEntityQuery {
    @MainActor
    func entities(for identifiers: [String]) async throws -> [TerminalEntity] {
        try await terminals().filter { identifiers.contains($0.id) }
    }

    @MainActor
    func suggestedEntities() async throws -> [TerminalEntity] {
        try await terminals()
    }

    @MainActor
    func allEntities() async throws -> [TerminalEntity] {
        try await terminals()
    }
}

/// Every terminal, of the app's answer: a line each of its id, its title,
/// its folder, its tab's id and its window's, apart by tabs.
@MainActor
private func terminals() async throws -> [TerminalEntity] {
    let answer = try await ask(.workspace)
    return answer.split(separator: "\n").compactMap { line in
        let fields = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        guard fields.count == 5 else { return nil }
        return TerminalEntity(
            id: fields[0], title: fields[1], folder: fields[2],
            tab: Int(fields[3]) ?? 0, window: Int(fields[4]) ?? 0)
    }
}

/// An entry of Lokyr's command palette, of its title: by its id, the
/// command's name, then its entry's arguments, if any, as a URL's query,
/// as `splits.split?direction=right` of Split Right.
struct CommandEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Lokyr Command"
    static let defaultQuery = CommandQuery()

    let id: String
    let title: String

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(title)", subtitle: "\(call)")
    }

    /// The command's name and its arguments, as `splits.split direction=right`.
    var call: String {
        let parts = id.split(separator: "?", maxSplits: 1).map(String.init)
        guard parts.count == 2 else { return id }
        let pairs = parts[1].split(separator: "&").map { pair in
            String(pair).removingPercentEncoding ?? String(pair)
        }
        return ([parts[0]] + pairs).joined(separator: " ")
    }
}

/// The command palette's entries, as the app lists them, found by their
/// titles and their names as you type.
struct CommandQuery: EntityStringQuery {
    @MainActor
    func entities(for identifiers: [String]) async throws -> [CommandEntity] {
        try await commands().filter { identifiers.contains($0.id) }
    }

    @MainActor
    func entities(matching string: String) async throws -> [CommandEntity] {
        try await commands().filter {
            $0.title.localizedCaseInsensitiveContains(string) || $0.id.localizedCaseInsensitiveContains(string)
        }
    }

    @MainActor
    func suggestedEntities() async throws -> [CommandEntity] {
        try await commands()
    }
}

/// Every entry of the command palette, of the app's answer: a line each of
/// its id and its title, apart by a tab.
@MainActor
private func commands() async throws -> [CommandEntity] {
    let answer = try await ask(.commands)
    return answer.split(separator: "\n").compactMap { line in
        let fields = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        guard fields.count == 2 else { return nil }
        return CommandEntity(id: fields[0], title: fields[1])
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
    static let description = IntentDescription("Opens a new Lokyr window, in a folder and running a command line if given, its shell taking environment variables, each NAME=value.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Folder") var folder: String?
    @Parameter(title: "Command") var command: String?
    @Parameter(title: "Environment Variables") var environment: [String]?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.newWindow, text: folder, more: command, variables: environment)
        return .result()
    }
}

/// Opens a tab in the window in front.
struct NewTabIntent: AppIntent {
    static let title: LocalizedStringResource = "New Tab"
    static let description = IntentDescription("Opens a tab in Lokyr's window in front, in a folder and running a command line if given, its shell taking environment variables, each NAME=value.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Folder") var folder: String?
    @Parameter(title: "Command") var command: String?
    @Parameter(title: "Environment Variables") var environment: [String]?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.newTab, text: folder, more: command, variables: environment)
        return .result()
    }
}

/// Splits a terminal, the focused one unless another is given.
struct SplitPaneIntent: AppIntent {
    static let title: LocalizedStringResource = "Split Pane"
    static let description = IntentDescription("Splits a terminal of Lokyr's, the focused one unless another is given, of a new terminal beside it, its shell taking environment variables, each NAME=value.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Terminal") var terminal: TerminalEntity?
    @Parameter(title: "Direction", default: .right) var direction: SplitDirection
    @Parameter(title: "Folder") var folder: String?
    @Parameter(title: "Command") var command: String?
    @Parameter(title: "Environment Variables") var environment: [String]?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(
            .split, first: terminal?.pane ?? 0, second: direction.code, text: folder, more: command,
            variables: environment)
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

/// Runs a command line in a terminal's shell, at its prompt.
struct RunCommandLineIntent: AppIntent {
    static let title: LocalizedStringResource = "Run Command Line"
    static let description = IntentDescription("Runs a command line, such as ls -la, in the shell of a terminal of Lokyr's, the focused one unless another is given: Lokyr pastes it at the prompt and presses Return, while the shell waits at an empty prompt, and refuses otherwise, such as while a program runs.")
    static let openAppWhenRun: Bool = false

    @Parameter(title: "Command Line") var line: String
    @Parameter(title: "Terminal") var terminal: TerminalEntity?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.runLine, first: terminal?.pane ?? 0, text: line)
        return .result()
    }
}

/// A modifier held with a key.
enum KeyModifier: String, AppEnum {
    case control, option, shift, command

    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Modifier"
    static let caseDisplayRepresentations: [KeyModifier: DisplayRepresentation] = [
        .control: "Control", .option: "Option", .shift: "Shift", .command: "Command",
    ]

    /// The modifier as a binding writes it.
    var binding: String {
        switch self {
        case .control: "ctrl"
        case .option: "alt"
        case .shift: "shift"
        case .command: "cmd"
        }
    }
}

/// Sends a key to a terminal, as the keyboard sends it.
struct SendKeyboardEventIntent: AppIntent {
    static let title: LocalizedStringResource = "Send Keyboard Event"
    static let description = IntentDescription("Presses a key in a terminal of Lokyr's, the focused one unless another is given, which takes the focus, as you press it in Lokyr: a key of a shortcut runs its command, such as a with Command to select all; another goes to the program as the keyboard sends it, such as c with Control to interrupt it. The key is a character, or a name such as up, enter, tab, escape or f5.")
    static let openAppWhenRun: Bool = false

    @Parameter(title: "Key") var key: String
    @Parameter(title: "Modifiers") var modifiers: [KeyModifier]?
    @Parameter(title: "Terminal") var terminal: TerminalEntity?

    @MainActor
    func perform() async throws -> some IntentResult {
        let held = (modifiers ?? []).map(\.binding)
        let keys = (held + [key.trimmingCharacters(in: .whitespaces).lowercased()]).joined(separator: "+")
        _ = try await ask(.sendKey, first: terminal?.pane ?? 0, text: keys)
        return .result()
    }
}

/// Runs one of Lokyr's commands by its name.
struct RunCommandIntent: AppIntent {
    static let title: LocalizedStringResource = "Run Command"
    static let description = IntentDescription("Runs one of Lokyr's commands by its name, as the command palette runs it, such as tabs.next, with its arguments, a line each such as index=2.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Command") var command: String
    @Parameter(title: "Arguments", inputOptions: String.IntentInputOptions(capitalizationType: .none, multiline: true, autocorrect: false))
    var arguments: String?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.run, text: command, more: arguments)
        return .result()
    }
}

/// Runs an entry of the command palette, chosen from its list, of the
/// arguments that the entry gives, as Split Right.
struct RunPaletteCommandIntent: AppIntent {
    static let title: LocalizedStringResource = "Run Palette Command"
    static let description = IntentDescription("Runs one of Lokyr's commands, chosen by its title as the command palette lists it, such as Split Right, with the arguments of its entry. Arguments, a line each such as index=2, add to them.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Command") var command: CommandEntity
    @Parameter(title: "Arguments", inputOptions: String.IntentInputOptions(capitalizationType: .none, multiline: true, autocorrect: false))
    var arguments: String?

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.run, text: command.id, more: arguments)
        return .result()
    }
}

/// Closes a terminal.
struct CloseTerminalIntent: AppIntent {
    static let title: LocalizedStringResource = "Close Terminal"
    static let description = IntentDescription("Closes a terminal of Lokyr's. Lokyr asks first when a program other than a shell runs in it.")
    static let openAppWhenRun: Bool = false

    @Parameter(title: "Terminal") var terminal: TerminalEntity

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.close, first: terminal.pane)
        return .result()
    }
}

/// Brings a terminal forward.
struct FocusTerminalIntent: AppIntent {
    static let title: LocalizedStringResource = "Focus Terminal"
    static let description = IntentDescription("Brings a terminal of Lokyr's forward: its window, its tab and its pane.")
    static let openAppWhenRun: Bool = true

    @Parameter(title: "Terminal") var terminal: TerminalEntity

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.focus, first: terminal.pane)
        return .result()
    }
}

/// Hides the quick terminal, its shell running.
struct CloseQuickTerminalIntent: AppIntent {
    static let title: LocalizedStringResource = "Close Quick Terminal"
    static let description = IntentDescription("Hides Lokyr's quick terminal, which slides out; its shell keeps running, and Open Quick Terminal brings it back.")
    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.run, text: "window.hide-quick-terminal")
        return .result()
    }
}

/// Shows the quick terminal.
struct OpenQuickTerminalIntent: AppIntent {
    static let title: LocalizedStringResource = "Open Quick Terminal"
    static let description = IntentDescription("Shows Lokyr's quick terminal, which slides in over every app, opening it first.")
    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult {
        _ = try await ask(.run, text: "window.show-quick-terminal")
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
        _ = try await ask(.switchSession, text: name)
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
