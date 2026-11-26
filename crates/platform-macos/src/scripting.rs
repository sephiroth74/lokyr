//! AppleScript's objects of the dictionary that the bundle holds,
//! `Lokyr.sdef`: the application's windows, their tabs and each tab's
//! terminals, its panes, of Cocoa Scripting's key-value coding. Each read
//! goes through the app's automation, as the system's scripts ask: the
//! windows and what they hold of one request, which the automation's plugin
//! answers at once; a terminal's text of its own, which may wait for the
//! user's answer to a question, the script's command suspended meanwhile.
//! Each object's specifier names it by its unique id, the app's, which
//! stays the same between a script's requests.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use lokyr_platform_core::automation::{Answer, Pane, Request, Tab, Window};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{
    AllocAnyThread, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, Message,
    define_class, msg_send,
};
use objc2_app_kit::{NSApplication, NSRunningApplication};
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSArray, NSNumber, NSObject, NSRunLoop,
    NSScriptClassDescription, NSScriptCommand, NSScriptObjectSpecifier, NSString,
    NSUniqueIDSpecifier,
};

/// The application's key of its windows, as the dictionary names it, which
/// its delegate answers.
pub(crate) const WINDOWS: &str = "lokyrWindows";

/// The keys of a window's tabs and a tab's terminals.
const TABS: &str = "tabs";
const TERMINALS: &str = "terminals";

/// The attribute of an Apple event that holds its sender's process id,
/// `keySenderPIDAttr`.
const SENDER_PID: u32 = u32::from_be_bytes(*b"spid");

/// AppleScript's errors: a failure, an object not there, a parameter
/// missing, a value of the wrong kind, and a privilege lacking.
const FAILED: isize = -10000;
const NO_SUCH_OBJECT: isize = -1728;
const PARAMETER_MISSING: isize = -1715;
const WRONG_KIND: isize = -1703;
const PRIVILEGE: isize = -10004;

/// The registry's codes of a thing not there and of an argument missing.
const NOT_FOUND: u16 = 101;
const MISSING_ARGUMENT: u16 = 103;

/// What a window holds.
pub(crate) struct WindowIvars {
    window: Window,
}

/// What a tab holds, and its window's id.
pub(crate) struct TabIvars {
    window: u64,
    tab: Tab,
}

/// What a terminal holds, and its window's and its tab's ids.
pub(crate) struct TerminalIvars {
    window: u64,
    tab: u64,
    pane: Pane,
}

define_class!(
    /// A window of AppleScript's.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LokyrScriptWindow"]
    #[ivars = WindowIvars]
    pub(crate) struct ScriptWindow;

    impl ScriptWindow {
        #[unsafe(method_id(uniqueID))]
        fn unique_id(&self) -> Retained<NSNumber> {
            NSNumber::numberWithUnsignedLongLong(self.ivars().window.id)
        }

        #[unsafe(method_id(name))]
        fn name(&self) -> Retained<NSString> {
            NSString::from_str(&self.ivars().window.title)
        }

        #[unsafe(method_id(tabs))]
        fn tabs(&self) -> Retained<NSArray<ScriptTab>> {
            let window = &self.ivars().window;
            let tabs: Vec<Retained<ScriptTab>> = window
                .tabs
                .iter()
                .map(|tab| ScriptTab::new(self.mtm(), window.id, tab.clone()))
                .collect();
            NSArray::from_retained_slice(&tabs)
        }

        #[unsafe(method_id(objectSpecifier))]
        fn object_specifier(&self) -> Option<Retained<NSScriptObjectSpecifier>> {
            window_specifier(self.ivars().window.id)
        }
    }
);

define_class!(
    /// A tab of AppleScript's.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LokyrScriptTab"]
    #[ivars = TabIvars]
    pub(crate) struct ScriptTab;

    impl ScriptTab {
        #[unsafe(method_id(uniqueID))]
        fn unique_id(&self) -> Retained<NSNumber> {
            NSNumber::numberWithUnsignedLongLong(self.ivars().tab.id)
        }

        #[unsafe(method_id(name))]
        fn name(&self) -> Retained<NSString> {
            NSString::from_str(&self.ivars().tab.title)
        }

        #[unsafe(method_id(terminals))]
        fn terminals(&self) -> Retained<NSArray<ScriptTerminal>> {
            let ivars = self.ivars();
            let terminals: Vec<Retained<ScriptTerminal>> = ivars
                .tab
                .panes
                .iter()
                .map(|pane| ScriptTerminal::new(self.mtm(), ivars.window, ivars.tab.id, pane.clone()))
                .collect();
            NSArray::from_retained_slice(&terminals)
        }

        #[unsafe(method_id(objectSpecifier))]
        fn object_specifier(&self) -> Option<Retained<NSScriptObjectSpecifier>> {
            let ivars = self.ivars();
            tab_specifier(ivars.window, ivars.tab.id)
        }
    }
);

