use super::*;

fn chord(text: &str) -> Shortcut {
    parse(text).unwrap_or_else(|why| panic!("{text}: {why}"))
}

fn own(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect()
}

fn chord_of(table: &Table, action: &str) -> Option<Shortcut> {
    table.rows.get(action).unwrap_or_else(|| panic!("{action} is not bindable")).chord
}

/// Every spelling a person types reads as the chord the menus show, and `Ctrl` means the one control key off
/// macOS and the control key beside `Cmd` on it.
#[test]
fn a_chord_is_read_however_it_is_written() {
    let windows = |text: &str| parse_for(text, false).unwrap();
    assert_eq!(windows("Ctrl+Shift+K"), Shortcut::command_shift(egui::Key::K));
    assert_eq!(windows("ctrl-shift-k"), Shortcut::command_shift(egui::Key::K));
    assert_eq!(windows("Cmd+Shift+K"), Shortcut::command_shift(egui::Key::K));
    assert_eq!(windows("Ctrl+/"), Shortcut::command(egui::Key::Slash));
    assert_eq!(windows("Ctrl+Slash"), Shortcut::command(egui::Key::Slash));
    assert_eq!(windows("Ctrl++"), Shortcut::command(egui::Key::Plus));
    assert_eq!(windows("ctrl--"), Shortcut::command(egui::Key::Minus));
    assert_eq!(windows("Ctrl+,"), Shortcut::command(egui::Key::Comma));
    assert_eq!(windows("Shift+F6"), Shortcut { shift: true, ..Shortcut::plain(egui::Key::F6) });
    assert_eq!(windows("alt+pagedown"), Shortcut { alt: true, ..Shortcut::plain(egui::Key::PageDown) });
    assert_eq!(windows("Ctrl+`"), Shortcut::command(egui::Key::Backtick));
    let mac = |text: &str| parse_for(text, true).unwrap();
    assert_eq!(mac("Cmd+K"), Shortcut::command(egui::Key::K));
    assert_eq!(mac("Ctrl+K"), Shortcut::control(egui::Key::K));
    assert_eq!(mac("Option+F7"), Shortcut { alt: true, ..Shortcut::plain(egui::Key::F7) });
    assert!(parse_for("Ctrl+Hyper+K", false).is_err());
    assert!(parse_for("Ctrl+Nonsense", false).is_err());
}

/// What is written down reads back as the same chord.
#[test]
fn a_chord_written_down_reads_back_as_itself() {
    for bindable in bindables() {
        if let Some(default) = bindable.default {
            let back = parse(&text(&default)).unwrap_or_else(|why| panic!("{}: {why}", text(&default)));
            assert!(same(&back, &default), "{} wrote {} and read back {back:?}", bindable.name, text(&default));
        }
    }
}

/// Two chords that match the same key press are the same chord.
#[test]
fn plus_and_equals_are_one_chord_and_so_are_the_two_spellings_of_slash() {
    assert!(same(&Shortcut::command(egui::Key::Plus), &Shortcut::command(egui::Key::Equals)));
    assert!(same(
        &Shortcut { shift: true, ..Shortcut::command(egui::Key::Questionmark) },
        &Shortcut::command_shift(egui::Key::Slash)
    ));
    assert!(!same(&Shortcut::command(egui::Key::S), &Shortcut::command_shift(egui::Key::S)));
}

/// Every row of every preset names an action a chord can be given, and its chords read on both platforms.
#[test]
fn every_preset_row_names_a_bindable_action_and_reads_on_both_platforms() {
    for preset in Preset::ALL {
        for row in preset.rows() {
            let found = bindables().iter().find(|b| b.name == row.action);
            let found = found.unwrap_or_else(|| panic!("{} names {}, which is not bindable", preset.name(), row.action));
            assert!(found.fixed.is_none(), "{} names {}, which is fixed", preset.name(), row.action);
            for (text, mac) in [(row.windows, false), (row.mac, true)] {
                if !text.is_empty() && text != "none" {
                    parse_for(text, mac).unwrap_or_else(|why| panic!("{}: {text}: {why}", row.action));
                }
            }
        }
    }
}

/// No two rows of one preset share a chord, except the pair the menus keep apart by which is enabled, so no
/// preset row ever loses its chord to another.
#[test]
fn no_preset_row_loses_its_chord_to_another_on_either_platform() {
    for preset in Preset::ALL {
        for mac in [false, true] {
            let table = table_for(preset, &[], bindables(), mac);
            for row in preset.rows() {
                let binding = &table.rows[row.action];
                assert!(
                    binding.lost_to.is_none(),
                    "{} on {}: {} lost its chord to {:?}",
                    preset.name(),
                    if mac { "macOS" } else { "Windows" },
                    row.action,
                    binding.lost_to
                );
            }
        }
    }
}

