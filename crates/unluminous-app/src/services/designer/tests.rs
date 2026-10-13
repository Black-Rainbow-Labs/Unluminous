//! The Designer's unit tests: what is a design, the notes file, the copies, the words sent to the agent, the
//! page's reports, and every verb of the command line. No window and no browser: the provider is opened on a
//! temporary project the way the released binary opens it on the person's own.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::config::{checked_folder, Configuration, Device, Starter};
use super::notes::{Anchor, NoteState, NotesFile};
use super::page::{self, Element, Mode, Report};
use super::{history, library, prompt, Designer};
use crate::services::plugin_ui::{Context, Request, UiProvider};
use crate::services::store::Values;

/// A project folder of its own, empty, under the temporary folder.
fn a_project(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("unluminous-designer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a project");
    folder
}

/// The Designer opened on a project, with a folder of its own for its settings.
fn a_designer(name: &str) -> (Designer, PathBuf) {
    let project = a_project(name);
    let mut designer = Designer::new();
    let context = Context {
        project: Some(project.clone()),
        folder: Some(project.join("plugin-settings")),
        ..Context::default()
    };
    designer.open(&context).expect("opens");
    (designer, project)
}

/// A designer with one design in it, showing.
fn with_a_design(name: &str) -> (Designer, PathBuf, PathBuf) {
    let (mut designer, project) = a_designer(name);
    let design = designer.make_a_design("landing").expect("made");
    designer.show(&design);
    // Making it opened it in a tab and asked the window for the pane, which is not what these tests are about.
    designer.asking();
    (designer, project, design)
}

/// Write a fixture file, through the one way a file is written here.
fn put(path: impl AsRef<Path>, text: impl AsRef<[u8]>) -> std::io::Result<()> {
    crate::services::store::write_atomically(path.as_ref(), text.as_ref())
}