define_class!(
    /// A terminal of AppleScript's, a pane.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LokyrScriptTerminal"]
    #[ivars = TerminalIvars]
    pub(crate) struct ScriptTerminal;

    impl ScriptTerminal {
        #[unsafe(method_id(uniqueID))]
        fn unique_id(&self) -> Retained<NSNumber> {
            NSNumber::numberWithUnsignedLongLong(self.ivars().pane.id)
        }

        #[unsafe(method_id(name))]
        fn name(&self) -> Retained<NSString> {
            NSString::from_str(&self.ivars().pane.title)
        }

        #[unsafe(method_id(workingDirectory))]
        fn working_directory(&self) -> Retained<NSString> {
            NSString::from_str(self.ivars().pane.folder.as_deref().unwrap_or_default())
        }

        #[unsafe(method_id(contents))]
        fn contents(&self) -> Option<Retained<NSString>> {
            text(self.ivars().pane.id, false)
        }

        #[unsafe(method_id(selectedText))]
        fn selected_text(&self) -> Option<Retained<NSString>> {
            text(self.ivars().pane.id, true)
        }

        #[unsafe(method_id(objectSpecifier))]
        fn object_specifier(&self) -> Option<Retained<NSScriptObjectSpecifier>> {
            let ivars = self.ivars();
            terminal_specifier(ivars.window, ivars.tab, ivars.pane.id)
        }
    }
);

impl ScriptWindow {
    /// The window's id.
    pub(crate) fn id(&self) -> u64 {
        self.ivars().window.id
    }

    fn new(mtm: MainThreadMarker, window: Window) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(WindowIvars { window });
        // SAFETY: `NSObject`'s `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }
}

impl ScriptTab {
    /// The tab's id.
    pub(crate) fn id(&self) -> u64 {
        self.ivars().tab.id
    }

    fn new(mtm: MainThreadMarker, window: u64, tab: Tab) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TabIvars { window, tab });
        // SAFETY: `NSObject`'s `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }
}

impl ScriptTerminal {
    /// The terminal's id, its pane's.
    pub(crate) fn id(&self) -> u64 {
        self.ivars().pane.id
    }

    fn new(mtm: MainThreadMarker, window: u64, tab: u64, pane: Pane) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TerminalIvars { window, tab, pane });
        // SAFETY: `NSObject`'s `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }
}

/// Registers the dictionary's classes with the runtime, before Cocoa
/// Scripting reads the dictionary, which finds each class by its name.
pub(crate) fn register() {
    let _ = (
        ScriptWindow::class(),
        ScriptTab::class(),
        ScriptTerminal::class(),
    );
    crate::script_commands::register();
}

/// The application's windows, of the automation's answer, which comes at
/// once; none, of the script command's error, otherwise.
pub(crate) fn windows(mtm: MainThreadMarker) -> Retained<NSArray<ScriptWindow>> {
    let windows = match ask_now(Request::Workspace) {
        Some(Answer::Workspace(windows)) => windows,
        Some(other) => {
            fail(&other);
            Vec::new()
        }
        None => Vec::new(),
    };
    let windows: Vec<Retained<ScriptWindow>> = windows
        .into_iter()
        .map(|window| ScriptWindow::new(mtm, window))
        .collect();
    NSArray::from_retained_slice(&windows)
}

/// The text of the pane `pane`, its selection or its screen: at once, or
/// none while the script's command is suspended until the answer comes,
/// which resumes it.
fn text(pane: u64, selection: bool) -> Option<Retained<NSString>> {
    let request = Request::PaneText {
        pane: Some(pane),
        selection,
    };
    match ask(
        NSScriptCommand::currentCommand().as_deref(),
        request,
        text_result,
    ) {
        Asked::Now(Answer::Text(text)) => Some(NSString::from_str(&text)),
        Asked::Now(other) => {
            fail(&other);
            None
        }
        Asked::Later => None,
    }
}

/// A script's text, as a resumed command returns it: a descriptor, as the
/// command's reply takes a result of its own type, `any`, of which a
/// descriptor is one and a plain text none.
fn text_result(answer: Answer) -> Converted {
    match answer {
        Answer::Text(text) => Ok(Some(object(NSAppleEventDescriptor::descriptorWithString(
            &NSString::from_str(&text),
        )))),
        other => Err(error_of(&other)),
    }
}