/// After any preset, no two menu entries that the menu watcher reads claim one chord, which is
/// `actions::tests::no_two_menu_items_claim_one_key_equivalent` asked of every preset.
#[test]
fn no_two_menu_entries_claim_one_chord_under_any_preset() {
    for preset in Preset::ALL {
        let table = table(preset, &[], bindables());
        let mut held: HashMap<Shortcut, Vec<String>> = HashMap::new();
        for (name, binding) in &table.rows {
            if let Some(chord) = binding.chord {
                held.entry(chord.normalised_for(is_mac())).or_default().push(name.clone());
            }
        }
        for names in held.values().filter(|names| names.len() > 1) {
            assert!(
                names.len() == 2 && shared_by_state(&names[0], &names[1]),
                "{}: {names:?} share a chord",
                preset.name()
            );
        }
    }
}

/// Unluminous's preset changes nothing, so the menus are exactly as built.
#[test]
fn the_unluminous_preset_with_nothing_of_your_own_changes_nothing() {
    assert!(resolve(Preset::Unluminous, &[]).is_empty());
}

/// A preset gives its chords, and an action whose Unluminous chord a preset gave to another loses it.
#[test]
fn intellij_puts_duplicate_on_ctrl_d_and_show_diff_loses_it() {
    let table = table_for(Preset::IntelliJ, &[], bindables(), false);
    assert_eq!(chord_of(&table, "duplicate-line"), Some(parse_for("Ctrl+D", false).unwrap()));
    assert_eq!(chord_of(&table, "git-show-diff"), None);
    assert_eq!(chord_of(&table, "replace"), Some(parse_for("Ctrl+R", false).unwrap()));
    assert_eq!(table.rows["duplicate-line"].source, Source::Preset);
    let vs_code = table_for(Preset::VsCode, &[], bindables(), false);
    assert_eq!(chord_of(&vs_code, "go-to-definition"), Some(parse_for("F12", false).unwrap()));
    assert_eq!(chord_of(&vs_code, "debug-start"), chord_of(&vs_code, "debug-resume"));
}

/// The person's own binding beats the preset's, and takes the chord from the action the preset gave it to.
#[test]
fn your_own_binding_beats_the_preset_and_takes_its_chord() {
    let yours = own(&[("duplicate-line", "Ctrl+F")]);
    let table = table_for(Preset::IntelliJ, &yours, bindables(), false);
    assert_eq!(chord_of(&table, "duplicate-line"), Some(parse_for("Ctrl+F", false).unwrap()));
    assert_eq!(table.rows["duplicate-line"].source, Source::Yours);
    assert_eq!(chord_of(&table, "find"), None);
    assert_eq!(table.rows["find"].lost_to.as_deref(), Some("duplicate-line"));
}

/// `none` takes an action's chord away.
#[test]
fn none_takes_a_chord_away() {
    let table = table_for(Preset::Unluminous, &own(&[("save", "none")]), bindables(), false);
    assert_eq!(chord_of(&table, "save"), None);
    let resolved = resolved_from(&table, bindables());
    assert_eq!(resolved.changed.get("save"), Some(&None));
}

/// A line naming an action this build has not got is kept, and reported, rather than lost.
#[test]
fn a_binding_for_an_unknown_action_is_kept_and_reported() {
    let mut yours = own(&[("teleport", "Ctrl+Shift+Y")]);
    let table = table(Preset::Unluminous, &yours, bindables());
    assert_eq!(table.unknown, own(&[("teleport", "Ctrl+Shift+Y")]));
    bind(&mut yours, Preset::Unluminous, "save", Some(chord("F4"))).unwrap();
    assert!(yours.iter().any(|(name, _)| name == "teleport"), "binding something else kept it: {yours:?}");
}

/// Binding takes the chord from the action that has it, writes `none` for it, and says so.
#[test]
fn binding_a_chord_another_action_has_takes_it_from_that_action() {
    let mut yours = Vec::new();
    let save = bindables().iter().find(|b| b.name == "save").and_then(|b| b.default).unwrap();
    let bound = bind(&mut yours, Preset::Unluminous, "find", Some(save)).unwrap();
    assert_eq!(bound.taken_from, vec!["save".to_owned()]);
    let table = table(Preset::Unluminous, &yours, bindables());
    assert!(chord_of(&table, "find").is_some_and(|got| same(&got, &save)));
    assert_eq!(chord_of(&table, "save"), None);
    assert!(yours.contains(&("save".to_owned(), "none".to_owned())), "{yours:?}");
}

/// Binding an action to the chord it would have anyway writes nothing.
#[test]
fn binding_the_chord_an_action_already_has_writes_nothing() {
    let mut yours = Vec::new();
    let save = bindables().iter().find(|b| b.name == "save").and_then(|b| b.default).unwrap();
    let bound = bind(&mut yours, Preset::Unluminous, "save", Some(save)).unwrap();
    assert!(bound.taken_from.is_empty());
    assert!(yours.is_empty(), "{yours:?}");
}

