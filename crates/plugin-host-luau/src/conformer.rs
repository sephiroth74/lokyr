//! The conformance suite's test plugin of Luau: a plugin whose code makes
//! the requests that the suite gives it through their functions of `cx`,
//! as a plugin of Luau makes them, and which tells what its code took, the
//! answers and the events, back as the SDK's types, by the front end's own
//! conversions, so that its trace is of what reached the host from Luau and
//! of what Luau took from the host.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use lokyr_plugin_sdk::command::{Args, Code, Command, Error};
use lokyr_plugin_sdk::host::Host;
use lokyr_plugin_sdk::message::{Call, Event, Request, Response};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::ViewId;
use lokyr_plugin_sdk::view::Node;
use mlua::{Lua, MultiValue, Table, Value};

use crate::shape::{self, Budget, ShapeError};
use crate::vm::Vm;
use crate::{Deadlines, ENTRY, Execution, Package, events, requests, tree};

/// The test plugin's entry.
const SOURCE: &str = include_str!("../luau/conformer/src/init.luau");

/// How long a call of the test plugin takes at most, its conversions too:
/// long past its work, as the suite checks no deadline.
const DEADLINE: Duration = Duration::from_secs(10);

/// The ids that each host gives its own, which the suite's scenarios name
/// by their symbols, as the suite's harness reads them.
pub trait Ids {
    /// `request`, of the host's ids of the symbols that it holds.
    fn resolved(&self, request: &Request) -> Request;
    /// `response`, of the symbols of the host's ids that it holds.
    fn response(&mut self, response: Response) -> Response;
    /// `event`, of the symbols of the host's ids that it holds.
    fn event(&mut self, event: Event) -> Event;
}

/// What the test plugin's code took, in its order.
#[derive(Clone, Debug, PartialEq)]
pub enum Taken {
    /// A request, as it reached the host from Luau, of its symbols, and
    /// the answer that Luau took, as a response, of the symbols too.
    Asked(Request, Response),
    /// A batch of events, as Luau took it, of the symbols.
    Heard(Vec<Event>),
}

/// The test plugin of Luau: its activation registers `command` and
/// subscribes to the events that it is given, each call of a command makes
/// the requests of the next of its asks, and its trace takes what its code
/// took, of the symbols of `ids`.
pub struct Conformer {
    name: String,
    package: Arc<Package>,
    vm: Option<Vm>,
    command: Command,
    subscriptions: Vec<String>,
    asks: VecDeque<Vec<Request>>,
    ids: Box<dyn Ids>,
    trace: Rc<RefCell<Vec<Taken>>>,
}

impl Conformer {
    /// The test plugin `name`, of the command `command`, the events of
    /// `subscriptions` and the requests of each call, `asks`, of the ids of
    /// `ids`: and its trace.
    pub fn new(
        name: &str,
        command: Command,
        subscriptions: Vec<String>,
        asks: VecDeque<Vec<Request>>,
        ids: Box<dyn Ids>,
    ) -> (Self, Rc<RefCell<Vec<Taken>>>) {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let conformer = Self {
            name: name.to_owned(),
            package: Arc::new(Package::new([(ENTRY.to_owned(), SOURCE.to_owned())])),
            vm: None,
            command,
            subscriptions,
            asks,
            ids,
            trace: Rc::clone(&trace),
        };
        (conformer, trace)
    }

    /// Has its code make `asked`, through their functions of `cx`, each of
    /// the host's ids of its symbols: each request as it reached the host,
    /// and the answer that its code took, as a response, of the symbols.
    fn run(&mut self, cx: &mut Cx<'_>, asked: &[Request]) -> Result<Vec<Taken>, Error> {
        let failed = |message: String| Error::new(Code::Failed, message);
        let vm = self
            .vm
            .as_ref()
            .ok_or_else(|| failed("the test plugin has no VM".to_owned()))?;
        let mut recorder = Recorder {
            outer: cx,
            ids: self.ids.as_mut(),
            made: Vec::new(),
        };
        let returned = {
            let mut inner = Cx::new(&mut recorder, &self.name);
            vm.invoke("run", DEADLINE, &mut inner, |lua| {
                Ok(vec![Value::Table(calls(lua, asked)?)])
            })
            .map_err(|failure| failed(format!("its code failed: {failure:?}")))?
            .unwrap_or_default()
        };
        let made = recorder.made;
        let answers = match returned.into_iter().next() {
            Some(Value::Table(answers)) => answers,
            other => return Err(failed(format!("its `run` returned {other:?}"))),
        };
        if made.len() != asked.len() || answers.raw_len() != asked.len() {
            return Err(failed(format!(
                "its code made {} requests and took {} answers of {}",
                made.len(),
                answers.raw_len(),
                asked.len()
            )));
        }
        made.into_iter()
            .zip(answers.sequence_values::<Table>())
            .map(|(request, packed)| {
                let answer = packed
                    .map_err(ShapeError::from)
                    .and_then(|packed| unpacked(&packed))
                    .and_then(|answer| requests::response_of(&request, &answer))
                    .map_err(|error| failed(format!("{request:?} answered: {error}")))?;
                Ok(Taken::Asked(request, answer))
            })
            .collect()
    }
}

