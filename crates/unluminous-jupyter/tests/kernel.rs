//! The kernel module against a real Jupyter kernel.
//!
//! Nothing here is faked: each test starts the bridge, which starts an `ipykernel`, and checks what a
//! cell really produces. A kernel is started by every test, so tests may run side by side. When the
//! machine has no Python with `ipykernel` a test says so in one line and passes, so the suite still
//! runs on a machine without Python.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use unluminous_jupyter::kernel::{
    find_pythons, install_command, list_kernelspecs, Event, Kernel, KernelState, Waker,
};

/// Longer than the bridge's own 180 seconds, so a slow start is reported by the bridge.
const START_LIMIT: Duration = Duration::from_secs(200);
const RUN_LIMIT: Duration = Duration::from_secs(60);

/// The Python the tests run kernels with, or `None` when the machine has no suitable one.
fn test_python() -> Option<PathBuf> {
    static CHOSEN: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    CHOSEN.get_or_init(choose_python).clone()
}

/// Choose the Python once for the whole run, so tests that run side by side do not each search.
fn choose_python() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("UNLUMINOUS_TEST_PYTHON").filter(|value| !value.is_empty())
    {
        return Some(PathBuf::from(path));
    }
    let found = find_pythons(None)
        .into_iter()
        .find(|python| python.has_ipykernel && python.has_jupyter_client);
    found.map(|python| python.path)
}

/// A started kernel, the events seen so far, and a way to wait for the next one.
struct Session {
    kernel: Kernel,
    log: Vec<Event>,
    wake: Arc<(Mutex<bool>, Condvar)>,
    python: PathBuf,
    _place: Place,
}

impl Session {
    /// Start a kernel and wait for it to be ready. `None` when there is no Python to use.
    fn start() -> Option<Session> {
        let Some(python) = test_python() else {
            println!(
                "no Python with ipykernel and jupyter_client was found, so this test did nothing"
            );
            return None;
        };
        let mut session = Session::unstarted(&python, None);
        let began = Instant::now();
        let started = session.wait_for(START_LIMIT, |event| matches!(event, Event::Started { .. }));
        println!("the kernel took {:?} to start", began.elapsed());
        assert!(
            started.is_some(),
            "the kernel did not start: {:?}\n{}",
            session.log,
            session.kernel.stderr_tail()
        );
        Some(session)
    }

    /// Start the bridge without waiting for the kernel.
    fn unstarted(python: &Path, kernel_name: Option<&str>) -> Session {
        let place = Place::take();
        let wake = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = wake.clone();
        let waker: Waker = Arc::new(move || {
            *signal.0.lock().unwrap() = true;
            signal.1.notify_all();
        });
        let folder = std::env::temp_dir();
        let kernel = Kernel::start(python, kernel_name, &folder, waker).expect("the bridge starts");
        Session { kernel, log: Vec::new(), wake, python: python.to_path_buf(), _place: place }
    }

    /// Wait until an event in the log (new ones included) satisfies `found`, and return a copy of it.
    fn wait_for(&mut self, limit: Duration, found: impl Fn(&Event) -> bool) -> Option<Event> {
        self.wait_from(0, limit, found)
    }