/// What a command returns of an answer, or its error's number and message.
pub(crate) type Converted = Result<Option<Retained<AnyObject>>, (isize, String)>;

/// The automation's answer to a script's request.
pub(crate) enum Asked {
    /// It came at once.
    Now(Answer),
    /// It comes later, the command suspended until then, or it never comes,
    /// of the command's error.
    Later,
}

/// Asks the automation `request` for `command`: its answer, if it comes at
/// once; or, as it comes later, `command` suspended until then, which then
/// resumes of `convert`'s result, at the run loop's next turn.
pub(crate) fn ask(
    command: Option<&NSScriptCommand>,
    request: Request,
    convert: fn(Answer) -> Converted,
) -> Asked {
    let command: Option<Retained<NSScriptCommand>> = command.map(Message::retain);
    let slot: Rc<RefCell<Option<Answer>>> = Rc::default();
    let suspended = Rc::new(Cell::new(false));
    let reply = {
        let (slot, suspended, command) = (slot.clone(), suspended.clone(), command.clone());
        move |answer: Answer| match (suspended.get(), command) {
            // The command resumes at the run loop's next turn, out of the
            // app's call that answers.
            (true, Some(command)) => resume_later(command, convert(answer)),
            _ => *slot.borrow_mut() = Some(answer),
        }
    };
    if !crate::app::post_automation(&client(), request, reply) {
        fail_with(FAILED, "Lokyr cannot take the request now");
        return Asked::Later;
    }
    let answer = slot.borrow_mut().take();
    if let Some(answer) = answer {
        return Asked::Now(answer);
    }
    // The answer comes later, as a window opens or the user answers a
    // question: the command waits for it.
    if let Some(command) = &command {
        suspended.set(true);
        command.suspendExecution();
    }
    Asked::Later
}

/// What `command` returns of `converted`, setting its error of one.
pub(crate) fn finish(
    command: &NSScriptCommand,
    converted: Converted,
) -> Option<Retained<AnyObject>> {
    match converted {
        Ok(result) => result,
        Err((number, message)) => {
            command.setScriptErrorNumber(number);
            command.setScriptErrorString(Some(&NSString::from_str(&message)));
            None
        }
    }
}

/// Resumes `command` at the run loop's next turn, of `converted`.
fn resume_later(command: Retained<NSScriptCommand>, converted: Converted) {
    let pending = RefCell::new(Some((command, converted)));
    let block = block2::RcBlock::new(move || {
        let Some((command, converted)) = pending.borrow_mut().take() else {
            return;
        };
        let result = finish(&command, converted);
        let result: Option<&AnyObject> = result.as_deref();
        // An exception of Cocoa Scripting's as it replies, which no Rust
        // frame may unwind through, fails this script alone.
        let resumed = objc2::exception::catch(std::panic::AssertUnwindSafe(|| {
            // SAFETY: the result is the command's, a descriptor of its
            // reply, or none, of an error that the command holds.
            unsafe { command.resumeExecutionWithResult(result) };
        }));
        if let Err(exception) = resumed {
            eprintln!("lokyr: a script's command did not resume: {exception:?}");
        }
    });
    // SAFETY: the block runs once, on the main thread, as the main run
    // loop's.
    unsafe { NSRunLoop::mainRunLoop().performBlock(&block) };
}

/// `descriptor`, as an object of a command's result.
pub(crate) fn object(descriptor: Retained<NSAppleEventDescriptor>) -> Retained<AnyObject> {
    Retained::into_super(Retained::into_super(descriptor))
}

/// The automation's answer to `request`, if it comes at once; none, of the
/// script command's error, otherwise.
fn ask_now(request: Request) -> Option<Answer> {
    let slot: Rc<RefCell<Option<Answer>>> = Rc::default();
    let reply = {
        let slot = slot.clone();
        move |answer: Answer| *slot.borrow_mut() = Some(answer)
    };
    if !crate::app::post_automation(&client(), request, reply) {
        fail_with(FAILED, "Lokyr cannot take the request now");
        return None;
    }
    let answer = slot.borrow_mut().take();
    if answer.is_none() {
        fail_with(FAILED, "Lokyr did not answer at once");
    }
    answer
}

/// Has the script's command fail of `answer`'s error.
fn fail(answer: &Answer) {
    let (number, message) = error_of(answer);
    fail_with(number, &message);
}

/// AppleScript's error of `answer`: its number, of the registry's code,
/// and its message.
pub(crate) fn error_of(answer: &Answer) -> (isize, String) {
    match answer {
        Answer::Error { code, message } => (error_number(*code), message.clone()),
        other => (FAILED, format!("Lokyr answered otherwise: {other:?}")),
    }
}

