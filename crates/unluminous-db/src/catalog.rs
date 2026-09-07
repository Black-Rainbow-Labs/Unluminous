//! What is in a database, as the tree draws it.
//!
//! One set of values for both engines, so `components::database::tree` has no idea which engine a row
//! came from — the seam `unluminous_core::mermaid::Scene` is for diagrams and `unluminous_chat::Reply` is for
//! the five wire shapes, made once more here.
//!
//! **Everything is asked for one level at a time.** Opening a data source lists its schemas, opening a
//! schema lists its tables, opening a table lists its columns. A database with four thousand tables in
//! it is the reason: The reference editor's own answer to that is an introspection-level setting, and asking
//! lazily is the same answer with nothing to configure.

use crate::value::Column;

/// What kind of thing a row in the tree is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Table,
    View,
    /// A materialised view, which is a view that holds its rows and therefore reads like a table.
    MaterialisedView,
    /// A table that lives somewhere else, through a foreign data wrapper.
    Foreign,
    Index,
    Sequence,
    Routine,
    /// A search index: rows, text and a vector each, with its own declaration.
    ///
    /// Inillucent's `CREATE VIRTUAL TABLE … USING inillucent_search(…)`. To the schema it is a table
    /// like any other, so this kind exists to carry what a table cannot: how wide its vectors are,
    /// whether its answers are exact or approximate, and which distance they are exact about.
    Search,
    /// A table another object owns and keeps its state in.
    ///
    /// A search index's five: `%_config`, `%_content`, `%_delta`, `%_gen` and `%_state`. They are
    /// **shown rather than hidden**, because they are where the vectors are and hiding them would hide
    /// the thing somebody came to look at — and they are marked, because editing one by hand would
    /// corrupt the index that owns it.
    Shadow,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Table => "table",
            Kind::View => "view",
            Kind::MaterialisedView => "materialised view",
            Kind::Foreign => "foreign table",
            Kind::Index => "index",
            Kind::Sequence => "sequence",
            Kind::Routine => "routine",
            Kind::Search => "search index",
            Kind::Shadow => "shadow table",
        }
    }

    /// Which folder in the tree it hangs under, which is the reference editor's grouping.
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Table | Kind::Foreign => "tables",
            Kind::View | Kind::MaterialisedView => "views",
            Kind::Index => "indexes",
            Kind::Sequence => "sequences",
            Kind::Routine => "routines",
            Kind::Search => "search",
            Kind::Shadow => "shadow",
        }
    }

    /// Whether rows can be read out of it at all, which is what decides whether double-clicking it
    /// opens a grid.
    pub fn holds_rows(self) -> bool {
        matches!(
            self,
            Kind::Table
                | Kind::View
                | Kind::MaterialisedView
                | Kind::Foreign
                | Kind::Search
                | Kind::Shadow
        )
    }

    /// Whether rows in it can be **changed**, before the key question is even asked. A view's rows
    /// belong to the tables underneath it, and a search index's belong to the index.
    ///
    /// A **shadow** table is the one of these that would otherwise pass every test: `docs_content` has
    /// an `INTEGER PRIMARY KEY`, so the addressability rule would say yes, and a hand-written `UPDATE`
    /// there would leave the row and the index that describes it disagreeing — with nothing to say so
    /// until a search returned the wrong passage. A search table itself is refused for the ordinary
    /// reason as well, having no key of its own.
    pub fn can_be_changed(self) -> bool {
        matches!(self, Kind::Table)
    }
}

/// One thing in a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    pub kind: Kind,
}

/// A table or view, once its columns have been asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    /// Empty for SQLite, which has no schemas in the PostgreSQL sense.
    pub schema: String,
    pub name: String,
    pub columns: Vec<Column>,
    /// The columns of the primary key, in key order.
    ///
    /// **This is what decides whether a row can be changed**, and it is why it is a list rather than
    /// a flag: a compound key needs every part of itself in the `WHERE` clause, and matching on one
    /// of two would update the wrong rows.
    pub key: Vec<String>,
    /// Which of these columns hold a vector, decided by the **schema** and never by the bytes.
    ///
    /// Empty for every table in every engine but one: Inillucent reads it from a search index's own
    /// `%_config`, which is what makes the difference between a column the grid draws as
    /// `768d · [0.0231, …] · |v| 1.000` and one it draws as `3072 bytes: 3f 00 00 00…`. A blob in any
    /// other column is left alone — a PNG's length also divides by four, and a grid that guessed would
    /// be lying in a way nobody could catch. See `crate::vector`.
    pub vector_columns: Vec<String>,
    /// The object this table belongs to, when it is another object's storage.
    ///
    /// A search index's five shadow tables name it here. It is what makes them **read only** despite
    /// having a perfectly good primary key: they are the index's own state, and editing one by hand
    /// would leave the row and the index that describes it disagreeing.
    pub owned_by: Option<String>,
}

impl Table {
    /// The name to put in a statement, quoted so that a name needing quotes works and a name that
    /// does not is unchanged in the console's own history.
    pub fn qualified(&self, quote: char) -> String {
        match self.schema.is_empty() {
            true => quoted(&self.name, quote),
            false => format!("{}.{}", quoted(&self.schema, quote), quoted(&self.name, quote)),
        }
    }

    /// Whether a row here can be addressed on its own.
    ///
    /// The whole of the editing rule: a row can only be changed if there is something that names it
    /// and nothing else. Anything else means an `UPDATE` matching on every column, which quietly
    /// changes two identical rows — see `tasks/task-1777-database-plugin-tdd.md` §6.3.
    pub fn can_be_changed(&self) -> bool {
        if self.owned_by.is_some() {
            return false;
        }
        !self.key.is_empty() && self.key.iter().all(|name| self.columns.iter().any(|column| column.name == *name))
    }