/// The calls of `asked`, as Luau's tables of their functions' names and
/// their arguments, `nil` among them, with how many they are.
fn calls(lua: &Lua, asked: &[Request]) -> Result<Table, ShapeError> {
    let calls = lua.create_table()?;
    for (at, request) in asked.iter().enumerate() {
        let (name, args) = requests::call_of(lua, request)?;
        let given = lua.create_table()?;
        for (place, arg) in args.iter().enumerate() {
            given.raw_set(place + 1, arg.clone())?;
        }
        let call = lua.create_table()?;
        call.raw_set("name", name)?;
        call.raw_set("args", given)?;
        call.raw_set("n", args.len())?;
        calls.raw_set(at + 1, call)?;
    }
    Ok(calls)
}

/// The values of `packed`, as `table.pack` packed them.
fn unpacked(packed: &Table) -> Result<MultiValue, ShapeError> {
    let count: usize = packed.raw_get("n")?;
    (1..=count)
        .map(|at| packed.raw_get::<Value>(at).map_err(ShapeError::from))
        .collect::<Result<Vec<_>, _>>()
        .map(MultiValue::from_vec)
}

/// The host of the test plugin's code: each request, of the symbols that
/// the code gives, reaches the host of the call of its ids, and its answer
/// comes back of the symbols; it keeps the requests as they came.
struct Recorder<'r, 'c> {
    outer: &'r mut Cx<'c>,
    ids: &'r mut dyn Ids,
    made: Vec<Request>,
}

impl Host for Recorder<'_, '_> {
    fn handle(&mut self, call: Call) -> Response {
        let asked = self.ids.resolved(&call.request);
        self.made.push(call.request);
        let response = self.outer.request(asked);
        self.ids.response(response)
    }
}

impl Plugin for Conformer {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.vm.is_none() {
            let vm = Vm::new(
                &self.package,
                &self.name,
                Execution::Native,
                Deadlines::default(),
            )
            .map_err(|failure| Error::new(Code::Failed, format!("{failure:?}")))?;
            self.vm = Some(vm);
        }
        let declared = [
            Request::RegisterCommand(self.command.clone()),
            Request::Subscribe(self.subscriptions.clone()),
        ];
        for taken in self.run(cx, &declared)? {
            if let Taken::Asked(request, response) = taken
                && response != Response::Done
            {
                return Err(Error::new(
                    Code::Failed,
                    format!("{request:?} answered {response:?}"),
                ));
            }
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        let asked = self.asks.pop_front().unwrap_or_default();
        let taken = self.run(cx, &asked)?;
        self.trace.borrow_mut().extend(taken);
        Ok(())
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let failed = |message: String| Error::new(Code::Failed, message);
        let given: Vec<Event> = events
            .iter()
            .map(|event| self.ids.event(event.clone()))
            .collect();
        let vm = self
            .vm
            .as_ref()
            .ok_or_else(|| failed("the test plugin has no VM".to_owned()))?;
        let returned = vm
            .invoke("on_event", DEADLINE, cx, |lua| {
                Ok(vec![Value::Table(events::list_to_lua(lua, &given)?)])
            })
            .map_err(|failure| failed(format!("its code failed: {failure:?}")))?
            .unwrap_or_default();
        let Some(Value::Table(took)) = returned.into_iter().next() else {
            return Err(failed("its `on_event` returned no events".to_owned()));
        };
        let lua = vm.lua();
        let heard = took
            .sequence_values::<Value>()
            .map(|event| {
                event
                    .map_err(ShapeError::from)
                    .and_then(|event| events::from_lua(lua, event))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| failed(format!("an event that it took: {error}")))?;
        self.trace.borrow_mut().push(Taken::Heard(heard));
        Ok(())
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let vm = self.vm.as_ref()?;
        let returned = vm
            .invoke("render", DEADLINE, cx, |lua| {
                Ok(vec![shape::to_lua(lua, view)?])
            })
            .ok()??;
        match returned.into_iter().next() {
            Some(Value::Nil) | None => None,
            Some(tree) => tree::from_lua(tree, &Budget::default()).ok(),
        }
    }
}
