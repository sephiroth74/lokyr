//! AppleScript's commands of the dictionary that the bundle holds: windows,
//! tabs and splits created, text input, a window, a tab or a terminal
//! focused or closed, and an action of the app's commands performed. Each
//! is a script command of Cocoa Scripting's, whose default implementation
//! makes its request of the app's automation and returns its answer, at
//! once or as the command resumes.

use lokyr_platform_core::automation::{Answer, Arg, Edge, Request, Target};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{ClassType, MainThreadMarker, define_class, msg_send};
use objc2_foundation::{
    NSAppleEventDescriptor, NSArray, NSDictionary, NSNumber, NSObject, NSScriptCommand, NSString,
};

use crate::scripting::{
    Asked, Converted, ScriptTab, ScriptTerminal, ScriptWindow, ask, error_of, finish,
};

/// The dictionary's codes of a split's directions.
const RIGHT: u32 = u32::from_be_bytes(*b"LKdR");
const LEFT: u32 = u32::from_be_bytes(*b"LKdL");
const DOWN: u32 = u32::from_be_bytes(*b"LKdD");
const UP: u32 = u32::from_be_bytes(*b"LKdU");

/// The keyword of a record's fields of the user's, `keyASUserRecordFields`.
const USER_FIELDS: u32 = u32::from_be_bytes(*b"usrf");

/// The terms of AppleScript's own that the commands' arguments share their
/// names with, which a script compiles to the terms' codes rather than to
/// fields of the user's, as `{name:"api"}`: each code and the argument's
/// name.
const TERMS: [(u32, &str); 6] = [
    (u32::from_be_bytes(*b"pnam"), "name"),
    (u32::from_be_bytes(*b"pidx"), "index"),
    (u32::from_be_bytes(*b"colr"), "color"),
    (u32::from_be_bytes(*b"kind"), "kind"),
    (u32::from_be_bytes(*b"asdr"), "folder"),
    (u32::from_be_bytes(*b"ctxt"), "text"),
];

/// AppleScript's errors of a parameter of the wrong kind and of a
/// failure.
const WRONG_KIND: isize = -1703;
const FAILED: isize = -10000;

define_class!(
    /// `create window`.
    #[unsafe(super(NSScriptCommand, NSObject))]
    #[name = "LokyrCreateWindowCommand"]
    struct CreateWindowCommand;

    impl CreateWindowCommand {
        #[unsafe(method_id(performDefaultImplementation))]
        fn perform(&self) -> Option<Retained<AnyObject>> {
            create_window(self)
        }
    }
);

define_class!(
    /// `create tab`.
    #[unsafe(super(NSScriptCommand, NSObject))]
    #[name = "LokyrCreateTabCommand"]
    struct CreateTabCommand;

    impl CreateTabCommand {
        #[unsafe(method_id(performDefaultImplementation))]
        fn perform(&self) -> Option<Retained<AnyObject>> {
            create_tab(self)
        }
    }
);

define_class!(
    /// `split`.
    #[unsafe(super(NSScriptCommand, NSObject))]
    #[name = "LokyrSplitCommand"]
    struct SplitCommand;

    impl SplitCommand {
        #[unsafe(method_id(performDefaultImplementation))]
        fn perform(&self) -> Option<Retained<AnyObject>> {
            split(self)
        }
    }
);

define_class!(
    /// `input text`.
    #[unsafe(super(NSScriptCommand, NSObject))]
    #[name = "LokyrInputTextCommand"]
    struct InputTextCommand;

    impl InputTextCommand {
        #[unsafe(method_id(performDefaultImplementation))]
        fn perform(&self) -> Option<Retained<AnyObject>> {
            input_text(self)
        }
    }
);

define_class!(
    /// `focus`.
    #[unsafe(super(NSScriptCommand, NSObject))]
    #[name = "LokyrFocusCommand"]
    struct FocusCommand;

    impl FocusCommand {
        #[unsafe(method_id(performDefaultImplementation))]
        fn perform(&self) -> Option<Retained<AnyObject>> {
            focus(self)
        }
    }
);