    /// Like [`Session::wait_for`], but only looks at events at or after position `from` in the log.
    fn wait_from(
        &mut self,
        from: usize,
        limit: Duration,
        found: impl Fn(&Event) -> bool,
    ) -> Option<Event> {
        let deadline = Instant::now() + limit;
        loop {
            let events = self.kernel.events();
            self.log.extend(events);
            if let Some(event) = self.log.iter().skip(from).find(|event| found(event)) {
                return Some(event.clone());
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (flag, signal) = &*self.wake;
            let held = flag.lock().unwrap();
            let (mut held, _) = signal
                .wait_timeout(held, (deadline - now).min(Duration::from_millis(200)))
                .unwrap();
            *held = false;
        }
    }

    /// Wait until `done` is true of the events at or after `from`. False when `limit` passes first.
    fn wait_until(
        &mut self,
        from: usize,
        limit: Duration,
        done: impl Fn(&[Event]) -> bool,
    ) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            let events = self.kernel.events();
            self.log.extend(events);
            if done(&self.log[from..]) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            let (flag, signal) = &*self.wake;
            let held = flag.lock().unwrap();
            let _ = signal.wait_timeout(held, Duration::from_millis(200)).unwrap();
        }
    }

    /// Run a cell and return every event it produced, once the kernel has replied and gone idle.
    fn run(&mut self, code: &str) -> Vec<Event> {
        let from = self.log.len();
        let id = self.kernel.execute(code);
        self.finish(from, &id)
    }

    /// Wait for the reply to request `id` and for the kernel to go idle, and return the events since `from`.
    fn finish(&mut self, from: usize, id: &str) -> Vec<Event> {
        let reply = self.wait_from(
            from,
            RUN_LIMIT,
            |event| matches!(event, Event::ExecuteReply { request: Some(r), .. } if r == id),
        );
        assert!(
            reply.is_some(),
            "no reply to {id}: {:?}\n{}",
            &self.log[from..],
            self.kernel.stderr_tail()
        );
        let idle = self.wait_until(from, RUN_LIMIT, idle_after_busy);
        assert!(idle, "the kernel never went idle after {id}");
        self.log[from..].to_vec()
    }

    /// Wait for the one event that answers `id`.
    fn answer(&mut self, id: &str) -> Event {
        let found = self.wait_for(RUN_LIMIT, |event| answers(event, id));
        found.unwrap_or_else(|| panic!("no answer to {id}: {:?}", self.log))
    }
}

/// Whether an event is the answer to the question with this request id.
fn answers(event: &Event, id: &str) -> bool {
    let request = match event {
        Event::CompleteReply { request, .. }
        | Event::InspectReply { request, .. }
        | Event::IsCompleteReply { request, .. }
        | Event::Variables { request, .. }
        | Event::KernelSpecs { request, .. }
        | Event::Error { request, .. } => request,
        _ => return false,
    };
    request.as_deref() == Some(id)
}

/// The plain text of a mime bundle.
fn plain(data: &Value) -> String {
    data["text/plain"].as_str().unwrap_or_default().to_owned()
}

/// Whether an operating system process with this id is running.
fn process_exists(pid: u32) -> bool {
    if cfg!(windows) {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .expect("tasklist runs");
        String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
    } else {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

/// Wait until none of the processes exist, up to `limit`.
fn wait_until_gone(pids: &[u32], limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if pids.iter().all(|pid| !process_exists(*pid)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    pids.iter().all(|pid| !process_exists(*pid))
}

#[test]
fn a_started_kernel_reports_python_as_its_language() {
    let Some(session) = Session::start() else { return };
    let language = session.kernel.language().expect("language is known once started");
    assert_eq!(language.name, "python");
    assert!(language.version.starts_with('3'), "{language:?}");
    assert_eq!(language.file_extension, ".py");
    assert_eq!(session.kernel.state(), &KernelState::Idle);
    assert!(session.kernel.info().is_some());
    assert!(session.kernel.kernel_pid().is_some());
}

#[test]
fn an_expression_gives_an_execute_result_and_an_ok_reply_numbered_one() {
    let Some(mut session) = Session::start() else { return };
    let events = session.run("1+1");
    let result = events.iter().find_map(|event| match event {
        Event::ExecuteResult { data, execution_count, .. } => Some((plain(data), *execution_count)),
        _ => None,
    });
    assert_eq!(result, Some(("2".to_owned(), Some(1))));
    let reply = events.iter().any(|event| matches!(event, Event::ExecuteReply { status, execution_count: Some(1), .. } if status == "ok"));
    assert!(reply, "{events:?}");
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::ExecuteInput { execution_count: Some(1), .. })));
}

#[test]
fn print_gives_a_stdout_stream() {
    let Some(mut session) = Session::start() else { return };
    let events = session.run("print('hello')");
    assert!(events.iter().any(|event| matches!(event, Event::Stream { name, text, .. } if name == "stdout" && text == "hello\n")), "{events:?}");
}

#[test]
fn writing_to_stderr_gives_a_stderr_stream() {
    let Some(mut session) = Session::start() else { return };
    let events = session.run("import sys; sys.stderr.write('e')");
    assert!(events.iter().any(|event| matches!(event, Event::Stream { name, text, .. } if name == "stderr" && text == "e")), "{events:?}");
}