/// AppleScript's error of the registry's code `code`: an object not there,
/// a parameter missing, a value of the wrong kind of the other mistakes of
/// a request, a privilege for what the security refuses, and a failure.
fn error_number(code: u16) -> isize {
    match code {
        NOT_FOUND => NO_SUCH_OBJECT,
        MISSING_ARGUMENT => PARAMETER_MISSING,
        100..=199 => WRONG_KIND,
        300..=399 => PRIVILEGE,
        _ => FAILED,
    }
}

/// Has the script's command fail of `number` and `message`.
fn fail_with(number: isize, message: &str) {
    if let Some(command) = NSScriptCommand::currentCommand() {
        command.setScriptErrorNumber(number);
        command.setScriptErrorString(Some(&NSString::from_str(message)));
    }
}

/// The app that sent the Apple event that runs now, by its name and its
/// bundle's identifier, or its process's name.
fn client() -> String {
    let pid = NSAppleEventManager::sharedAppleEventManager()
        .currentAppleEvent()
        .and_then(|event| {
            // SAFETY: `attributeDescriptorForKeyword:` takes a keyword, a
            // four characters' code, and answers a descriptor or none.
            let sender: Option<Retained<NSAppleEventDescriptor>> =
                unsafe { msg_send![&*event, attributeDescriptorForKeyword: SENDER_PID] };
            sender
        })
        .map(|sender| sender.int32Value());
    let Some(pid) = pid.filter(|&pid| pid > 0) else {
        return "an app".to_owned();
    };
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
    let name = app
        .as_ref()
        .and_then(|app| app.localizedName())
        .map(|name| name.to_string());
    let bundle = app
        .as_ref()
        .and_then(|app| app.bundleIdentifier())
        .map(|id| id.to_string());
    match (name, bundle) {
        (Some(name), Some(bundle)) => format!("{name} ({bundle})"),
        (None, Some(bundle)) => bundle,
        _ => process_name(pid).unwrap_or_else(|| format!("process {pid}")),
    }
}

/// The name of the process `pid`, as `osascript`.
fn process_name(pid: i32) -> Option<String> {
    let mut name = [0u8; 256];
    // SAFETY: the buffer holds its length's bytes, which the call fills
    // with a name ended by a zero.
    let length = unsafe { libc::proc_name(pid, name.as_mut_ptr().cast(), 256) };
    let length = usize::try_from(length).ok().filter(|&length| length > 0)?;
    Some(String::from_utf8_lossy(&name[..length]).into_owned())
}

/// The specifier of the window `window`, of the application's.
pub(crate) fn window_specifier(window: u64) -> Option<Retained<NSScriptObjectSpecifier>> {
    specifier(NSApplication::class(), None, WINDOWS, window)
}

/// The specifier of the tab `tab` of the window `window`.
pub(crate) fn tab_specifier(window: u64, tab: u64) -> Option<Retained<NSScriptObjectSpecifier>> {
    let container = window_specifier(window)?;
    specifier(ScriptWindow::class(), Some(&container), TABS, tab)
}

/// The specifier of the terminal `pane` of the tab `tab` of the window
/// `window`.
pub(crate) fn terminal_specifier(
    window: u64,
    tab: u64,
    pane: u64,
) -> Option<Retained<NSScriptObjectSpecifier>> {
    let container = tab_specifier(window, tab)?;
    specifier(ScriptTab::class(), Some(&container), TERMINALS, pane)
}

/// A specifier of the object of the unique id `id` among the elements
/// `key` of a container of the class `class`, of `container`'s specifier,
/// or of the application's without one.
fn specifier(
    class: &AnyClass,
    container: Option<&NSScriptObjectSpecifier>,
    key: &str,
    id: u64,
) -> Option<Retained<NSScriptObjectSpecifier>> {
    // SAFETY: a class of the dictionary, whose description the dictionary
    // gives.
    let description = unsafe { NSScriptClassDescription::classDescriptionForClass(class) }?;
    let id = NSNumber::numberWithUnsignedLongLong(id);
    // SAFETY: the container's description and specifier, a key of its
    // elements, and a number, as the dictionary's ids are integers.
    let specifier = unsafe {
        NSUniqueIDSpecifier::initWithContainerClassDescription_containerSpecifier_key_uniqueID(
            NSUniqueIDSpecifier::alloc(),
            &description,
            container,
            &NSString::from_str(key),
            &id,
        )
    };
    Some(Retained::into_super(specifier))
}