fn words(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

fn an_element(selector: &str) -> Element {
    Element {
        selector: selector.to_owned(),
        tag: "h1.hero-title".to_owned(),
        excerpt: "Ship designs with your agent".to_owned(),
        html: "<h1 class=\"hero-title\">Ship designs with your agent</h1>".to_owned(),
        styles: vec![
            ("font-size".to_owned(), "64px".to_owned()),
            ("color".to_owned(), "rgb(17, 20, 26)".to_owned()),
        ],
        rect: page::PageRect { x: 100.0, y: 200.0, w: 400.0, h: 80.0 },
    }
}

// ------------------------------------------------------------------------------------ configuration

#[test]
fn the_defaults_are_the_ones_the_prd_names() {
    let config = Configuration::default();
    assert_eq!(config.folder, PathBuf::from(".designs"));
    assert_eq!(config.device, Device::Fit);
    assert_eq!(config.starter, Starter::House);
    assert!(config.send_html);
    assert_eq!(config.history_kept, 20);
}

#[test]
fn a_folder_outside_the_project_is_refused_with_a_sentence_and_the_default_is_kept() {
    for wrong in ["C:/elsewhere", "/etc", "../up", "a/../../b", ""] {
        assert!(checked_folder(wrong).is_err(), "{wrong} should be refused");
    }
    let (config, refused) = Configuration::of(&Values::parse(
        "folder = ../outside\ndevice = phone\nhistory_kept = 3\n",
    ));
    assert_eq!(config.folder, PathBuf::from(".designs"));
    assert_eq!(config.device, Device::Phone, "what is right is still read");
    assert_eq!(config.history_kept, 3);
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(refused[0].contains("inside the project"), "{refused:?}");
}

#[test]
fn a_device_or_starter_this_version_has_not_got_is_refused_naming_the_choices() {
    let (config, refused) =
        Configuration::of(&Values::parse("device = watch\nstarter = fancy\nhistory_kept = 9999\n"));
    assert_eq!(config.device, Device::Fit);
    assert_eq!(config.history_kept, 200, "the most a person can ask for");
    assert!(
        refused.iter().any(|why| why.contains("desktop, laptop, tablet, phone, fit")),
        "{refused:?}"
    );
    assert!(refused.iter().any(|why| why.contains("house, blank")), "{refused:?}");
}

#[test]
fn the_configuration_is_written_to_the_plugins_own_file_and_read_back() {
    let folder = a_project("config-round-trip");
    let config = Configuration {
        device: Device::Tablet,
        send_html: false,
        folder: PathBuf::from("design/pages"),
        ..Configuration::default()
    };
    config.write(&folder).expect("written");
    let (read, refused) = Configuration::read(&folder);
    assert!(refused.is_empty(), "{refused:?}");
    assert_eq!(read, config);
}

// ----------------------------------------------------------------------------------------- library

#[test]
fn which_files_are_designs_for_the_default_folder_and_a_configured_one() {
    let project = Path::new("C:/project");
    let default = Path::new(".designs");
    assert!(library::is_a_design(project, default, Path::new(".designs/landing.html")));
    assert!(library::is_a_design(project, default, Path::new(".designs/pages/sign-up.HTM")));
    assert!(library::is_a_design(project, default, &project.join(".designs/landing.html")));
    assert!(
        !library::is_a_design(project, default, Path::new("index.html")),
        "an HTML file elsewhere is not one"
    );
    assert!(!library::is_a_design(project, default, Path::new(".designs/landing.css")));
    assert!(!library::is_a_design(project, default, Path::new(".designs")));
    assert!(!library::is_a_design(project, default, Path::new(".designs-old/landing.html")));
    assert!(library::is_a_design(project, Path::new("ui/mocks"), Path::new("ui/mocks/home.html")));
    assert!(!library::is_a_design(
        project,
        Path::new("ui/mocks"),
        Path::new(".designs/landing.html")
    ));
}

#[test]
fn a_new_design_is_the_starter_page_and_a_name_that_exists_or_holds_a_slash_is_refused() {
    let project = a_project("library-new");
    let folder = Path::new(".designs");
    let made = library::make(&project, folder, "Landing Page", Starter::House).expect("made");
    assert_eq!(made, PathBuf::from(".designs/Landing Page.html"));
    let text = std::fs::read_to_string(project.join(&made)).expect("read");
    assert!(text.contains("hero-title") && text.contains("--accent"), "the house starter");
    assert!(library::make(&project, folder, "Landing Page", Starter::House)
        .unwrap_err()
        .contains("already"));
    assert!(library::make(&project, folder, "a/b", Starter::House).unwrap_err().contains("slash"));
    assert!(library::make(&project, folder, "  ", Starter::House).is_err());
    let blank = library::make(&project, folder, "empty.html", Starter::Blank).expect("blank");
    assert!(!std::fs::read_to_string(project.join(blank)).unwrap().contains("hero"));
}

#[test]
fn designs_are_listed_below_the_folder_and_found_by_the_names_a_person_types() {
    let project = a_project("library-list");
    let folder = Path::new(".designs");
    library::make(&project, folder, "landing", Starter::Blank).unwrap();
    std::fs::create_dir_all(project.join(".designs/pages")).unwrap();
    put(project.join(".designs/pages/pricing.html"), "<p>x</p>").unwrap();
    put(project.join(".designs/notes.txt"), "not a design").unwrap();
    let listed: Vec<String> =
        library::list(&project, folder).iter().map(|d| library::name_of(folder, d)).collect();
    assert_eq!(listed, ["landing.html", "pages/pricing.html"]);
    for name in
        ["landing", "landing.html", ".designs/landing.html", "pages/pricing", "PAGES/pricing.html"]
    {
        if name.starts_with("PAGES") {
            continue;
        }
        assert!(library::find(&project, folder, name).is_ok(), "{name}");
    }
    let refusal = library::find(&project, folder, "nothing").unwrap_err();
    assert!(refusal.contains("landing.html"), "the refusal lists the designs: {refusal}");
}

// ------------------------------------------------------------------------------------------ notes

#[test]
fn a_notes_file_round_trips_and_keeps_the_fields_it_does_not_know() {
    let text = r#"{
      "version": 1, "design": "landing.html", "next": 4, "colour": "kept",
      "notes": [{
        "id": 3, "state": "open", "extra_note_field": [1, 2],
        "anchor": { "selector": "h1", "tag": "h1.hero", "excerpt": "Hi", "fx": 0.9, "fy": 0.1,
                    "page": { "x": 612.0, "y": 188.0 }, "device": "desktop", "pinned_by": "x" },
        "messages": [{ "author": "you", "body": "Too loud.", "at": "2026-10-12T23:40:12Z", "edited": true }],
        "created": "2026-10-12T23:40:12Z", "resolved": null
      }]
    }"#;
    let file = NotesFile::parse(text, "landing.html").expect("reads");
    assert_eq!(file.next, 4);
    assert_eq!(file.notes[0].anchor.fx, 0.9);
    let written = file.text();
    for kept in ["colour", "extra_note_field", "pinned_by", "edited"] {
        assert!(written.contains(kept), "{kept} was thrown away: {written}");
    }
    assert_eq!(NotesFile::parse(&written, "").unwrap(), file, "and it reads back as the same file");
}

#[test]
fn a_note_number_is_never_used_again_after_a_delete() {
    let mut file = NotesFile::empty("landing.html");
    let first = file.add(Anchor::on("h1"), "you", "one");
    let second = file.add(Anchor::on("h2"), "you", "two");
    file.remove(second).unwrap();
    let third = file.add(Anchor::on("p"), "you", "three");
    assert_eq!((first, second, third), (1, 2, 3));
    // And a file whose `next` was behind its notes is read forward of the highest.
    let behind =
        r#"{"next": 1, "notes": [{"id": 7, "anchor": {"selector": "a"}, "messages": []}]}"#;
    assert_eq!(NotesFile::parse(behind, "x.html").unwrap().next, 8);
}

#[test]
fn nothing_is_written_when_nothing_changed_and_nothing_at_all_for_a_design_never_commented_on() {
    let project = a_project("notes-write");
    let path = project.join("landing.comments.json");
    let mut file = NotesFile::empty("landing.html");
    assert!(!file.write(&path).unwrap(), "no notes and no file is not worth making one");
    assert!(!path.exists());
    file.add(Anchor::on("h1"), "you", "Too loud.");
    assert!(file.write(&path).unwrap());
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert!(!file.write(&path).unwrap(), "the same text is not written again");
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
}

