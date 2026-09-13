//! What a search table declared about itself, and which of its columns hold vectors.
//!
//! A search table is the reason this engine is in Unluminous at all, and to the schema it is an
//! ordinary row: `sqlite_schema` calls it a `table`, so the driver reports `Kind::Table` and nothing
//! downstream would know. What makes it one is the text of its own `CREATE`, and what it *promises* is
//! in a shadow table it wrote when it was made:
//!
//! ```sql
//! CREATE VIRTUAL TABLE docs USING inillucent_search(title, body, dims = 768, mode = 'exact', metric = 'cosine');
//! ```
//!
//! ```text
//! docs_config     columns  title,body     dims 768     format 1
//!                 metric   cosine         mode exact   tokenize porter
//! ```
//!
//! **Both are read rather than guessed at.** The `CREATE` text is what the file itself stores, so
//! there is no second list in Unluminous of what a search table is; and the declaration comes from
//! `%_config` rather than from parsing the `CREATE`, because `%_config` is what the engine actually
//! reads back when it opens the table — a value parsed out of the text would be Unluminous's opinion of
//! the index rather than the index's own.

use std::collections::HashMap;

use inillucent_driver::Connection;

use crate::catalog::{Kind, Table};
use crate::rows::{Answer, Failure};
use crate::value::Value;

/// The module a search table is declared with.
pub const MODULE: &str = "inillucent_search";

/// The five shadow tables a search index owns, in the order the engine makes them.
pub const SUFFIXES: &[&str] = &["config", "content", "delta", "gen", "state"];

/// The hidden column a search table hands its stored vector back through.
pub const VECTOR_COLUMN: &str = "vector";

/// The column the `%_content` shadow table keeps a vector in.
pub const CONTENT_VECTOR_COLUMN: &str = "v";

/// What a search table says about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchIndex {
    /// The table it belongs to.
    pub table: String,
    /// How wide a vector is, or zero for an index with no vectors at all.
    ///
    /// Zero is a real answer rather than a missing one: `dims` is optional in the declaration, and a
    /// table declared without it is lexical only. Drawing `0d` would be wrong, so the summary says
    /// `lexical` instead.
    pub dimensions: usize,
    /// `exact` compares every candidate; `approximate` traverses the graph.
    pub mode: String,
    /// The distance being minimised.
    pub metric: String,
    /// Which analysis produced the terms.
    pub tokenizer: String,
    /// The indexed text columns, in declared order.
    pub columns: Vec<String>,
    /// The layout version stamped into `%_config`.
    pub format: String,
}

impl SearchIndex {
    /// The line the tree draws under a search table.
    ///
    /// Everything a person needs to know before they trust a result: how wide the vectors are, whether
    /// the answer is exact or approximate, what distance it is exact *about*, and which analysis
    /// produced the terms.
    pub fn summary(&self) -> String {
        let width = match self.dimensions {
            0 => "lexical".to_owned(),
            dimensions => format!("{dimensions}d"),
        };
        let mut parts = vec![width];
        for part in [&self.mode, &self.metric, &self.tokenizer] {
            if !part.is_empty() {
                parts.push(part.clone());
            }
        }
        parts.join(" · ")
    }

    /// Whether rows in it carry a vector at all.
    pub fn has_vectors(&self) -> bool {
        self.dimensions > 0
    }
}

/// Every object's `CREATE` text, and what that makes it.
#[derive(Debug, Clone, Default)]
pub struct Declarations {
    /// The search tables, by name.
    searches: Vec<String>,
    /// A shadow table's name, mapped to the search table that owns it.
    shadows: HashMap<String, String>,
}

impl Declarations {
    pub fn is_search(&self, name: &str) -> bool {
        self.searches.iter().any(|search| search == name)
    }

    /// Which search table owns this shadow table, if it is one.
    pub fn owner_of(&self, name: &str) -> Option<&str> {
        self.shadows.get(name).map(String::as_str)
    }

    pub fn searches(&self) -> &[String] {
        &self.searches
    }
}