    /// Why not, for the line the grid shows in place of the buttons it does not draw.
    ///
    /// Two reasons rather than one, because they are different situations and the second is the more
    /// dangerous: a table with no key **cannot** be addressed, while a shadow table can be addressed
    /// perfectly well and must not be, since it is another object's own state.
    pub fn why_not_changeable(&self) -> Option<String> {
        if let Some(owner) = &self.owned_by {
            return Some(format!(
                "`{}` is where `{owner}` keeps its state, so it is read only here. Changing a row \
                 would leave `{owner}` describing something the row no longer says, and nothing would \
                 report it — write to `{owner}` instead and it will keep this in step.",
                self.name
            ));
        }
        match self.can_be_changed() {
            true => None,
            false => Some(format!(
                "`{}` has no primary key, so there is no way to change one row without changing \
                 every row that looks like it.",
                self.name
            )),
        }
    }

    /// Whether this column is one the schema says holds a vector.
    ///
    /// @param name - the column's name
    pub fn is_a_vector_column(&self, name: &str) -> bool {
        self.vector_columns.iter().any(|column| column == name)
    }
}

/// A name with quotes round it, doubling any quote already inside it.
///
/// Identifiers are the one place this crate builds SQL text rather than binding a parameter, because a
/// table name cannot be a parameter in any engine. Doubling is the escape both engines use, and the
/// quote character differs — `"` for PostgreSQL and SQLite, which is why it is an argument rather than
/// a constant.
pub fn quoted(name: &str, quote: char) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push(quote);
    for character in name.chars() {
        if character == quote {
            out.push(quote);
        }
        out.push(character);
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_table(key: &[&str]) -> Table {
        Table {
            schema: "public".to_owned(),
            name: "member".to_owned(),
            columns: vec![Column::new("id", "int4"), Column::new("name", "text")],
            key: key.iter().map(|name| (*name).to_owned()).collect(),
            ..Table::default()
        }
    }

    #[test]
    fn a_shadow_table_is_read_only_even_though_it_has_a_perfectly_good_key() {
        // The one case the addressability rule alone gets wrong. `docs_content` has an INTEGER
        // PRIMARY KEY, so every other test here would say yes — and a hand-written UPDATE would leave
        // the row and the search index that describes it disagreeing, with nothing to report it.
        let mut shadow = a_table(&["id"]);
        shadow.name = "docs_content".to_owned();
        assert!(shadow.can_be_changed(), "with no owner it is an ordinary table");
        shadow.owned_by = Some("docs".to_owned());
        assert!(!shadow.can_be_changed());
        let why = shadow.why_not_changeable().expect("a reason");
        assert!(why.contains("keeps its state"), "{why}");
        assert!(why.contains("`docs`"), "it names the owner to write to instead: {why}");
    }

    #[test]
    fn a_vector_column_is_the_schemas_claim_rather_than_the_grids_guess() {
        let mut table = a_table(&["id"]);
        assert!(!table.is_a_vector_column("name"), "nothing is a vector until the schema says so");
        table.vector_columns = vec!["vector".to_owned()];
        assert!(table.is_a_vector_column("vector"));
        assert!(!table.is_a_vector_column("name"));
    }

    #[test]
    fn the_two_kinds_the_search_engine_adds_hold_rows_and_refuse_edits() {
        assert!(Kind::Search.holds_rows());
        assert!(Kind::Shadow.holds_rows());
        assert!(!Kind::Search.can_be_changed());
        assert!(!Kind::Shadow.can_be_changed());
        assert_eq!(Kind::Search.folder(), "search");
        assert_eq!(Kind::Shadow.folder(), "shadow");
        assert_eq!(Kind::Search.name(), "search index");
    }

    #[test]
    fn a_table_with_no_key_cannot_be_changed_and_says_why() {
        assert!(a_table(&["id"]).can_be_changed());
        let no_key = a_table(&[]);
        assert!(!no_key.can_be_changed());
        assert!(no_key.why_not_changeable().unwrap().contains("no primary key"));
    }

    #[test]
    fn a_key_naming_a_column_that_is_not_there_is_not_a_key() {
        // Which happens when the columns were fetched and the key came from a stale read: better to
        // draw a read-only grid than to write a `WHERE` clause naming a column that does not exist.
        assert!(!a_table(&["id", "tenant"]).can_be_changed());
    }

    #[test]
    fn a_name_is_quoted_and_a_quote_inside_it_is_doubled() {
        assert_eq!(quoted("member", '"'), "\"member\"");
        assert_eq!(quoted("odd\"name", '"'), "\"odd\"\"name\"");
        assert_eq!(a_table(&["id"]).qualified('"'), "\"public\".\"member\"");
        assert_eq!(
            Table { schema: String::new(), name: "notes".to_owned(), ..Table::default() }.qualified('"'),
            "\"notes\""
        );
    }

    #[test]
    fn a_kind_says_where_it_hangs_and_whether_its_rows_can_be_changed() {
        assert_eq!(Kind::Table.folder(), "tables");
        assert_eq!(Kind::MaterialisedView.folder(), "views");
        assert!(Kind::View.holds_rows());
        // A view's rows belong to the tables underneath it, so they are read here and changed there.
        assert!(!Kind::View.can_be_changed());
        assert!(Kind::Table.can_be_changed());
        assert!(!Kind::Sequence.holds_rows());
    }
}