#[test]
fn dividing_by_zero_gives_an_error_with_a_traceback_and_an_error_reply() {
    let Some(mut session) = Session::start() else { return };
    let events = session.run("1/0");
    let error = events.iter().find_map(|event| match event {
        Event::Error { ename, traceback, .. } => Some((ename.clone(), traceback.len())),
        _ => None,
    });
    let (name, lines) = error.expect("an error event");
    assert_eq!(name, "ZeroDivisionError");
    assert!(lines > 0);
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::ExecuteReply { status, .. } if status == "error")));
}

#[test]
fn a_matplotlib_plot_gives_a_png_display() {
    let Some(mut session) = Session::start() else { return };
    let setup = session.run("%matplotlib inline\nimport matplotlib.pyplot as plt");
    if setup.iter().any(|event| matches!(event, Event::Error { .. })) {
        println!("matplotlib is not installed, so this test did nothing");
        return;
    }
    let events = session.run("plt.plot([1,2]); plt.show()");
    let png = events.iter().any(
        |event| matches!(event, Event::DisplayData { data, .. } if data["image/png"].is_string()),
    );
    assert!(png, "{events:?}");
}

#[test]
fn a_pandas_table_gives_html() {
    let Some(mut session) = Session::start() else { return };
    let events = session.run("import pandas as pd\npd.DataFrame({'a': [1, 2]})");
    if events
        .iter()
        .any(|event| matches!(event, Event::Error { ename, .. } if ename == "ModuleNotFoundError"))
    {
        println!("pandas is not installed, so this test did nothing");
        return;
    }
    let html = events.iter().any(
        |event| matches!(event, Event::ExecuteResult { data, .. } if data["text/html"].is_string()),
    );
    assert!(html, "{events:?}");
}

#[test]
fn updating_a_display_by_its_id_gives_an_update_event() {
    let Some(mut session) = Session::start() else { return };
    let events = session.run("from IPython.display import display\nh = display('first', display_id=True)\nh.update('second')");
    let shown = events.iter().find_map(|event| match event {
        Event::DisplayData { display_id: Some(id), .. } => Some(id.clone()),
        _ => None,
    });
    let updated = events.iter().find_map(|event| match event {
        Event::UpdateDisplayData { display_id, data, .. } => {
            Some((display_id.clone(), plain(data)))
        }
        _ => None,
    });
    let (id, text) = updated.expect("an update event");
    assert_eq!(shown, Some(id));
    assert_eq!(text, "'second'");
}

#[test]
fn clearing_the_output_gives_a_clear_output_event() {
    let Some(mut session) = Session::start() else { return };
    let events = session
        .run("from IPython.display import clear_output\nprint('x')\nclear_output(wait=True)");
    assert!(
        events.iter().any(|event| matches!(event, Event::ClearOutput { wait: true, .. })),
        "{events:?}"
    );
}

#[test]
fn input_asks_for_an_answer_and_uses_the_reply() {
    let Some(mut session) = Session::start() else { return };
    let from = session.log.len();
    let id = session.kernel.execute("print('hi', input('name? '))");
    let asked = session.wait_for(RUN_LIMIT, |event| matches!(event, Event::InputRequest { .. }));
    assert!(
        matches!(asked, Some(Event::InputRequest { prompt, password: false, .. }) if prompt == "name? ")
    );
    session.kernel.input_reply("bob");
    let events = session.finish(from, &id);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Stream { text, .. } if text == "hi bob\n")),
        "{events:?}"
    );
}

#[test]
fn interrupting_a_long_sleep_ends_it_with_a_keyboard_interrupt() {
    let Some(mut session) = Session::start() else { return };
    let from = session.log.len();
    // Thirty seconds of short sleeps, not one `time.sleep(30)`. On Windows ipykernel 7.4 delivers the
    // interrupt only when Python next runs a bytecode, and a single long sleep never wakes for it.
    let id = session
        .kernel
        .execute("import time\nprint('ready', flush=True)\nfor _ in range(300): time.sleep(0.1)");
    let running = session.wait_for(
        RUN_LIMIT,
        |event| matches!(event, Event::Stream { text, .. } if text == "ready\n"),
    );
    assert!(running.is_some());
    let asked = Instant::now();
    session.kernel.interrupt();
    let error = session.wait_from(
        from,
        Duration::from_secs(10),
        |event| matches!(event, Event::Error { ename, .. } if ename == "KeyboardInterrupt"),
    );
    assert!(error.is_some(), "no KeyboardInterrupt: {:?}", &session.log[from..]);
    println!("the interrupt took {:?}", asked.elapsed());
    session.finish(from, &id);
    // A kernel aborts a cell sent right after an error, so one that is aborted is sent again.
    let mut events = session.run("2+2");
    for _ in 0..3 {
        if !events
            .iter()
            .any(|event| matches!(event, Event::ExecuteReply { status, .. } if status == "aborted"))
        {
            break;
        }
        events = session.run("2+2");
    }
    let four = events
        .iter()
        .any(|event| matches!(event, Event::ExecuteResult { data, .. } if plain(data) == "4"));
    assert!(four, "{events:?}");
}