/// Read every object's declaration out of the schema.
///
/// One query rather than a `ddl()` per item: a database with a few hundred objects in it would
/// otherwise pay a statement each to answer a question the schema answers all at once, and the tree
/// asks this every time it is filled in.
///
/// @param connection - the open connection
pub fn declarations(connection: &Connection<'_>) -> Answer<Declarations> {
    let answered = connection
        .query("select name, sql from sqlite_schema", &[], usize::MAX)
        .map_err(super::said)?;
    let mut found = Declarations::default();
    let mut names: Vec<String> = Vec::new();
    for row in &answered.rows {
        let name = match row.first().and_then(inillucent_driver::Value::text) {
            Some(name) => name.to_owned(),
            None => continue,
        };
        let sql = row.get(1).and_then(inillucent_driver::Value::text).unwrap_or_default();
        if is_a_search_declaration(sql) {
            found.searches.push(name.clone());
        }
        names.push(name);
    }
    // The shadows are worked out afterwards, because a shadow can be listed before its owner and the
    // question "is `docs_content` a shadow" cannot be answered until `docs` is known to be a search
    // table.
    for search in &found.searches {
        for suffix in SUFFIXES {
            let shadow = format!("{search}_{suffix}");
            if names.iter().any(|name| name == &shadow) {
                found.shadows.insert(shadow, search.clone());
            }
        }
    }
    Ok(found)
}

/// Whether a `CREATE` statement makes a search table.
///
/// Matched on the module name after `using`, case-insensitively, because the engine stores the text as
/// it was typed and `USING`, `using` and `Using` are the same statement.
///
/// @param sql - the `CREATE` text the schema stores
pub fn is_a_search_declaration(sql: &str) -> bool {
    let lowered = sql.to_ascii_lowercase();
    let Some(at) = lowered.find(MODULE) else {
        return false;
    };
    lowered[..at].contains("using")
}

/// What kind of row in the tree an item really is.
///
/// The driver knows tables, views, indexes and triggers; the two kinds it does not know about are the
/// two this engine's own storage introduces.
///
/// @param item - what the driver reported
/// @param declarations - what the schema's text says
pub fn kind_of(item: &inillucent_driver::Item, declarations: &Declarations) -> Kind {
    match item.kind {
        inillucent_driver::Kind::Table if declarations.is_search(&item.name) => Kind::Search,
        inillucent_driver::Kind::Table if declarations.owner_of(&item.name).is_some() => {
            Kind::Shadow
        }
        inillucent_driver::Kind::Table => Kind::Table,
        inillucent_driver::Kind::View => Kind::View,
        inillucent_driver::Kind::Index => Kind::Index,
        // A trigger is a routine in this crate's vocabulary: it is a thing with a body that runs, and
        // it holds no rows. The alternative is an eighth `Kind` that draws exactly like the seventh.
        inillucent_driver::Kind::Trigger => Kind::Routine,
    }
}

/// Which of a table's columns hold vectors.
///
/// **The schema decides, never the bytes.** A column is a vector because the table it is in declared
/// vectors and this is the column they arrive in — the search table's own hidden `vector`, or the `v`
/// of its `%_content` shadow. Any other blob is left alone, because a PNG is also a run of bytes whose
/// length divides by four and a grid that drew one as an embedding would be lying.
///
/// @param name - the table being described
/// @param declarations - what the schema's text says
/// @param table - the columns as they were read
pub fn vector_columns(name: &str, declarations: &Declarations, table: &Table) -> Vec<String> {
    let holds = |column: &str| table.columns.iter().any(|had| had.name == column);
    if declarations.is_search(name) && holds(VECTOR_COLUMN) {
        return vec![VECTOR_COLUMN.to_owned()];
    }
    let is_content = declarations.owner_of(name).is_some_and(|_| name.ends_with("_content"));
    match is_content && holds(CONTENT_VECTOR_COLUMN) {
        true => vec![CONTENT_VECTOR_COLUMN.to_owned()],
        false => Vec::new(),
    }
}