define_class!(
    /// `close`.
    #[unsafe(super(NSScriptCommand, NSObject))]
    #[name = "LokyrCloseCommand"]
    struct CloseCommand;

    impl CloseCommand {
        #[unsafe(method_id(performDefaultImplementation))]
        fn perform(&self) -> Option<Retained<AnyObject>> {
            close(self)
        }
    }
);

define_class!(
    /// `perform action`.
    #[unsafe(super(NSScriptCommand, NSObject))]
    #[name = "LokyrPerformActionCommand"]
    struct PerformActionCommand;

    impl PerformActionCommand {
        #[unsafe(method_id(performDefaultImplementation))]
        fn perform(&self) -> Option<Retained<AnyObject>> {
            perform_action(self)
        }
    }
);

/// What `create window` does.
fn create_window(command: &NSScriptCommand) -> Option<Retained<AnyObject>> {
    let request = Request::NewWindow {
        folder: text_argument(command, "folder"),
        command: text_argument(command, "command"),
    };
    run(command, request, opened_window)
}

/// What `create tab` does.
fn create_tab(command: &NSScriptCommand) -> Option<Retained<AnyObject>> {
    let request = Request::NewTab {
        window: argument(command, "window")
            .and_then(|window| window.downcast::<ScriptWindow>().ok())
            .map(|window| window.id()),
        folder: text_argument(command, "folder"),
        command: text_argument(command, "command"),
    };
    run(command, request, opened_tab)
}

/// What `split` does.
fn split(command: &NSScriptCommand) -> Option<Retained<AnyObject>> {
    let edge = match argument(command, "direction")
        .and_then(|code| code.downcast::<NSNumber>().ok())
        .map(|code| code.unsignedIntValue())
    {
        None | Some(RIGHT) => Edge::Right,
        Some(LEFT) => Edge::Left,
        Some(DOWN) => Edge::Down,
        Some(UP) => Edge::Up,
        Some(_) => return refuse(command, "a split's direction is right, left, down or up"),
    };
    let pane = match receivers(command).as_slice() {
        [] => None,
        [Target::Pane(pane)] => Some(*pane),
        _ => return refuse(command, "a split splits one terminal"),
    };
    let request = Request::Split {
        pane,
        edge,
        folder: text_argument(command, "folder"),
        command: text_argument(command, "command"),
    };
    run(command, request, opened_terminal)
}

/// What `input text` does.
fn input_text(command: &NSScriptCommand) -> Option<Retained<AnyObject>> {
    let Some(text) = command
        .directParameter()
        .and_then(|text| text.downcast::<NSString>().ok())
    else {
        return refuse(command, "`input text` takes a text");
    };
    let request = Request::SendText {
        pane: argument(command, "terminal")
            .and_then(|terminal| terminal.downcast::<ScriptTerminal>().ok())
            .map(|terminal| terminal.id()),
        text: text.to_string(),
    };
    run(command, request, done)
}

/// What `focus` does.
fn focus(command: &NSScriptCommand) -> Option<Retained<AnyObject>> {
    match receivers(command).as_slice() {
        [target] => run(command, Request::Focus(*target), done),
        _ => refuse(command, "`focus` takes one window, tab or terminal"),
    }
}

/// What `close` does.
fn close(command: &NSScriptCommand) -> Option<Retained<AnyObject>> {
    let targets = receivers(command);
    if targets.is_empty() {
        return refuse(command, "`close` takes windows, tabs or terminals");
    }
    // Each closes at once; the first that fails tells why.
    for target in targets {
        if let Asked::Now(answer) = ask(Some(command), Request::Close(target), done) {
            if let Err(error) = done(answer) {
                return finish(command, Err(error));
            }
        } else {
            return None;
        }
    }
    None
}

