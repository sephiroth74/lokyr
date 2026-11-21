//! A plugin's temporary files, of no capability: written whole in a folder
//! of the plugin's own, marked, bounded, and emptied at the start and the
//! quit; and the branch of a folder's git repository.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lokyr_plugin_sdk::host::{
    Bytes, MOST_TEMPORARY_FILES, NO_TEMPORARY_FOLDER, TEMPORARY_BUSY, TEMPORARY_RATE,
};
use lokyr_plugin_sdk::message::Request as Asked;

use super::*;

/// The files that [`mark`] marked, as the platform marks a download.
static MARKED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn mark(path: &Path) -> std::io::Result<()> {
    MARKED.lock().unwrap().push(path.to_owned());
    Ok(())
}

/// A plugin whose command `asker.go` makes the requests that `asks` holds
/// then, keeping what each got in `got`.
fn asker(asks: &Rc<RefCell<Vec<Asked>>>, got: &Rc<RefCell<Vec<Response>>>) -> Box<dyn Plugin> {
    let (asks, got) = (Rc::clone(asks), Rc::clone(got));
    scripted(
        |cx| cx.register_command(command("asker.go")),
        move |cx, _, _| {
            let asked: Vec<Asked> = asks.borrow_mut().drain(..).collect();
            for request in asked {
                let response = cx.request(request);
                got.borrow_mut().push(response);
            }
            Ok(())
        },
    )
}

/// A temporary file `name` of `text`.
fn file(name: &str, text: &str) -> Asked {
    Asked::TempFile {
        name: name.to_owned(),
        bytes: Bytes(text.as_bytes().to_vec()),
    }
}

#[test]
fn a_plugin_s_temporary_files_are_written_marked_bounded_and_emptied() {
    let mut app = App::default();
    let folder = std::env::temp_dir().join(format!("lokyr-temporary-{}", std::process::id()));
    let (asks, got) = (Rc::default(), Rc::default());
    let mut plugins = Plugins::new();
    plugins.add("asker", asker(&asks, &got)).unwrap();
    // A file of a run that did not quit, which the start takes away.
    std::fs::create_dir_all(folder.join("asker")).unwrap();
    std::fs::write(folder.join("asker/left.txt"), "left").unwrap();
    plugins.set_temporary_directory(Some(folder.clone()), mark);
    assert!(!folder.exists());
    assert!(plugins.activate(&mut app.context()).is_empty());
    let mut ask = |plugins: &mut Plugins, requests: Vec<Asked>| -> Vec<Response> {
        asks.borrow_mut().extend(requests);
        plugins
            .run("asker.go", Args::new(), &mut app.context())
            .unwrap();
        got.borrow_mut().drain(..).collect()
    };
    // Written in the plugin's folder, whole, marked before it takes its
    // name: its path the answer.
    let path = folder.join("asker/output.txt");
    let wanted = Response::Text(Some(path.to_string_lossy().into_owned()));
    assert_eq!(
        ask(&mut plugins, vec![file("output.txt", "ls\n")]),
        std::slice::from_ref(&wanted)
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "ls\n");
    assert!(
        MARKED
            .lock()
            .unwrap()
            .iter()
            .any(|marked| marked.parent() == path.parent()),
        "{:?}",
        MARKED.lock().unwrap()
    );
    // Of the same name, in the old one's place.
    assert_eq!(
        ask(&mut plugins, vec![file("output.txt", "ls -l\n")]),
        [wanted]
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "ls -l\n");
    // As many as the bound, then refused.
    let more: Vec<Asked> = (1..MOST_TEMPORARY_FILES)
        .map(|at| file(&format!("{at}.txt"), "x"))
        .collect();
    let written = ask(&mut plugins, more);
    assert!(
        written
            .iter()
            .all(|response| matches!(response, Response::Text(Some(_)))),
        "{written:?}"
    );
    let [Response::Failed(error)] = &ask(&mut plugins, vec![file("one-more.txt", "x")])[..] else {
        panic!("a file past the bound");
    };
    assert_eq!(error.code, Code::Failed);
    assert!(!folder.join("asker/one-more.txt").exists());
    // The quit takes them all away.
    plugins.empty_temporary();
    assert!(!folder.exists());
    // Without a state's folder, none.
    plugins.set_temporary_directory(None, mark);
    let [Response::Failed(error)] = &ask(&mut plugins, vec![file("output.txt", "x")])[..] else {
        panic!("a file of no folder");
    };
    assert_eq!(error.message, NO_TEMPORARY_FOLDER);
}