/// Read a search table's declaration out of its own `%_config`.
///
/// @param connection - the open connection
/// @param name - the search table
pub fn read_config(connection: &Connection<'_>, name: &str) -> Answer<SearchIndex> {
    let statement = format!(
        "select k, v from {}",
        inillucent_driver::introspect::quoted(&format!("{name}_config"))
    );
    let answered = connection.query(&statement, &[], usize::MAX).map_err(super::said)?;
    let mut index = SearchIndex { table: name.to_owned(), ..SearchIndex::default() };
    for row in &answered.rows {
        let key = row.first().and_then(inillucent_driver::Value::text).unwrap_or_default();
        let value = row.get(1).and_then(inillucent_driver::Value::text).unwrap_or_default();
        match key {
            "dims" => index.dimensions = value.trim().parse().unwrap_or(0),
            "mode" => index.mode = value.to_owned(),
            "metric" => index.metric = value.to_owned(),
            "tokenize" => index.tokenizer = value.to_owned(),
            "format" => index.format = value.to_owned(),
            "columns" => {
                index.columns = value
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect()
            }
            _ => {}
        }
    }
    if index.columns.is_empty() && index.mode.is_empty() {
        return Err(Failure::said(format!(
            "`{name}` declares itself a search table, but its configuration is empty — the file may \
             have been written by a later version of the engine."
        )));
    }
    Ok(index)
}

/// The statement a search table's `Search…` entry composes.
///
/// It is put into a console page rather than run behind somebody's back, so the `k`, the text and the
/// ordering are all there to be read and edited — which matters more here than for an ordinary query,
/// because `k` decides how deep the retrieval went and `LIMIT` only trims what came back.
///
/// @param name - the search table
/// @param index - what it declared
pub fn search_statement(name: &str, index: &SearchIndex) -> String {
    let quoted = crate::catalog::quoted(name, '"');
    let columns: Vec<String> =
        index.columns.iter().map(|column| crate::catalog::quoted(column, '"')).collect();
    let selected = match columns.is_empty() {
        true => "*".to_owned(),
        false => columns.join(", "),
    };
    format!(
        "select rowid, {selected}, rank\n  from {quoted}\n where {quoted} match 'your query here'\n   \
         and k = 10\n order by rank\n limit 10;"
    )
}