/// What `perform action` does.
fn perform_action(command: &NSScriptCommand) -> Option<Retained<AnyObject>> {
    let Some(name) = command
        .directParameter()
        .and_then(|name| name.downcast::<NSString>().ok())
    else {
        return refuse(
            command,
            "`perform action` takes a command's name, such as \"splits.split\"",
        );
    };
    let args = match argument(command, "arguments") {
        None => Vec::new(),
        Some(record) => match arguments_of(&record) {
            Some(args) => args,
            None => {
                return refuse(
                    command,
                    "the arguments are a record of texts, numbers and booleans, such as {direction:\"right\"}; write a field named after a word of AppleScript's between bars, such as {|to|:\"end\"}",
                );
            }
        },
    };
    let request = Request::Run {
        command: name.to_string(),
        args,
    };
    run(command, request, done)
}

/// Registers the commands' classes with the runtime, before Cocoa
/// Scripting reads the dictionary, which finds each class by its name.
pub(crate) fn register() {
    let _ = (
        CreateWindowCommand::class(),
        CreateTabCommand::class(),
        SplitCommand::class(),
        InputTextCommand::class(),
        FocusCommand::class(),
        CloseCommand::class(),
        PerformActionCommand::class(),
    );
}

/// What `command` returns of the automation's answer to `request`, of
/// `convert`: at once, or as it resumes.
fn run(
    command: &NSScriptCommand,
    request: Request,
    convert: fn(Answer) -> Converted,
) -> Option<Retained<AnyObject>> {
    match ask(Some(command), request, convert) {
        Asked::Now(answer) => finish(command, convert(answer)),
        Asked::Later => None,
    }
}

/// Fails `command` of a parameter of the wrong kind, of `message`.
fn refuse(command: &NSScriptCommand, message: &str) -> Option<Retained<AnyObject>> {
    finish(command, Err((WRONG_KIND, message.to_owned())))
}

/// The evaluated argument `key` of `command`, if it has one.
fn argument(command: &NSScriptCommand, key: &str) -> Option<Retained<AnyObject>> {
    command
        .evaluatedArguments()?
        .objectForKey(&NSString::from_str(key))
}

/// The text argument `key` of `command`, if it has one.
fn text_argument(command: &NSScriptCommand, key: &str) -> Option<String> {
    argument(command, key)
        .and_then(|text| text.downcast::<NSString>().ok())
        .map(|text| text.to_string())
}

/// The windows, tabs and terminals that `command` acts on, its receivers.
fn receivers(command: &NSScriptCommand) -> Vec<Target> {
    let Some(receivers) = command.evaluatedReceivers() else {
        return Vec::new();
    };
    let objects: Vec<Retained<AnyObject>> = match receivers.downcast::<NSArray>() {
        Ok(list) => list.to_vec(),
        Err(single) => vec![single],
    };
    objects
        .iter()
        .filter_map(|object| target_of(object))
        .collect()
}

/// The window, tab or terminal of `object`, if it is one.
fn target_of(object: &AnyObject) -> Option<Target> {
    if let Some(window) = object.downcast_ref::<ScriptWindow>() {
        return Some(Target::Window(window.id()));
    }
    if let Some(tab) = object.downcast_ref::<ScriptTab>() {
        return Some(Target::Tab(tab.id()));
    }
    object
        .downcast_ref::<ScriptTerminal>()
        .map(|terminal| Target::Pane(terminal.id()))
}

/// The command's arguments of an AppleScript record: a dictionary of its
/// fields' names, or the record's descriptor; none of a value that is no
/// text, number or boolean, or of a field of a term that no argument is
/// named after.
fn arguments_of(record: &AnyObject) -> Option<Vec<(String, Arg)>> {
    if let Some(record) = record.downcast_ref::<NSAppleEventDescriptor>() {
        return record_fields(record);
    }
    let record = record.downcast_ref::<NSDictionary>()?;
    let mut args = Vec::new();
    for key in record.allKeys().to_vec() {
        let name = key.downcast_ref::<NSString>()?.to_string();
        let value = record.objectForKey(&key)?;
        let arg = match value.downcast_ref::<NSString>() {
            Some(text) => Arg::Text(text.to_string()),
            None => number_arg(value.downcast_ref::<NSNumber>()?),
        };
        args.push((name, arg));
    }
    Some(args)
}