#[test]
fn a_plugin_s_temporary_files_take_its_budget_of_writes_which_comes_back_in_time() {
    let mut app = App::default();
    let folder = std::env::temp_dir().join(format!("lokyr-budget-{}", std::process::id()));
    let (asks, got) = (Rc::default(), Rc::default());
    let mut plugins = Plugins::new();
    plugins.add("asker", asker(&asks, &got)).unwrap();
    plugins.set_temporary_directory(Some(folder.clone()), mark);
    assert!(plugins.activate(&mut app.context()).is_empty());
    let big = Asked::TempFile {
        name: "big.txt".to_owned(),
        bytes: Bytes(vec![b'x'; TEMPORARY_RATE as usize]),
    };
    let ask = |plugins: &mut Plugins, app: &mut App, times: usize| -> Vec<Response> {
        asks.borrow_mut()
            .extend(std::iter::repeat_n(big.clone(), times));
        plugins
            .run("asker.go", Args::new(), &mut app.context())
            .unwrap();
        got.borrow_mut().drain(..).collect()
    };
    // Four files of 8 MiB take the budget's 32, the fifth waits.
    let written = ask(&mut plugins, &mut app, 5);
    assert!(
        written[..4]
            .iter()
            .all(|response| matches!(response, Response::Text(Some(_)))),
        "{written:?}"
    );
    assert!(
        matches!(&written[4], Response::Failed(error) if error.message == TEMPORARY_BUSY),
        "{written:?}"
    );
    // A second gives 8 MiB back.
    app.wait(Duration::from_secs(1));
    let written = ask(&mut plugins, &mut app, 2);
    assert!(matches!(written[0], Response::Text(Some(_))), "{written:?}");
    assert!(matches!(written[1], Response::Failed(_)), "{written:?}");
    plugins.empty_temporary();
}

#[test]
fn a_folder_s_branch_reads_of_process_read_alone() {
    let mut app = App::default();
    let root = std::env::temp_dir().join(format!("lokyr-branch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    let branch = Asked::GitBranch {
        folder: root.join("src").to_string_lossy().into_owned(),
    };
    // A plugin of every capability, as a first-party one, reads it.
    let (asks, got) = (Rc::default(), Rc::default());
    let mut plugins = Plugins::new();
    plugins.add("asker", asker(&asks, &got)).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    asks.borrow_mut().push(branch.clone());
    plugins
        .run("asker.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        got.borrow_mut().drain(..).collect::<Vec<_>>(),
        [Response::Text(Some("main".to_owned()))]
    );
    // One of no `process-read` is refused.
    let mut app = App::default();
    let manifest = lokyr_pkg_model::manifest::Manifest::parse(
        "[plugin]\nname = \"lokyr/asker\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    )
    .unwrap();
    let mut plugins = Plugins::new();
    plugins.add_declared(manifest, asker(&asks, &got)).unwrap();
    let failed = plugins.activate(&mut app.context());
    assert!(failed.is_empty(), "{failed:?}");
    asks.borrow_mut().push(branch);
    plugins
        .run("asker.go", Args::new(), &mut app.context())
        .unwrap();
    let got: Vec<Response> = got.borrow_mut().drain(..).collect();
    assert!(
        matches!(&got[..], [Response::Failed(error)] if error.code == Code::MissingCapability),
        "{got:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