/// The statement that reads one row's vector, for `plugins run database vector`.
///
/// @param name - the search table
/// @param column - the vector column
/// @param key - the row's rowid
pub fn vector_statement(name: &str, column: &str, key: i64) -> (String, Vec<Value>) {
    (
        format!(
            "select {} from {} where rowid = ?1",
            crate::catalog::quoted(column, '"'),
            crate::catalog::quoted(name, '"')
        ),
        vec![Value::typed(key.to_string())],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declarations_of(searches: &[&str], names: &[&str]) -> Declarations {
        let mut found = Declarations {
            searches: searches.iter().map(|name| (*name).to_owned()).collect(),
            shadows: HashMap::new(),
        };
        for search in searches {
            for suffix in SUFFIXES {
                let shadow = format!("{search}_{suffix}");
                if names.contains(&shadow.as_str()) {
                    found.shadows.insert(shadow, (*search).to_owned());
                }
            }
        }
        found
    }

    #[test]
    fn a_search_table_is_recognised_from_the_text_the_file_stores() {
        assert!(is_a_search_declaration(
            "CREATE VIRTUAL TABLE docs using inillucent_search(title, body, dims = 4)"
        ));
        assert!(is_a_search_declaration(
            "CREATE VIRTUAL TABLE docs USING INILLUCENT_SEARCH(title)"
        ));
        // An ordinary table is not one, and neither is one whose *name* merely mentions the module —
        // the word has to come after `using`, which is where a module name lives.
        assert!(!is_a_search_declaration("CREATE TABLE member(id integer primary key)"));
        assert!(!is_a_search_declaration("CREATE TABLE inillucent_search_notes(id integer)"));
        assert!(!is_a_search_declaration(""));
    }

    #[test]
    fn the_five_shadow_tables_are_attributed_to_their_owner() {
        let names = ["docs", "docs_config", "docs_content", "docs_delta", "docs_gen", "docs_state"];
        let found = declarations_of(&["docs"], &names);
        assert!(found.is_search("docs"));
        for suffix in SUFFIXES {
            let shadow = format!("docs_{suffix}");
            assert_eq!(found.owner_of(&shadow), Some("docs"), "{shadow}");
        }
        // And a table that merely starts with the same letters is nobody's shadow.
        assert_eq!(found.owner_of("docs_archive"), None);
        assert_eq!(found.owner_of("member"), None);
    }

    #[test]
    fn a_shadow_is_only_a_shadow_when_its_owner_is_a_search_table() {
        // The fault this prevents: a person with a table called `notes` and another called
        // `notes_state` would otherwise find the second one drawn as plumbing they must not touch.
        let found = declarations_of(&[], &["notes", "notes_state"]);
        assert_eq!(found.owner_of("notes_state"), None);
    }

    #[test]
    fn the_vector_column_comes_from_the_schema_rather_than_from_the_bytes() {
        let mut table = Table { name: "docs".to_owned(), ..Table::default() };
        table.columns = ["title", "body", "docs", "k", "vector", "recall", "rank"]
            .iter()
            .map(|name| crate::value::Column::new(*name, "BLOB"))
            .collect();
        let found = declarations_of(&["docs"], &["docs", "docs_content"]);
        assert_eq!(vector_columns("docs", &found, &table), ["vector"]);

        let mut content = Table { name: "docs_content".to_owned(), ..Table::default() };
        content.columns = ["id", "c0", "c1", "v"]
            .iter()
            .map(|name| crate::value::Column::new(*name, ""))
            .collect();
        assert_eq!(vector_columns("docs_content", &found, &content), ["v"]);

        // An ordinary table's blob is left alone, whatever it looks like.
        let mut member = Table { name: "member".to_owned(), ..Table::default() };
        member.columns = vec![crate::value::Column::new("portrait", "BLOB")];
        assert!(vector_columns("member", &found, &member).is_empty());
    }

    #[test]
    fn the_summary_says_the_four_things_worth_knowing_before_trusting_a_result() {
        let index = SearchIndex {
            table: "docs".to_owned(),
            dimensions: 768,
            mode: "exact".to_owned(),
            metric: "cosine".to_owned(),
            tokenizer: "porter".to_owned(),
            columns: vec!["title".to_owned(), "body".to_owned()],
            format: "1".to_owned(),
        };
        assert_eq!(index.summary(), "768d · exact · cosine · porter");
        assert!(index.has_vectors());

        // A table declared with no width is lexical, and says so rather than claiming zero dimensions.
        let lexical = SearchIndex { dimensions: 0, ..index };
        assert_eq!(lexical.summary(), "lexical · exact · cosine · porter");
        assert!(!lexical.has_vectors());
    }

    #[test]
    fn the_search_statement_is_something_a_person_can_read_and_edit() {
        let index = SearchIndex {
            table: "docs".to_owned(),
            dimensions: 768,
            columns: vec!["title".to_owned(), "body".to_owned()],
            ..SearchIndex::default()
        };
        let statement = search_statement("docs", &index);
        assert!(statement.contains("\"docs\" match 'your query here'"), "{statement}");
        assert!(
            statement.contains("and k = 10"),
            "k is the retrieval depth and is shown: {statement}"
        );
        assert!(statement.contains("order by rank"), "{statement}");
        assert!(statement.contains("\"title\", \"body\""), "{statement}");
    }

    #[test]
    fn reading_one_vector_binds_the_key_rather_than_pasting_it() {
        let (statement, values) = vector_statement("docs", "vector", 7);
        assert_eq!(statement, "select \"vector\" from \"docs\" where rowid = ?1");
        assert_eq!(values, [Value::Text("7".to_owned())]);
    }
}