/// The chords something other than a menu entry reads are refused, and an action that cannot be bound is.
#[test]
fn reserved_chords_and_fixed_actions_are_refused() {
    let mut yours = Vec::new();
    assert!(bind(&mut yours, Preset::Unluminous, "save", Some(chord("Ctrl+C"))).is_err());
    assert!(bind(&mut yours, Preset::Unluminous, "save", Some(chord("A"))).is_err());
    assert!(bind(&mut yours, Preset::Unluminous, "save", Some(chord("Ctrl+Left"))).is_err());
    assert!(bind(&mut yours, Preset::Unluminous, "save", Some(chord("Enter"))).is_err());
    assert!(bind(&mut yours, Preset::Unluminous, "copy", Some(chord("F4"))).is_err());
    assert!(bind(&mut yours, Preset::Unluminous, "teleport", Some(chord("F4"))).is_err());
    assert!(bind(&mut yours, Preset::Unluminous, "save", Some(chord("F4"))).is_ok());
    assert!(reserved_for(&parse_for("Ctrl+Shift+Left", false).unwrap(), false).is_some());
    assert!(reserved_for(&parse_for("Ctrl+Alt+Left", false).unwrap(), false).is_none());
}

/// Two of the person's own bindings on one chord, which only a hand edited file can hold: the one whose
/// name sorts first keeps it.
#[test]
fn two_of_your_own_bindings_on_one_chord_keep_the_first_by_name() {
    let yours = own(&[("save", "F4"), ("find", "F4")]);
    let table = table(Preset::Unluminous, &yours, bindables());
    assert!(chord_of(&table, "find").is_some());
    assert_eq!(chord_of(&table, "save"), None);
}

/// Cut, Copy and Paste are listed and fixed, and an action that needs a path is not listed at all.
#[test]
fn the_clipboard_is_fixed_and_an_action_about_a_path_is_not_bindable() {
    let copy = bindables().iter().find(|b| b.name == "copy").expect("copy is listed");
    assert!(copy.fixed.is_some());
    assert!(bindables().iter().all(|b| b.name != "open-recent" && b.name != "copy-path"));
    let moving = bindables().iter().find(|b| b.name == "move-line-up").expect("moving a line is listed");
    assert!(moving.fixed.is_none(), "moving a line can be bound, because the window reads it from the keymap");
    assert_eq!(moving.menu, "Code");
}

/// The keymap puts its chords on the menus, and the menu watcher follows them.
#[test]
fn the_menus_and_the_watcher_follow_the_keymap() {
    let state = MenuState {
        line_edits_apply: true,
        in_repository: true,
        has_file: true,
        keymap: std::sync::Arc::new(resolve(Preset::IntelliJ, &[])),
        ..MenuState::default()
    };
    let menus = actions::menus(&state);
    let control = if is_mac() {
        egui::Modifiers { command: true, mac_cmd: true, ..Default::default() }
    } else {
        egui::Modifiers { command: true, ctrl: true, ..Default::default() }
    };
    assert_eq!(actions::action_for_key(&menus, egui::Key::D, &control), Some(Action::DuplicateLines));
    let shown = bindables_in(&menus);
    let diff = shown.iter().find(|b| b.name == "git-show-diff").expect("Show Diff is on the Git menu");
    assert_eq!(diff.default, None, "Show Diff shows no chord once IntelliJ gave Ctrl+D away");
}

/// The menus with no chords, for the macOS bar while a row is recording.
#[test]
fn without_chords_takes_every_chord_off() {
    let menus = without_chords(&actions::menus(&MenuState::default()));
    assert!(bindables_in(&menus).iter().all(|b| b.default.is_none()));
}

/// Every preset reads back from its own name and from the names people use.
#[test]
fn a_preset_reads_back_from_its_name() {
    for preset in Preset::ALL {
        assert_eq!(Preset::parse(preset.name()), Some(preset));
        assert_eq!(Preset::parse(preset.label()), Some(preset));
    }
    assert_eq!(Preset::parse("JetBrains"), Some(Preset::IntelliJ));
    assert_eq!(Preset::parse("VS-Code"), Some(Preset::VsCode));
    assert_eq!(Preset::parse("emacs"), None);
}

/// An action on two menus is listed with the chord one of them shows: `Settings` is on `Unluminous` with no
/// chord and on `Edit` with one.
#[test]
fn an_action_on_two_menus_is_listed_with_the_chord_one_of_them_shows() {
    let settings = bindables().iter().find(|b| b.name == "settings").expect("Settings is listed");
    assert!(settings.default.is_some(), "{settings:?}");
    assert_eq!(settings.menu, "Edit");
}
