//! How a path is shown to a person.
//!
//! One rule, in one place, because the window draws a path in three unrelated corners — the database
//! tree and its settings page, the New File prompt, and the name of a picture attached to a chat
//! message — and each of them had reached the same wrong answer separately.

use unluminous_db::Source;

/// A path cut to the folder it is in and its own name.
///
/// `C:\Users\jason\AppData\Local\Temp\unluminous-database-shot-tree\library.db` becomes
/// `…\unluminous-database-shot-tree\library.db`.
///
/// **The first reason is that it reads better.** A row in the database tree has room for about
/// eighteen characters before it is cut off, and cutting an absolute path at the *end* throws away the
/// file's name and keeps `C:\Users\jason\AppData`, which is the part nobody is looking for. What
/// somebody wants to see is which file this is and which folder it is in.
///
/// **The second is that a picture of the window holding a path holds the path of the machine that took
/// it.** `components/settings_dialog.rs` already says this about the path it is handed: *"a screenshot
/// test has to be able to pin it: a picture holding this machine's own path is a picture no other
/// machine can match."* Nine accepted pictures held one, and every one of them failed on a CI runner,
/// whose temporary folder is under `C:\Users\RUNNER~1`. `task-1922`.
///
/// The separators are the ones the path already had rather than the platform's, so nothing is
/// normalised on the way to being shown: a path typed with `/` on Windows is shown with `/`.
pub fn the_useful_end_of(path: &str) -> String {
    let cuts: Vec<usize> =
        path.char_indices().filter(|(_, c)| *c == '/' || *c == '\\').map(|(at, _)| at).collect();
    // Nothing to cut: no separator at all, or one folder and a name, which is already the answer.
    if cuts.len() < 2 {
        return path.to_owned();
    }
    format!("…{}", &path[cuts[cuts.len() - 2]..])
}

/// Where a data source points, as it is drawn.
///
/// [`Source::where_it_points`] is the whole of it and is what an agent is answered with. This is the
/// same sentence with a file's path cut to its end. A server address is short and is left exactly as
/// that function wrote it.
///
/// It is composed here rather than in `unluminous-db` because that crate is a leaf on purpose — it
/// depends on no other crate of Unluminous's — and because how long a path is drawn is a question
/// about this window rather than about a database.
pub fn where_a_source_points(source: &Source) -> String {
    match source.engine.is_a_file() {
        true => format!("{} · {}", source.engine.title(), the_useful_end_of(&source.database)),
        false => source.where_it_points(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path is cut to the folder it is in and its own name, with the separators it already had.
    #[test]
    fn a_path_is_shown_as_its_last_folder_and_its_name() {
        assert_eq!(
            the_useful_end_of(r"C:\Users\jason\AppData\Local\Temp\shot-tree\library.db"),
            "…\\shot-tree\\library.db"
        );
        assert_eq!(
            the_useful_end_of("/Users/runner/work/unluminous/shot-tree/library.db"),
            "…/shot-tree/library.db"
        );
        // Already the answer: one folder and a name, or a bare name, or nothing at all.
        assert_eq!(the_useful_end_of(r"shot-tree\library.db"), r"shot-tree\library.db");
        assert_eq!(the_useful_end_of("library.db"), "library.db");
        assert_eq!(the_useful_end_of(""), "");
    }

    /// A folder is cut the same way, which is what the New File prompt says where a file will go.
    #[test]
    fn a_folder_is_cut_the_same_way_as_a_file() {
        assert_eq!(
            the_useful_end_of(r"C:\Users\jason\AppData\Local\Temp\unluminous-screenshot-folder"),
            "…\\Temp\\unluminous-screenshot-folder"
        );
    }

    /// **Nothing about the machine is in what is drawn**, which is what makes a picture of the window
    /// matchable on a machine that is not the one it was taken on. `task-1922`.
    #[test]
    fn what_is_drawn_about_a_file_holds_no_part_of_the_machine_it_is_on() {
        let here = Source::sqlite(
            "library",
            r"C:\Users\jason\AppData\Local\Temp\unluminous-database-shot-tree\library.db",
        );
        let runner = Source::sqlite(
            "library",
            r"C:\Users\RUNNER~1\AppData\Local\Temp\unluminous-database-shot-tree\library.db",
        );
        assert_eq!(where_a_source_points(&here), where_a_source_points(&runner));
        assert!(where_a_source_points(&here).ends_with(r"shot-tree\library.db"));
        // And the full answer, which is what an agent is given, still holds the whole path.
        assert_ne!(here.where_it_points(), runner.where_it_points());
        assert!(here.where_it_points().contains("jason"));
    }

    /// A server is not a file and is left exactly as `where_it_points` wrote it.
    #[test]
    fn a_server_address_is_left_whole() {
        let source = Source::parse("ai", "postgres://postgres@localhost:5432/ai").expect("read");
        assert_eq!(where_a_source_points(&source), source.where_it_points());
    }
}
