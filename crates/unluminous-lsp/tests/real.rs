//! The client against the real rust-analyzer and tsserver.
//!
//! With `UNLUMINOUS_REQUIRE_SERVERS=1` a server that cannot be found fails the test. Without it the test
//! prints one line and passes, so a machine with no TypeScript still has a green suite. rust-analyzer can
//! take a minute to index the first time, so a question is asked again until the answer holds what is
//! expected or the time runs out.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use unluminous_lsp::{
    find_program, Adapter, Item, Reply, Server, ServerSpec, ServerState, Trigger,
};

const PATIENCE: Duration = Duration::from_secs(180);

/// True when a missing server is a failure.
fn required() -> bool {
    std::env::var("UNLUMINOUS_REQUIRE_SERVERS").is_ok_and(|v| v == "1")
}

/// A fresh folder for a project the server will read.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("unluminous-lsp-real-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let at = root.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }
    root
}

/// Polls for the reply to one ticket.
fn await_reply<T>(
    server: &mut Server,
    wait: Duration,
    pick: impl Fn(&Reply) -> Option<T>,
) -> Option<T> {
    let started = Instant::now();
    while started.elapsed() < wait {
        for reply in server.poll() {
            if let Some(found) = pick(&reply) {
                return Some(found);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

/// Asks for completions again and again until every wanted label is among them.
fn complete_until(
    server: &mut Server,
    path: &Path,
    revision: u64,
    offset: usize,
    trigger: Trigger,
    wanted: &[&str],
) -> Vec<Item> {
    let started = Instant::now();
    let mut last = Vec::new();
    while started.elapsed() < PATIENCE {
        let ticket = server.complete(path, revision, offset, trigger);
        let items = await_reply(server, Duration::from_secs(15), |r| match r {
            Reply::Completions { ticket: t, items, .. } if *t == ticket => Some(items.clone()),
            _ => None,
        });
        if let Some(items) = items {
            if wanted.iter().all(|w| items.iter().any(|i| i.label == *w)) {
                println!(
                    "  {:?} answered {} rows after {:.1}s, state {}",
                    wanted,
                    items.len(),
                    started.elapsed().as_secs_f32(),
                    server.state().describe()
                );
                return items;
            }
            last = items;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let labels: Vec<&str> = last.iter().map(|i| i.label.as_str()).take(40).collect();
    panic!(
        "no answer holding {wanted:?} in {PATIENCE:?}; last rows {labels:?}; state {:?}",
        server.state()
    );
}

/// Finds the program, or says the test cannot run (and fails when servers are required).
fn spec_or_skip(adapter: Adapter, root: &Path, file: &Path) -> Option<ServerSpec> {
    match find_program(adapter, root, file, None, &[]) {
        Ok(spec) => Some(spec),
        Err(why) if adapter == Adapter::TsServer => fall_back_to_a_known_typescript(root, &why),
        Err(why) => {
            assert!(!required(), "{} not found: {why}", adapter.name());
            println!("skipped: {} not found ({why})", adapter.name());
            None
        }
    }
}

/// A TypeScript that this machine is known to have, for a project folder with no `node_modules` of its own.
fn fall_back_to_a_known_typescript(root: &Path, why: &str) -> Option<ServerSpec> {
    let known = std::env::var("UNLUMINOUS_TEST_TSSERVER").unwrap_or_else(|_| {
        "C:/jason/dev/ai-service/ui/node_modules/typescript/lib/tsserver.js".to_owned()
    });
    if !Path::new(&known).is_file() {
        assert!(!required(), "tsserver not found: {why}");
        println!("skipped: tsserver not found ({why})");
        return None;
    }
    let args =
        [known.as_str(), "--disableAutomaticTypingAcquisition", "--suppressDiagnosticEvents"]
            .map(String::from)
            .to_vec();
    Some(ServerSpec {
        adapter: Adapter::TsServer,
        root: root.to_path_buf(),
        program: PathBuf::from("node"),
        args,
        label: "tsserver".to_owned(),
    })
}

const LIB_ON_DISK: &str = "pub mod other;\n\npub struct Layout { pub width: f32 }\n\nimpl Layout {\n    pub fn caret_at(&self) -> f32 { 0.0 }\n    pub fn new(width: f32) -> Self { Layout { width } }\n    pub fn probe(&self) -> f32 { self.width }\n    pub fn build() -> Layout { Layout::new(1.0) }\n}\n";

#[test]
fn rust_analyzer_answers_members_and_signature_help() {
    let root = project("ra", &[
        ("Cargo.toml", "[package]\nname = \"realcrate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n"),
        ("src/lib.rs", LIB_ON_DISK),
        ("src/other.rs", "use crate::Layout;\n\npub fn make() -> Layout { Layout::new(2.0) }\n"),
    ]);
    let lib = root.join("src/lib.rs");
    let Some(spec) = spec_or_skip(Adapter::Lsp, &root, &lib) else { return };
    println!("rust-analyzer: {}", spec.program.display());
    let started = Instant::now();
    let mut server = Server::start(spec, Arc::new(|| {}));
    let other = root.join("src/other.rs");
    server.sync(&other, "rust", 1, Arc::from(std::fs::read_to_string(&other).unwrap().as_str()));
    let typing = LIB_ON_DISK.replace("{ self.width }", "{ self. }");
    server.sync(&lib, "rust", 1, Arc::from(typing.as_str()));
    let at = typing.find("self.").unwrap() + 5;
    let items =
        complete_until(&mut server, &lib, 1, at, Trigger::Character('.'), &["caret_at", "width"]);
    let caret_at = items.iter().find(|i| i.label == "caret_at").unwrap();
    assert!(caret_at.insertion.is_some());
    assert!(
        caret_at.callable,
        "a method whose insertion adds its brackets: {:?}",
        caret_at.insertion
    );
    println!("  members after {:.1}s", started.elapsed().as_secs_f32());

    let signature_at = typing.find("Layout::new(").unwrap() + "Layout::new(".len();
    let help = loop {
        let ticket = server.signature(&lib, 1, signature_at);
        let found = await_reply(&mut server, Duration::from_secs(15), |r| match r {
            Reply::Signature { ticket: t, help, .. } if *t == ticket => Some(help.clone()),
            _ => None,
        });
        if let Some(Some(help)) = found {
            break help;
        }
        assert!(started.elapsed() < PATIENCE, "no signature help; state {:?}", server.state());
        std::thread::sleep(Duration::from_secs(1));
    };
    println!("  signature: {} {:?} active {:?}", help.label, help.parameters, help.active);
    assert!(help.label.contains("new"), "{}", help.label);
    assert_eq!(help.parameters.len(), 1);
    assert_eq!(&help.label[help.parameters[0].clone()], "width: f32");
    assert_eq!(help.active, Some(0));
    assert!(!matches!(server.state(), ServerState::Failed(_)));
    let stopping = Instant::now();
    server.stop();
    println!("  stopped in {:.2}s", stopping.elapsed().as_secs_f32());
    let _ = std::fs::remove_dir_all(&root);
}

const A_TS: &str = "export interface Card { title: string; count: number }\nexport function makeCard(title: string): Card { return { title, count: 1 }; }\n";

#[test]
fn tsserver_answers_members_and_an_import_edit() {
    let b_disk =
        "import { Card, makeCard } from \"./a\";\nconst c: Card = makeCard(\"x\");\nc.title;\n";
    let root = project("ts", &[
        ("tsconfig.json", "{\"compilerOptions\": {\"strict\": true, \"target\": \"es2020\", \"module\": \"esnext\", \"moduleResolution\": \"bundler\"}, \"include\": [\"*.ts\"]}"),
        ("a.ts", A_TS),
        ("b.ts", b_disk),
        ("c.ts", "export const unused = 1;\n"),
    ]);
    let b = root.join("b.ts");
    let Some(spec) = spec_or_skip(Adapter::TsServer, &root, &b) else { return };
    println!("tsserver: {} {}", spec.program.display(), spec.args[0]);
    let started = Instant::now();
    let mut server = Server::start(spec, Arc::new(|| {}));
    server.sync(&root.join("a.ts"), "typescript", 1, Arc::from(A_TS));
    let typing = b_disk.replace("c.title;", "c.");
    server.sync(&b, "typescript", 1, Arc::from(typing.as_str()));
    let at = typing.len();
    let items =
        complete_until(&mut server, &b, 1, at, Trigger::Character('.'), &["title", "count"]);
    let title = items.iter().find(|i| i.label == "title").unwrap();
    println!(
        "  title: kind {:?} order {} insertion {:?}",
        title.kind, title.order, title.insertion
    );
    println!("  members after {:.1}s", started.elapsed().as_secs_f32());

    let c = root.join("c.ts");
    let source = "const made = makeCar";
    server.sync(&c, "typescript", 1, Arc::from(source));
    let items = complete_until(&mut server, &c, 1, source.len(), Trigger::Invoked, &["makeCard"]);
    let make_card = items.iter().find(|i| i.label == "makeCard").unwrap();
    assert!(make_card.needs_resolve);
    let ticket = server.resolve(&c, make_card);
    let resolved = await_reply(&mut server, Duration::from_secs(30), |r| match r {
        Reply::Resolved { ticket: t, item } if *t == ticket => Some(item.clone()),
        _ => None,
    })
    .expect("no resolve reply");
    println!("  import edits: {:?}; detail {:?}", resolved.extra_edits, resolved.detail);
    assert!(
        resolved
            .extra_edits
            .iter()
            .any(|e| e.text.contains("import") && e.text.contains("makeCard")),
        "{:?}",
        resolved.extra_edits
    );
    assert!(!matches!(server.state(), ServerState::Failed(_)));
    let stopping = Instant::now();
    server.stop();
    println!("  stopped in {:.2}s", stopping.elapsed().as_secs_f32());
    let _ = std::fs::remove_dir_all(&root);
}