/// The fields of a record's descriptor, as a script writes them: those of
/// the user's, `usrf`, and those of AppleScript's terms in [`TERMS`], by
/// their arguments' names.
fn record_fields(record: &NSAppleEventDescriptor) -> Option<Vec<(String, Arg)>> {
    let mut args = Vec::new();
    // SAFETY: each call is a method of `NSAppleEventDescriptor`'s of these
    // arguments and result, on a descriptor that lives through the calls,
    // of an index from 1 to its count.
    unsafe {
        let count: isize = msg_send![record, numberOfItems];
        for at in 1..=count {
            let keyword: u32 = msg_send![record, keywordForDescriptorAtIndex: at];
            let value: Option<Retained<NSAppleEventDescriptor>> =
                msg_send![record, descriptorAtIndex: at];
            let value = value?;
            if keyword == USER_FIELDS {
                args.extend(user_fields(&value)?);
            } else {
                args.push((term_name(keyword)?.to_owned(), descriptor_arg(&value)?));
            }
        }
    }
    Some(args)
}

/// The argument's name of AppleScript's term of `code`, if one of
/// [`TERMS`].
fn term_name(code: u32) -> Option<&'static str> {
    TERMS
        .iter()
        .find(|(term, _)| *term == code)
        .map(|(_, name)| *name)
}

/// A record's fields of the user's, of their list: each name and its value
/// in turn.
fn user_fields(fields: &NSAppleEventDescriptor) -> Option<Vec<(String, Arg)>> {
    // SAFETY: each call is a method of `NSAppleEventDescriptor`'s of these
    // arguments and result, on a descriptor that lives through the calls.
    unsafe {
        let count: isize = msg_send![fields, numberOfItems];
        let mut args = Vec::new();
        let mut at = 1;
        while at < count {
            let name: Option<Retained<NSAppleEventDescriptor>> =
                msg_send![fields, descriptorAtIndex: at];
            let value: Option<Retained<NSAppleEventDescriptor>> =
                msg_send![fields, descriptorAtIndex: at + 1];
            let name = name?.stringValue()?.to_string();
            let value = value?;
            args.push((name, descriptor_arg(&value)?));
            at += 2;
        }
        Some(args)
    }
}

/// The argument of a field's value: a text, a whole number, a boolean, or
/// the text of a real number.
fn descriptor_arg(value: &NSAppleEventDescriptor) -> Option<Arg> {
    // SAFETY: `descriptorType` and the values' getters are methods of
    // `NSAppleEventDescriptor`'s of no argument.
    unsafe {
        let kind: u32 = msg_send![value, descriptorType];
        match &kind.to_be_bytes() {
            b"utxt" | b"TEXT" | b"utf8" => Some(Arg::Text(value.stringValue()?.to_string())),
            b"long" | b"shor" => {
                let number: i32 = msg_send![value, int32Value];
                Some(Arg::Integer(i64::from(number)))
            }
            b"bool" | b"true" | b"fals" => {
                let yes: bool = msg_send![value, booleanValue];
                Some(Arg::Bool(yes))
            }
            b"doub" | b"sing" | b"comp" => {
                let number: f64 = msg_send![value, doubleValue];
                if number.fract() == 0.0 && number.abs() < 9.0e15 {
                    #[allow(clippy::cast_possible_truncation)]
                    Some(Arg::Integer(number as i64))
                } else {
                    Some(Arg::Text(number.to_string()))
                }
            }
            _ => None,
        }
    }
}

/// The argument of `number`: a boolean, a whole number, or the text of a
/// number that is not whole.
fn number_arg(number: &NSNumber) -> Arg {
    // SAFETY: an `NSNumber`'s type is a C string of its encoding.
    let kind = unsafe { std::ffi::CStr::from_ptr(number.objCType().as_ptr()) };
    match kind.to_bytes() {
        b"c" | b"B" => Arg::Bool(number.boolValue()),
        b"f" | b"d" => {
            let value = number.doubleValue();
            if value.fract() == 0.0 && value.abs() < 9.0e15 {
                #[allow(clippy::cast_possible_truncation)]
                Arg::Integer(value as i64)
            } else {
                Arg::Text(value.to_string())
            }
        }
        _ => Arg::Integer(number.longLongValue()),
    }
}