#[test]
fn a_time_is_written_as_utc_in_the_shape_the_file_uses() {
    assert_eq!(super::notes::iso_from_unix(0), "1970-01-01T00:00:00Z");
    assert_eq!(super::notes::iso_from_unix(1_760_312_412), "2025-10-12T23:40:12Z");
    assert_eq!(super::notes::iso_from_unix(951_782_400), "2000-02-29T00:00:00Z", "a leap day");
}

#[test]
fn a_file_another_program_changed_is_read_again() {
    let (mut designer, project, design) = with_a_design("notes-reread");
    let id = designer.comment(&design, "h1", "Too loud.", "you").unwrap();
    assert_eq!(designer.design_view(&design).unwrap().notes.notes.len(), 1);
    // An agent with its own tools adds a note by editing the JSON.
    let comments = project.join(library::comments_path(&design));
    let mut file = NotesFile::read(&comments, "landing.html").unwrap();
    file.add(Anchor::on("p"), "claude", "Added by hand.");
    std::thread::sleep(std::time::Duration::from_millis(30));
    put(&comments, file.text()).unwrap();
    designer.force_a_look_for_tests();
    let view = designer.design_view(&design).unwrap();
    assert_eq!(view.notes.notes.len(), 2, "the second note was seen");
    assert!(view.notes.get(id).is_some());
}

// ----------------------------------------------------------------------------------------- history

#[test]
fn a_copy_is_named_by_the_time_and_the_first_five_words_of_the_request() {
    assert_eq!(
        history::slug("Make the hero heading much darker please now"),
        "make-the-hero-heading-much"
    );
    assert_eq!(history::slug("  !!! "), "request");
    assert!(history::slug(&"word ".repeat(30)).len() <= 40);
    let (designer, project, design) = with_a_design("history-name");
    let copy =
        history::keep(&project, Path::new(".designs"), &design, "Too loud, make it calmer", 20)
            .unwrap();
    assert!(copy.name.ends_with("-too-loud-make-it-calmer.html"), "{}", copy.name);
    assert_eq!(copy.stamp.len(), 15);
    assert_eq!(copy.words, "too loud make it calmer");
    assert!(
        copy.file.starts_with(project.join(".unluminous/designs/landing")),
        "{}",
        copy.file.display()
    );
    let _ = designer;
}