#[test]
fn restarting_forgets_the_variables() {
    let Some(mut session) = Session::start() else { return };
    session.run("x = 5");
    let first_pid = session.kernel.kernel_pid();
    session.kernel.restart();
    assert_eq!(session.kernel.state(), &KernelState::Restarting);
    let restarted = session.wait_for(START_LIMIT, |event| matches!(event, Event::Restarted { .. }));
    assert!(restarted.is_some(), "{:?}\n{}", session.log, session.kernel.stderr_tail());
    session.kernel.events();
    assert_eq!(session.kernel.state(), &KernelState::Idle);
    assert_ne!(session.kernel.kernel_pid(), first_pid);
    let events = session.run("x");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Error { ename, .. } if ename == "NameError")),
        "{events:?}"
    );
}

#[test]
fn completing_impo_offers_import() {
    let Some(mut session) = Session::start() else { return };
    let id = session.kernel.complete("impo", 4);
    match session.answer(&id) {
        Event::CompleteReply { matches, cursor_start, cursor_end, .. } => {
            assert!(matches.iter().any(|name| name == "import"), "{matches:?}");
            assert_eq!((cursor_start, cursor_end), (0, 4));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn inspecting_len_finds_documentation() {
    let Some(mut session) = Session::start() else { return };
    let id = session.kernel.inspect("len", 3, 0);
    match session.answer(&id) {
        Event::InspectReply { found, data, .. } => {
            assert!(found);
            assert!(plain(&data).contains("len"), "{data}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_kernel_says_whether_code_is_complete() {
    let Some(mut session) = Session::start() else { return };
    let id = session.kernel.is_complete("for i in range(3):");
    assert!(
        matches!(session.answer(&id), Event::IsCompleteReply { status, .. } if status == "incomplete")
    );
    let id = session.kernel.is_complete("1 + 1");
    assert!(
        matches!(session.answer(&id), Event::IsCompleteReply { status, .. } if status == "complete")
    );
}

#[test]
fn variables_lists_the_users_names_with_their_sizes_and_shapes() {
    let Some(mut session) = Session::start() else { return };
    session.run("import os\nx = [1,2,3]\n_hidden = 1");
    let numpy = session.run("import numpy as np\na = np.zeros((3, 2))");
    let id = session.kernel.variables();
    let Event::Variables { rows, .. } = session.answer(&id) else { panic!("not variables") };
    let x = rows.iter().find(|row| row.name == "x").expect("x is listed");
    assert_eq!((x.type_name.as_str(), x.value.as_str(), x.size), ("list", "[1, 2, 3]", Some(3)));
    assert!(
        rows.iter().all(|row| row.name != "os" && row.name != "_hidden" && row.name != "In"),
        "{rows:?}"
    );
    if !numpy.iter().any(|event| matches!(event, Event::Error { .. })) {
        let a = rows.iter().find(|row| row.name == "a").expect("a is listed");
        assert_eq!(a.shape.as_deref(), Some("(3, 2)"));
    }
    assert_eq!(session.kernel.state(), &KernelState::Idle);
}

#[test]
fn kernelspecs_lists_python3() {
    let Some(mut session) = Session::start() else { return };
    let id = session.kernel.kernelspecs();
    let Event::KernelSpecs { specs, .. } = session.answer(&id) else { panic!("not kernelspecs") };
    assert!(
        specs.iter().any(|spec| spec.name == "python3" && spec.language == "python"),
        "{specs:?}"
    );
    let before_start = list_kernelspecs(&session.python).expect("specs without a bridge");
    assert!(before_start.iter().any(|spec| spec.name == "python3"));
}

#[test]
fn shutting_down_gives_stopped_and_leaves_no_process() {
    let Some(mut session) = Session::start() else { return };
    let pids = [session.kernel.kernel_pid().expect("kernel pid"), session.kernel.bridge_pid()];
    assert!(pids.iter().all(|pid| process_exists(*pid)));
    session.kernel.shutdown();
    let stopped =
        session.wait_for(Duration::from_secs(20), |event| matches!(event, Event::Stopped));
    assert!(stopped.is_some(), "{:?}", session.log);
    session.kernel.events();
    assert_eq!(session.kernel.state(), &KernelState::Stopped);
    assert!(
        wait_until_gone(&pids, Duration::from_secs(10)),
        "a process is still running: {pids:?}"
    );
}

#[test]
fn dropping_a_kernel_without_shutting_it_down_leaves_no_process() {
    let Some(session) = Session::start() else { return };
    let pids = [session.kernel.kernel_pid().expect("kernel pid"), session.kernel.bridge_pid()];
    drop(session);
    assert!(
        wait_until_gone(&pids, Duration::from_secs(10)),
        "a process is still running: {pids:?}"
    );
}

#[test]
fn a_kernel_that_is_killed_from_outside_gives_died() {
    let Some(mut session) = Session::start() else { return };
    let pid = session.kernel.kernel_pid().expect("kernel pid");
    if cfg!(windows) {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .stdout(Stdio::null())
            .status()
            .expect("taskkill runs");
    } else {
        Command::new("kill").args(["-9", &pid.to_string()]).status().expect("kill runs");
    }
    let died =
        session.wait_for(Duration::from_secs(15), |event| matches!(event, Event::Died { .. }));
    assert!(died.is_some(), "{:?}", session.log);
    session.kernel.events();
    assert!(matches!(session.kernel.state(), KernelState::Dead(_)));
}

#[test]
fn a_kernel_that_is_not_installed_gives_failed() {
    let Some(python) = test_python() else {
        println!("no Python with ipykernel and jupyter_client was found, so this test did nothing");
        return;
    };
    let mut session = Session::unstarted(&python, Some("no-such-kernel-installed"));
    let failed = session.wait_for(START_LIMIT, |event| matches!(event, Event::Failed { .. }));
    assert!(
        matches!(failed, Some(Event::Failed { message, missing: None }) if message.contains("no-such-kernel-installed"))
    );
    session.kernel.events();
    assert!(matches!(session.kernel.state(), KernelState::Dead(_)));
}

#[test]
fn the_install_command_runs_pip_for_that_python() {
    let command = install_command(std::path::Path::new("python"));
    assert_eq!(command, ["python", "-m", "pip", "install", "ipykernel"]);
}

#[test]
fn finding_pythons_probes_each_candidate_once() {
    let found = find_pythons(None);
    let mut paths: Vec<_> =
        found.iter().filter_map(|python| python.path.canonicalize().ok()).collect();
    let count = paths.len();
    paths.sort();
    paths.dedup();
    assert_eq!(count, paths.len(), "a Python was listed twice: {found:?}");
    assert!(found.iter().all(|python| !python.version.is_empty() && !python.found_by.is_empty()));
}

/// How many kernels run at once. Some machines start a kernel slowly, and thirty starting together
/// can each take longer than the time a test allows.
const KERNELS_AT_ONCE: usize = 2;

/// One of the places for a running kernel. Given back when dropped.
struct Place;

impl Place {
    /// Wait for a free place and take it.
    fn take() -> Place {
        let (count, free) = &**places();
        let mut held = count.lock().unwrap();
        while *held >= KERNELS_AT_ONCE {
            held = free.wait(held).unwrap();
        }
        *held += 1;
        Place
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        let (count, free) = &**places();
        *count.lock().unwrap() -= 1;
        free.notify_one();
    }
}

/// The count of places in use, shared by every test in this file.
fn places() -> &'static Arc<(Mutex<usize>, Condvar)> {
    static PLACES: std::sync::OnceLock<Arc<(Mutex<usize>, Condvar)>> = std::sync::OnceLock::new();
    PLACES.get_or_init(|| Arc::new((Mutex::new(0), Condvar::new())))
}

/// Whether the kernel went busy and then idle again. An idle report from an earlier cell that arrives
/// late does not count, because it comes before this cell's busy report.
fn idle_after_busy(events: &[Event]) -> bool {
    let busy =
        events.iter().position(|event| matches!(event, Event::Status { state } if state == "busy"));
    busy.is_some_and(|at| {
        events[at..].iter().any(|event| matches!(event, Event::Status { state } if state == "idle"))
    })
}