/// The window that a request opened, as the command's result, of which
/// Cocoa Scripting makes the script's reference, of its specifier.
fn opened_window(answer: Answer) -> Converted {
    opened(answer, |mtm, window, _, _| {
        object_of(ScriptWindow::of_id(mtm, window))
    })
}

/// The tab that a request opened, as the command's result.
fn opened_tab(answer: Answer) -> Converted {
    opened(answer, |mtm, window, tab, _| {
        object_of(ScriptTab::of_ids(mtm, window, tab))
    })
}

/// The terminal that a request opened, as the command's result.
fn opened_terminal(answer: Answer) -> Converted {
    opened(answer, |mtm, window, tab, pane| {
        object_of(ScriptTerminal::of_ids(mtm, window, tab, pane))
    })
}

/// The object that `make` makes of what `answer` opened.
fn opened(
    answer: Answer,
    make: impl Fn(MainThreadMarker, u64, u64, u64) -> Retained<AnyObject>,
) -> Converted {
    match answer {
        Answer::Opened { window, tab, pane } => {
            let mtm = MainThreadMarker::new().ok_or_else(|| {
                (
                    FAILED,
                    "a script's command runs off the main thread".to_owned(),
                )
            })?;
            Ok(Some(make(mtm, window, tab, pane)))
        }
        other => Err(error_of(&other)),
    }
}

/// `object` as an object of a command's result.
fn object_of<T: ClassType<Super = NSObject> + 'static>(object: Retained<T>) -> Retained<AnyObject> {
    Retained::into_super(Retained::into_super(object))
}

/// Nothing, of a request done.
fn done(answer: Answer) -> Converted {
    match answer {
        Answer::Done => Ok(None),
        other => Err(error_of(&other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A descriptor of `text`.
    fn text(text: &str) -> Retained<NSAppleEventDescriptor> {
        NSAppleEventDescriptor::descriptorWithString(&NSString::from_str(text))
    }

    #[test]
    fn a_record_s_fields_of_applescript_s_terms_are_its_arguments() {
        // As AppleScript compiles `{direction:"right", name:"api",
        // index:2}`: a field of the user's, and two of its own terms.
        let record = NSAppleEventDescriptor::recordDescriptor();
        let fields = NSAppleEventDescriptor::listDescriptor();
        // SAFETY: methods of `NSAppleEventDescriptor`'s of these arguments,
        // a descriptor, an index from 1 or a keyword, on descriptors that
        // live through the calls.
        unsafe {
            let _: () =
                msg_send![&*fields, insertDescriptor: &*text("direction"), atIndex: 1_isize];
            let _: () = msg_send![&*fields, insertDescriptor: &*text("right"), atIndex: 2_isize];
            let _: () = msg_send![&*record, setDescriptor: &*fields, forKeyword: USER_FIELDS];
            let _: () = msg_send![&*record, setDescriptor: &*text("api"), forKeyword: u32::from_be_bytes(*b"pnam")];
            let index = NSAppleEventDescriptor::descriptorWithInt32(2);
            let _: () = msg_send![&*record, setDescriptor: &*index, forKeyword: u32::from_be_bytes(*b"pidx")];
        }
        let mut args = record_fields(&record).unwrap();
        args.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            args,
            [
                ("direction".to_owned(), Arg::Text("right".to_owned())),
                ("index".to_owned(), Arg::Integer(2)),
                ("name".to_owned(), Arg::Text("api".to_owned())),
            ]
        );
        // A term that no argument is named after refuses the record.
        // SAFETY: as above.
        unsafe {
            let _: () = msg_send![&*record, setDescriptor: &*text("x"), forKeyword: u32::from_be_bytes(*b"ID  ")];
        }
        assert_eq!(record_fields(&record), None);
    }
}