#[test]
fn only_the_newest_copies_are_kept_and_older_ones_are_removed() {
    let (_, project, design) = with_a_design("history-prune");
    let folder = Path::new(".designs");
    for round in 0..5 {
        history::keep(&project, folder, &design, &format!("request {round}"), 3).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let copies = history::list(&project, folder, &design);
    assert_eq!(copies.len(), 3);
    assert!(
        copies[0].words.ends_with('4') && copies[2].words.ends_with('2'),
        "newest first: {copies:?}"
    );
    // Two in one second do not overwrite each other.
    let again = history::keep(&project, folder, &design, "request 4", 10).unwrap();
    assert!(again.file.exists());
}

#[test]
fn restoring_copies_the_current_file_first_so_a_restore_can_be_put_back() {
    let (mut designer, project, design) = with_a_design("history-restore");
    let disk = project.join(&design);
    put(&disk, "<p>first</p>").unwrap();
    history::keep(&project, Path::new(".designs"), &design, "before the agent", 20).unwrap();
    put(&disk, "<p>the agent's edit</p>").unwrap();
    let chosen = designer.restore(&design, "1").expect("restored");
    assert_eq!(std::fs::read_to_string(&disk).unwrap(), "<p>first</p>");
    let copies = history::list(&project, Path::new(".designs"), &design);
    let safety = copies
        .iter()
        .find(|copy| copy.words == "before restore")
        .expect("a copy of what was there");
    assert_eq!(std::fs::read_to_string(&safety.file).unwrap(), "<p>the agent's edit</p>");
    assert_eq!(chosen.words, "before the agent");
    assert!(designer.restore(&design, "99").is_err());
    assert!(designer.restore(&design, "zzz").unwrap_err().contains("no copy"));
}

// ------------------------------------------------------------------------------------------ prompt

fn a_note_on_a_heading() -> super::notes::Note {
    let mut file = NotesFile::empty("landing.html");
    let mut anchor = Anchor::on("main > section.hero > h1.hero-title");
    anchor.tag = "h1.hero-title".to_owned();
    file.add(anchor, "you", "Too loud. Try 48px and the ink colour.");
    file.notes.remove(0)
}

#[test]
fn the_request_for_one_note_names_the_design_the_element_the_words_and_how_to_answer() {
    let note = a_note_on_a_heading();
    let element = an_element("main > section.hero > h1.hero-title");
    let said = prompt::for_a_note(".designs/landing.html", &note, Some(&element), true);
    assert!(said.contains("Design: .designs/landing.html"), "{said}");
    assert!(
        said.contains("Note 1 on h1.hero-title, selector `main > section.hero > h1.hero-title`"),
        "{said}"
    );
    assert!(said.contains("Too loud. Try 48px and the ink colour."), "{said}");
    assert!(said.contains("<h1 class=\"hero-title\">"), "the markup goes when asked: {said}");
    assert!(said.contains("font-size 64px"), "{said}");
    assert!(
        said.contains("plugins run designer reply --design .designs/landing.html 1 "),
        "{said}"
    );
    assert!(
        said.contains("plugins run designer resolve --design .designs/landing.html 1"),
        "{said}"
    );
    let without = prompt::for_a_note(".designs/landing.html", &note, Some(&element), false);
    assert!(!without.contains("<h1"), "send_html off leaves the markup out: {without}");
    assert!(without.contains("Ship designs with your agent"), "but the excerpt stays");
}

#[test]
fn every_open_note_goes_in_one_message_numbered_with_one_reply_asked_of_each() {
    let mut file = NotesFile::empty("landing.html");
    file.add(Anchor::on("h1"), "you", "Smaller.");
    file.add(Anchor::on("p"), "you", "Greyer.");
    let pairs: Vec<(&super::notes::Note, Option<&Element>)> =
        file.notes.iter().map(|note| (note, None)).collect();
    let said = prompt::for_every_open_note("d.html", &pairs, true);
    assert!(said.contains("2 open notes"), "{said}");
    assert!(said.contains("Note 1 on") && said.contains("Note 2 on"), "{said}");
    assert!(
        said.contains("reply --design d.html <note>")
            && said.contains("resolve --design d.html <note>"),
        "{said}"
    );
}

#[test]
fn a_free_request_names_the_chosen_element_and_otherwise_the_whole_design() {
    let element = an_element("h1");
    let about_it = prompt::free("d.html", Some(&element), "  Make it darker  ", true);
    assert!(about_it.contains("About the element h1.hero-title, selector `h1`"), "{about_it}");
    assert!(about_it.contains("Request: Make it darker"), "{about_it}");
    let about_all = prompt::free("d.html", None, "Make it darker", true);
    assert!(
        about_all.contains("the whole design") && !about_all.contains("selector"),
        "{about_all}"
    );
    assert!(about_all.contains("designer comments --design d.html open"), "{about_all}");
}

// -------------------------------------------------------------------------------------------- page

#[test]
fn each_kind_of_report_is_read_and_what_is_not_one_is_ignored() {
    let json = json!({ "reports": [
        { "kind": "selected", "element": { "selector": "h1", "tag": "h1.a", "excerpt": "Hi", "html": "<h1>", "styles": { "color": "red" }, "rect": { "x": 1, "y": 2, "w": 3, "h": 4 } } },
        { "kind": "comment", "element": { "selector": "p" }, "fx": 4.0, "fy": -1.0 },
        { "kind": "pin", "id": 3 },
        { "kind": "cleared" },
        { "kind": "anchors", "found": [{ "id": 3, "x": 10.5, "y": 20 }], "missing": [4, "x", -1] },
        { "kind": "loaded", "url": "unluminous://tab-1/a.html", "scrollY": 120 },
        { "kind": "delete-everything" },
        { "kind": "pin", "id": "three" },
        { "kind": "selected", "element": { "tag": "no selector" } },
        "not an object"
    ]});
    let reports = page::read_reports(&json.to_string());
    assert_eq!(reports.len(), 6, "{reports:?}");
    assert!(
        matches!(&reports[0], Report::Selected(element) if element.selector == "h1" && element.rect.w == 3.0)
    );
    assert!(
        matches!(&reports[1], Report::Comment { fx, fy, .. } if *fx == 1.0 && *fy == 0.0),
        "fractions are kept inside the box"
    );
    assert_eq!(reports[2], Report::Pin(3));
    assert_eq!(reports[3], Report::Cleared);
    assert!(
        matches!(&reports[4], Report::Anchors { found, missing } if found.len() == 1 && missing == &vec![4])
    );
    assert!(matches!(&reports[5], Report::Loaded { scroll_y, .. } if *scroll_y == 120.0));
    assert!(page::read_reports("not json").is_empty());
    assert!(page::read_reports("{\"reports\": 5}").is_empty());
}

#[test]
fn a_page_cannot_make_the_host_hold_more_than_the_lengths_it_states() {
    let long = "x".repeat(5000);
    let json = json!({ "reports": [{ "kind": "selected", "element": {
        "selector": long, "tag": long, "excerpt": long, "html": long, "styles": { "color": long }, "rect": {} } }]});
    let Report::Selected(element) = &page::read_reports(&json.to_string())[0] else { panic!() };
    assert_eq!(element.selector.len(), 600);
    assert_eq!(element.tag.len(), 80);
    assert_eq!(element.excerpt.len(), 120);
    assert_eq!(element.html.len(), 1200);
    assert_eq!(element.styles[0].1.len(), 200);
}

#[test]
fn a_selector_with_a_quote_and_a_backslash_in_it_is_quoted_into_the_script_correctly() {
    let selector = r#"a[title="it's"] > .x\:y"#;
    let script = page::select(selector);
    let start = script.find("select(").unwrap() + "select(".len();
    let argument = &script[start..script.rfind(");").unwrap()];
    let back: String = serde_json::from_str(argument).expect("a string literal");
    assert_eq!(back, selector, "{script}");
    assert_eq!(
        page::set_mode(Mode::Comment),
        "window.__unluminousDesigner && window.__unluminousDesigner.setMode(\"comment\");"
    );
    let pins = page::set_pins(&[page::PinSpec {
        id: 3,
        selector: "h1".into(),
        fx: 0.5,
        fy: 0.0,
        page: (1.0, 2.0),
        active: true,
        detached: false,
    }]);
    assert!(pins.contains("\"selector\":\"h1\"") && pins.contains("\"active\":true"), "{pins}");
}

#[test]
fn the_overlay_is_told_to_undo_the_page_zoom_and_a_scale_that_is_not_a_size_is_one() {
    assert_eq!(
        page::set_scale(1.5),
        "window.__unluminousDesigner && window.__unluminousDesigner.setScale(1.5000);"
    );
    assert!(page::set_scale(f64::NAN).contains("setScale(1.0000)"));
    assert!(page::set_scale(-2.0).contains("setScale(1.0000)"));
    assert!(page::DESIGNER_SCRIPT.contains("setScale,"), "designer.js hands setScale to the host");
}

#[test]
fn the_script_the_host_keeps_in_the_page_defines_the_overlay_and_the_picker_it_leans_on() {
    let watch = page::watch();
    assert!(
        watch.install.contains("__unluminousSelectorFor"),
        "the picker's selector builder is exposed"
    );
    assert!(
        watch.install.contains("attachShadow({ mode: 'closed' })"),
        "the overlay is in a closed shadow root"
    );
    assert!(
        watch.install.find("__unluminousPicker").unwrap()
            < watch.install.find("__unluminousDesigner =").unwrap(),
        "the picker script comes first"
    );
    assert!(watch.take.contains(".take()"));
    assert!(
        !watch.install.contains("window.chrome") && !watch.install.contains("postMessage"),
        "no bridge"
    );
}

// ------------------------------------------------------------------------------------- the provider

#[test]
fn comment_reply_resolve_and_reopen_change_the_file_the_way_the_buttons_do() {
    let (mut designer, project, design) = with_a_design("commands-notes");
    let made = designer.command("comment", &words("h1.hero-title Too loud, try 48px")).unwrap();
    assert_eq!(made.value["note"], 1);
    let comments = project.join(library::comments_path(&design));
    let on_disk = NotesFile::read(&comments, "").unwrap();
    assert_eq!(on_disk.notes[0].anchor.selector, "h1.hero-title");
    assert_eq!(on_disk.notes[0].messages[0].author, "agent");
    assert_eq!(on_disk.notes[0].messages[0].body, "Too loud, try 48px");

    designer.command("reply", &words("--as claude 1 Changed to 48px")).unwrap();
    designer.command("resolve", &words("1")).unwrap();
    let after = NotesFile::read(&comments, "").unwrap();
    assert_eq!(after.notes[0].messages[1].author, "claude");
    assert_eq!(after.notes[0].state, NoteState::Resolved);
    assert!(after.notes[0].resolved.is_some());
    assert_eq!(
        designer.command("comments", &[]).unwrap().value["notes"].as_array().unwrap().len(),
        0,
        "open is the default"
    );
    assert_eq!(
        designer.command("comments", &words("resolved")).unwrap().value["notes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    designer.command("reopen", &words("1")).unwrap();
    assert_eq!(NotesFile::read(&comments, "").unwrap().notes[0].state, NoteState::Open);
    assert_eq!(designer.command("comments", &words("all")).unwrap().value["notes"][0]["id"], 1);

    // A selector with spaces in it is ended with `--`.
    let spaced =
        designer.command("comment", &words("main > section.hero > h1 -- Smaller, please")).unwrap();
    assert_eq!(spaced.value["note"], 2);
    let on_disk = NotesFile::read(&comments, "").unwrap();
    assert_eq!(on_disk.get(2).unwrap().anchor.selector, "main > section.hero > h1");
    assert_eq!(on_disk.get(2).unwrap().messages[0].body, "Smaller, please");
    designer.command("delete", &words("2")).unwrap();
    designer.command("move", &words("1 main > p")).unwrap();
    assert_eq!(NotesFile::read(&comments, "").unwrap().notes[0].anchor.selector, "main > p");
    designer.command("delete", &words("#1")).unwrap();
    assert!(NotesFile::read(&comments, "").unwrap().notes.is_empty());
    assert!(designer.command("reply", &words("9 hello")).unwrap_err().contains("no note 9"));
    assert!(designer
        .command("reply", &words("x hello"))
        .unwrap_err()
        .contains("not a note number"));
}

#[test]
fn a_verb_about_a_design_nobody_is_looking_at_takes_its_name_and_the_page_verbs_ask_for_open() {
    let (mut designer, _project, design) = with_a_design("commands-design");
    designer.forget(&design);
    let refusal = designer.command("comment", &words("h1 text")).unwrap_err();
    assert!(refusal.contains("--design"), "{refusal}");
    assert!(designer.command("comment", &words("--design landing h1 text")).is_ok());
    for verb in ["select", "device", "mode"] {
        let refusal = designer.command(verb, &words("--design landing h1")).unwrap_err();
        assert!(refusal.contains("open landing.html"), "{verb}: {refusal}");
    }
    designer.show(&design);
    assert_eq!(designer.command("device", &words("phone")).unwrap().value["device"], "phone");
    assert_eq!(designer.design_view(&design).unwrap().device, Device::Phone);
    assert!(designer.command("device", &words("watch")).unwrap_err().contains("desktop, laptop"));
    designer.command("mode", &words("comment")).unwrap();
    assert_eq!(designer.design_view(&design).unwrap().mode, Mode::Comment);
    designer.command("mode-select", &[]).unwrap();
    assert_eq!(designer.design_view(&design).unwrap().mode, Mode::Select);
    let calls = designer.take_calls(&design);
    assert!(calls.contains(&super::PageCall::SetMode(Mode::Select)), "{calls:?}");
}

#[test]
fn every_verb_the_provider_lists_is_answered() {
    let (mut designer, _project, _design) = with_a_design("commands-all");
    for (verb, _) in designer.commands() {
        let outcome = designer.command(verb, &[]);
        if let Err(why) = outcome {
            assert!(!why.contains("there is no `"), "{verb} is listed and not answered: {why}");
        }
    }
    let refusal = designer.command("levitate", &[]).unwrap_err();
    assert!(refusal.contains("`levitate`") && refusal.contains("comments"), "{refusal}");
}

#[test]
fn new_and_open_make_a_design_open_it_in_a_tab_and_ask_for_the_pane() {
    let (mut designer, project) = a_designer("commands-new");
    let made = designer.command("new", &words("Pricing Page")).unwrap();
    assert_eq!(made.value["design"], "Pricing Page.html");
    assert!(project.join(".designs/Pricing Page.html").is_file());
    // `task-2260`: the design opens in a tab of the Designer pane, and the window is asked to show the pane.
    let asked = designer.asking();
    assert_eq!(asked, vec![Request::ShowPane(true)]);
    assert_eq!(designer.tabs(), &[Some(PathBuf::from(".designs/Pricing Page.html"))]);
    designer.command("open", &words("Pricing Page")).unwrap();
    assert_eq!(designer.asking().len(), 1);
    assert_eq!(designer.tabs().len(), 1, "opening it again shows the tab it is in");
    assert!(designer.command("new", &words("Pricing Page")).unwrap_err().contains("already"));
    let listed = designer.command("designs", &[]).unwrap();
    assert_eq!(listed.value["designs"][0]["name"], "Pricing Page.html");
    assert!(designer.command("new", &[]).unwrap().value["asked"] == true, "no name asks for one");
    assert!(designer.modal.is_some());
}

#[test]
fn asking_copies_the_design_then_queues_the_two_commands_in_order() {
    let (mut designer, project, design) = with_a_design("commands-ask");
    let id = designer.comment(&design, "main > h1", "Too loud.", "you").unwrap();
    designer.ask_note(&design, id).expect("asked");
    let folder = Path::new(".designs");
    let copies = history::list(&project, folder, &design);
    assert_eq!(copies.len(), 1, "the copy is taken before the request goes");
    assert_eq!(copies[0].words, "too loud");
    let asked = designer.asking();
    assert_eq!(asked.len(), 2, "{asked:?}");
    let Request::RunCommand { command, arguments, id: first_id } = &asked[0] else {
        panic!("{asked:?}")
    };
    assert_eq!(command, "plugins.run");
    assert_eq!(arguments["id"], "agent-chat");
    assert_eq!(arguments["command"], "open-pane");
    assert!(first_id.starts_with("designer-open:"));
    let Request::RunCommand { arguments, id: second_id, .. } = &asked[1] else {
        panic!("{asked:?}")
    };
    assert_eq!(arguments["command"], "send");
    assert!(second_id.starts_with(super::SEND_ID));
    let words = arguments["arguments"].as_str().expect("the words travel as one string");
    assert!(words.contains("Note 1 on") && words.contains("Too loud."), "{words}");
    assert!(words.contains("reply --design .designs/landing.html 1"), "{words}");
    // The thread says it was asked.
    let note = designer.design_view(&design).unwrap().notes.get(id).unwrap();
    assert_eq!(note.messages.last().unwrap().author, "designer");
    // The answer sets the status line, and a refusal also becomes a notice that stays.
    designer.answered(second_id, Ok(json!({ "queued": true })));
    assert_eq!(designer.design_view(&design).unwrap().status, "Queued behind the answer arriving");
    designer.answered(second_id, Err("Agent-Chat is switched off".to_owned()));
    assert_eq!(designer.design_view(&design).unwrap().status, "Agent-Chat is switched off");
    assert!(matches!(designer.asking().as_slice(), [Request::Notice { .. }]));
}

#[test]
fn asking_about_every_open_note_and_free_prompts_go_the_same_way_and_empty_asks_are_refused() {
    let (mut designer, project, design) = with_a_design("commands-ask-all");
    assert!(designer.command("ask-all", &[]).unwrap_err().contains("no open notes"));
    assert!(designer.command("prompt", &[]).unwrap_err().contains("what to change"));
    designer.comment(&design, "h1", "One.", "you").unwrap();
    designer.comment(&design, "p", "Two.", "you").unwrap();
    assert_eq!(designer.command("ask-all", &[]).unwrap().value["notes"], 2);
    assert_eq!(designer.asking().len(), 2);
    designer.command("prompt", &words("Make the hero darker")).unwrap();
    assert_eq!(designer.asking().len(), 2);
    assert_eq!(
        history::list(&project, Path::new(".designs"), &design).len(),
        2,
        "one copy for each request"
    );
}

#[test]
fn what_the_page_reports_changes_the_selection_the_composer_and_the_detached_marks() {
    let (mut designer, project, design) = with_a_design("page-reports");
    let id = designer.comment(&design, "h1.hero-title", "Too loud.", "you").unwrap();
    designer.page_reported(&design, Report::Selected(an_element("h1.hero-title")));
    assert_eq!(
        designer.design_view(&design).unwrap().selection.as_ref().unwrap().selector,
        "h1.hero-title"
    );

    designer.page_reported(
        &design,
        Report::Comment { element: an_element("p.lede"), fx: 0.5, fy: 0.25 },
    );
    let composer =
        designer.design_view(&design).unwrap().composing.clone().expect("a note is being written");
    assert_eq!(composer.element.selector, "p.lede");
    designer.design_view_mut(&design).unwrap().composing.as_mut().unwrap().text =
        "Greyer.".to_owned();
    let second = designer.add_the_note(&design, false).unwrap();
    let note = designer.design_view(&design).unwrap().notes.get(second).unwrap().clone();
    assert_eq!((note.anchor.fx, note.anchor.fy), (0.5, 0.25));
    assert_eq!(note.anchor.excerpt, "Ship designs with your agent");
    assert_eq!(
        note.anchor.page_x,
        100.0 + 0.5 * 400.0,
        "the page place is worked out from the element's box"
    );

    designer.page_reported(&design, Report::Anchors { found: vec![], missing: vec![id] });
    assert!(designer.design_view(&design).unwrap().detached.contains(&id));
    assert!(designer.pins(&design).iter().find(|pin| pin.id == id).unwrap().detached);
    designer.page_reported(&design, Report::Pin(second));
    assert_eq!(designer.design_view(&design).unwrap().active, Some(second));
    designer.page_reported(&design, Report::Cleared);
    assert!(designer.design_view(&design).unwrap().selection.is_none());

    // Where an element was last seen is written only when it moved by more than two pixels.
    let comments = project.join(library::comments_path(&design));
    let found = |x: f32| Report::Anchors {
        found: vec![page::Found { id: second, x, y: 300.0, element: None }],
        missing: vec![],
    };
    designer.page_reported(&design, found(300.0));
    let moved_at = std::fs::metadata(&comments).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    designer.page_reported(&design, found(301.0));
    assert_eq!(
        std::fs::metadata(&comments).unwrap().modified().unwrap(),
        moved_at,
        "one pixel is not worth a write"
    );
    designer.page_reported(&design, found(340.0));
    assert!(std::fs::metadata(&comments).unwrap().modified().unwrap() > moved_at);
    assert_eq!(NotesFile::read(&comments, "").unwrap().get(second).unwrap().anchor.page_x, 340.0);
}

#[test]
fn a_loaded_document_is_told_the_mode_the_pins_and_the_choice_again() {
    let (mut designer, _project, design) = with_a_design("page-loaded");
    designer.comment(&design, "h1", "x", "you").unwrap();
    designer.set_mode(&design, Mode::Select).unwrap();
    designer.page_reported(&design, Report::Selected(an_element("h1")));
    designer.take_calls(&design);
    let before = designer.design_view(&design).unwrap().page_revision;
    designer.page_reported(&design, Report::Loaded { url: "u".into(), scroll_y: 0.0 });
    assert!(designer.design_view(&design).unwrap().page_revision > before);
    let calls = designer.take_calls(&design);
    assert_eq!(calls[0], super::PageCall::SetMode(Mode::Select));
    assert!(calls.contains(&super::PageCall::Select("h1".to_owned())));
    assert!(matches!(calls.last(), Some(super::PageCall::SetPins(pins)) if pins.len() == 1));
    assert!(designer.take_calls(&design).is_empty(), "and only once");
}

#[test]
fn the_view_answers_everything_the_window_draws() {
    let (mut designer, _project, design) = with_a_design("view");
    designer.comment(&design, "h1", "x", "you").unwrap();
    designer.set_device(&design, Device::Tablet).unwrap();
    let view = designer.view();
    assert_eq!(view["folder"], ".designs");
    assert_eq!(view["designs"][0]["open"], 1);
    assert_eq!(view["open"][0]["device"], "tablet");
    assert_eq!(view["open"][0]["notes"][0]["messages"][0]["body"], "x");
    assert_eq!(view["showing"], "landing.html");
    assert_eq!(designer.badge().as_deref(), Some("1"));
}

#[test]
fn the_device_chosen_for_a_design_is_remembered_in_the_project_and_not_shared() {
    let (mut designer, project, design) = with_a_design("device-memory");
    designer.set_device(&design, Device::Phone).unwrap();
    let text = std::fs::read_to_string(project.join(".unluminous/designs.conf")).unwrap();
    assert!(text.contains("device..designs/landing.html = phone"), "{text}");
    designer.forget(&design);
    designer.show(&design);
    assert_eq!(designer.design_view(&design).unwrap().device, Device::Phone);
}

#[test]
fn changing_the_folder_changes_what_is_listed_and_what_opens_as_a_design() {
    let (mut designer, project) = a_designer("folder-change");
    std::fs::create_dir_all(project.join("mocks")).unwrap();
    put(project.join("mocks/home.html"), "<p>x</p>").unwrap();
    assert!(designer.designs().is_empty());
    assert!(!designer.is_a_design(Path::new("mocks/home.html")));
    designer.config.folder = PathBuf::from("mocks");
    designer.write_the_configuration().unwrap();
    assert_eq!(designer.designs().len(), 1);
    assert!(designer.is_a_design(Path::new("mocks/home.html")));
    assert!(designer.take_config_changed(), "the window is told, so it can tell the explorer");
    assert!(!designer.take_config_changed());
    let (read, _) = Configuration::read(&project.join("plugin-settings"));
    assert_eq!(read.folder, PathBuf::from("mocks"));
}

#[test]
fn a_copy_taken_this_evening_is_today_and_one_from_last_night_is_yesterday_on_the_local_clock() {
    let copy = |stamp: &str| history::Backup {
        file: PathBuf::from("x.html"),
        name: "x.html".to_owned(),
        stamp: stamp.to_owned(),
        words: String::new(),
    };
    let now = super::notes::seconds_from_iso("2026-10-12T19:31:00Z").unwrap();
    assert_eq!(copy("20261012-193000").friendly(now), "Today 19:30");
    assert_eq!(copy("20261011-233000").friendly(now), "Yesterday 23:30");
    assert_eq!(copy("20260105-143200").friendly(now), "5 Jan 14:32");
}

// ------------------------------------------------------------------------------------ the pane's own tabs

/// `task-2260`: the Designer pane starts on a blank tab, a design opened from the blank tab takes it, one opened
/// from a design's tab gets a tab of its own after it, and closing the last tab leaves a blank one.
#[test]
fn the_pane_starts_blank_and_a_design_takes_the_blank_tab_or_gets_its_own() {
    let (mut designer, _project, design) = with_a_design("tabs-open");
    // Making the design opened it, so its tab is closed first to begin from what a new window has.
    assert_eq!(designer.tabs(), &[Some(design.clone())]);
    designer.close_tab(0).unwrap();
    assert_eq!(designer.tabs(), &[None], "one blank tab to begin with");
    assert!(designer.active_design().is_none());
    designer.open_in_a_tab(&design);
    assert_eq!(designer.tabs(), &[Some(design.clone())], "the blank tab took it");
    let other = PathBuf::from(".designs/other.html");
    designer.open_in_a_tab(&other);
    assert_eq!(designer.tabs(), &[Some(design.clone()), Some(other.clone())]);
    assert_eq!(designer.active_tab(), 1);
    designer.open_in_a_tab(&design);
    assert_eq!(designer.active_tab(), 0, "an open design is shown, not opened twice");
    designer.new_tab();
    assert_eq!(designer.tabs(), &[Some(design.clone()), None, Some(other.clone())]);
    assert_eq!(designer.active_tab(), 1, "a new tab goes after the one showing");

    // Closing the tab showing shows the one that moves into its place; closing one to its left keeps the same
    // tab showing; closing the last leaves a blank tab.
    designer.close_tab(1).unwrap();
    assert_eq!(designer.active_design(), Some(other.as_path()));
    designer.close_tab(0).unwrap();
    assert_eq!(designer.tabs(), &[Some(other.clone())]);
    assert_eq!(designer.active_tab(), 0);
    designer.close_tab(0).unwrap();
    assert_eq!(designer.tabs(), &[None]);
    assert!(designer.close_tab(3).is_err());
    assert!(designer.show_tab(3).is_err());
}

/// The tabs go into `designs.conf` and come back, a design that has gone is left out, and a list that was
/// longer than the one written leaves nothing behind.
#[test]
fn the_tabs_are_written_down_and_read_back_without_the_designs_that_have_gone() {
    let mut values =
        Values::parse("device.x = phone\ntabs.count = 5\ntabs.4 = .designs/old.html\n");
    let tabs =
        vec![Some(PathBuf::from(".designs/a.html")), None, Some(PathBuf::from(".designs/b.html"))];
    super::tabs::write_tabs(&mut values, &tabs, 2);
    assert_eq!(values.text("tabs.count"), Some("3"));
    assert_eq!(values.text("tabs.1"), Some("blank"));
    assert_eq!(values.text("tabs.4"), None, "the longer list is gone");
    assert_eq!(values.text("device.x"), Some("phone"), "and what else the file held is kept");
    let (read, active) = super::tabs::tabs_from(&values, |_| true);
    assert_eq!(read, tabs);
    assert_eq!(active, 2);
    let (read, active) =
        super::tabs::tabs_from(&values, |design| design != Path::new(".designs/a.html"));
    assert_eq!(read, vec![None, Some(PathBuf::from(".designs/b.html"))]);
    assert_eq!(active, 1, "still on b.html");
    let (read, active) = super::tabs::tabs_from(&Values::new(), |_| true);
    assert_eq!((read, active), (vec![None], 0), "nothing written is one blank tab");
}

/// A tab is named by its number or by its design's name, with or without `.html`.
#[test]
fn a_command_names_a_tab_by_its_number_or_its_design() {
    let (mut designer, _project, design) = with_a_design("tabs-named");
    designer.new_tab();
    let name = designer.name_of(&design);
    assert_eq!(designer.tab_named("1").unwrap(), 1);
    assert_eq!(designer.tab_named(&name).unwrap(), 0);
    assert_eq!(designer.tab_named(name.trim_end_matches(".html")).unwrap(), 0);
    assert!(designer.tab_named("nothing").is_err());
    let said = designer.command("tabs", &[]).unwrap();
    assert_eq!(said.value["tabs"], serde_json::json!([name, null]));
    assert_eq!(said.value["active"], 1);
    designer.command("close-tab", &[]).unwrap();
    assert_eq!(designer.tabs().len(), 1, "close-tab with nothing closes the one showing");
}
