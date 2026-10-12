//! Whether a console's `SELECT` reads the rows of one table, and if it does, which table and how.
//!
//! `task-2253`: *"The db explorer has results when I execute a query. That should be the same result format as
//! when I double click on a table, so i can edit, sort, save, etc."* A row can only be changed when it can be
//! addressed, so a console result can only be edited when it is a table's own rows: one table, its own columns,
//! nothing grouped, joined, combined or computed. This module is the question, asked of the text.
//!
//! **It refuses anything it does not recognise.** A statement this reads wrongly would put an editable grid over
//! rows that are not the table's, so the reading accepts one shape and nothing else:
//!
//! ```sql
//! select <*, or column names> from [schema.]table
//!   [where <anything>] [order by <anything>] [limit <n>] [offset <n>]
//! ```
//!
//! A `DISTINCT`, an alias, a join, a second table, a `GROUP BY`, a `UNION`, a function in the column list or a
//! subquery in the `FROM` are all answered `None`, and the console then shows its rows read only.
//!
//! The words are read at the top level only. A `(` opens a level and a `)` closes it, and a string, a quoted
//! identifier, a comment and a dollar-quoted body are passed over whole by the same function the statement
//! splitter uses, so a `from` inside a string or a subquery in the `WHERE` cannot be mistaken for the clause.

use crate::sql::{skip_over, strip_comments};

/// A `SELECT` of one table's own rows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OneTable {
    /// The column names as they were written, quotes and all. Empty for `*`.
    pub columns: Vec<String>,
    /// The schema the table was named in, with its quotes taken off, when it was named in one.
    pub schema: Option<String>,
    /// The table's name, with its quotes taken off.
    pub name: String,
    /// What follows `WHERE`, as written. Empty when there is none.
    pub where_clause: String,
    /// What follows `ORDER BY`, as written. Empty when there is none.
    pub order_by: String,
    /// The `LIMIT`, when there is one.
    pub limit: Option<usize>,
    /// The `OFFSET`, zero when there is none.
    pub offset: usize,
}

/// One word, name, number or mark at a known place in the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Quoted,
    Number,
    Literal,
    Mark(char),
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
    /// How many brackets are open around it.
    depth: u32,
}

/// Words that mean the rows are not one table's own, wherever they appear at the top level.
const NOT_ONE_TABLE: &[&str] = &[
    "DISTINCT",
    "ALL",
    "INTO",
    "JOIN",
    "GROUP",
    "HAVING",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "WINDOW",
    "FOR",
    "FETCH",
    "RETURNING",
    "LATERAL",
    "NATURAL",
    "TABLESAMPLE",
];

/// Read a statement, and say which table it reads when it reads exactly one table's own rows.
///
/// @param statement - one statement, as `sql::Statement::to_send` gives it
pub fn read(statement: &str) -> Option<OneTable> {
    let text = strip_comments(statement);
    let text = text.trim().trim_end_matches(';').trim_end();
    let tokens = tokens(text)?;
    let top: Vec<Token> = tokens.iter().copied().filter(|token| token.depth == 0).collect();
    let word = |token: &Token, wanted: &str| {
        token.kind == Kind::Word && text[token.start..token.end].eq_ignore_ascii_case(wanted)
    };
    if !top.first().is_some_and(|first| word(first, "SELECT")) {
        return None;
    }
    if top.iter().any(|token| {
        token.kind == Kind::Word
            && NOT_ONE_TABLE
                .iter()
                .any(|bad| text[token.start..token.end].eq_ignore_ascii_case(bad))
    }) {
        return None;
    }
    let from = top.iter().position(|token| word(token, "FROM"))?;
    let columns = the_columns(text, &tokens, &top, from)?;
    let (schema, name, after) = the_table(text, &top, from + 1)?;
    let mut read = OneTable { columns, schema, name, ..OneTable::default() };
    the_clauses(text, &top[after..], &mut read)?;
    Some(read)
}

/// The column list between `SELECT` and `FROM`: `*` alone, or plain names separated by commas.
fn the_columns(text: &str, tokens: &[Token], top: &[Token], from: usize) -> Option<Vec<String>> {
    // A bracket anywhere in the list is a function or a subquery, and neither is a column of the table.
    let first = top[0].end;
    let last = top[from].start;
    if tokens.iter().any(|token| token.start >= first && token.end <= last && token.depth > 0) {
        return None;
    }
    let listed = &top[1..from];
    if listed.len() == 1 && listed[0].kind == Kind::Mark('*') {
        return Some(Vec::new());
    }
    let mut columns = Vec::new();
    for item in listed.split(|token| token.kind == Kind::Mark(',')) {
        match item {
            [one] if matches!(one.kind, Kind::Word | Kind::Quoted) => {
                columns.push(text[one.start..one.end].to_owned());
            }
            _ => return None,
        }
    }
    Some(columns)
}

/// The table after `FROM`, and where the tokens after it start.
fn the_table(text: &str, top: &[Token], at: usize) -> Option<(Option<String>, String, usize)> {
    let named = |token: Option<&Token>| {
        token
            .filter(|token| matches!(token.kind, Kind::Word | Kind::Quoted))
            .map(|token| unquote(&text[token.start..token.end]))
    };
    let first = named(top.get(at))?;
    if top.get(at + 1).is_some_and(|token| token.kind == Kind::Mark('.')) {
        let second = named(top.get(at + 2))?;
        return Some((Some(first), second, at + 3));
    }
    Some((None, first, at + 1))
}

/// `WHERE`, `ORDER BY`, `LIMIT` and `OFFSET`, each at most once and in that order, and nothing else.
fn the_clauses(text: &str, rest: &[Token], read: &mut OneTable) -> Option<()> {
    // Which clause each top level word opens, by its rank in the order they must come in.
    let rank = |token: &Token| -> Option<usize> {
        if token.kind != Kind::Word {
            return None;
        }
        let said = &text[token.start..token.end];
        ["WHERE", "ORDER", "LIMIT", "OFFSET"]
            .iter()
            .position(|word| said.eq_ignore_ascii_case(word))
    };
    let mut opened: Vec<(usize, usize)> = Vec::new();
    for (index, token) in rest.iter().enumerate() {
        if let Some(rank) = rank(token) {
            if opened.last().is_some_and(|(last, _)| *last >= rank) {
                return None;
            }
            opened.push((rank, index));
        }
    }
    // Something between the table and the first clause is an alias, a comma or a join.
    match opened.first() {
        Some((_, 0)) => {}
        None if rest.is_empty() => return Some(()),
        _ => return None,
    }
    for (place, (rank, index)) in opened.iter().enumerate() {
        let end = opened.get(place + 1).map_or(rest.len(), |(_, next)| *next);
        let mut body = &rest[index + 1..end];
        if *rank == 1 {
            let by = body.first()?;
            if !(by.kind == Kind::Word && text[by.start..by.end].eq_ignore_ascii_case("BY")) {
                return None;
            }
            body = &body[1..];
        }
        let (first, last) = (body.first()?, body.last()?);
        let words = text[first.start..last.end].trim().to_owned();
        match rank {
            0 => read.where_clause = words,
            1 => read.order_by = words,
            _ => {
                let [number] = body else { return None };
                if number.kind != Kind::Number {
                    return None;
                }
                let value = text[number.start..number.end].parse::<usize>().ok()?;
                match rank {
                    2 => read.limit = Some(value),
                    _ => read.offset = value,
                }
            }
        }
    }
    Some(())
}

/// The tokens of a statement, with how deep in brackets each one is. `None` when the brackets do not
/// balance, which is not a statement this module can say anything about.
fn tokens(text: &str) -> Option<Vec<Token>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut depth: u32 = 0;
    let mut index = 0;
    while index < bytes.len() {
        let character = text[index..].chars().next()?;
        let width = character.len_utf8();
        if character.is_whitespace() {
            index += width;
            continue;
        }
        if let Some(after) = skip_over(text, index) {
            let kind = match bytes[index] {
                b'"' | b'`' => Kind::Quoted,
                _ => Kind::Literal,
            };
            out.push(Token { kind, start: index, end: after, depth });
            index = after;
            continue;
        }
        if character.is_ascii_digit() {
            let mut end = index;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'.') {
                end += 1;
            }
            out.push(Token { kind: Kind::Number, start: index, end, depth });
            index = end;
            continue;
        }
        if character.is_alphabetic() || character == '_' {
            let mut end = index;
            for (offset, next) in text[index..].char_indices() {
                if !(next.is_alphanumeric() || next == '_' || next == '$') {
                    break;
                }
                end = index + offset + next.len_utf8();
            }
            out.push(Token { kind: Kind::Word, start: index, end, depth });
            index = end;
            continue;
        }
        match character {
            '(' => {
                out.push(Token { kind: Kind::Mark('('), start: index, end: index + 1, depth });
                depth += 1;
            }
            ')' => {
                depth = depth.checked_sub(1)?;
                out.push(Token { kind: Kind::Mark(')'), start: index, end: index + 1, depth });
            }
            other => {
                out.push(Token { kind: Kind::Mark(other), start: index, end: index + width, depth })
            }
        }
        index += width;
    }
    (depth == 0).then_some(out)
}

/// Whether a statement that returns rows can be read again inside `select * from (...)`, which is how a
/// console's grid filters, sorts and pages a result that is not one table's own rows.
///
/// A `SELECT`, a `VALUES` or a `WITH`, and only when no word in it writes: a `WITH` holding a `DELETE` would
/// run the delete again on every reload, and PostgreSQL refuses one inside a subquery anyway. Anything else,
/// such as a `PRAGMA`, an `EXPLAIN` or an `INSERT ... RETURNING`, is answered once and shown as it came.
///
/// @param statement - one statement, as `sql::Statement::to_send` gives it
pub fn can_be_read_again(statement: &str) -> bool {
    let text = strip_comments(statement);
    let text = text.trim().trim_end_matches(';').trim_end();
    let Some(tokens) = tokens(text) else { return false };
    let words: Vec<&str> = tokens
        .iter()
        .filter(|token| token.kind == Kind::Word)
        .map(|token| &text[token.start..token.end])
        .collect();
    let starts = words.first().is_some_and(|first| {
        ["SELECT", "VALUES", "WITH"].iter().any(|verb| first.eq_ignore_ascii_case(verb))
    });
    let writes = words.iter().any(|word| {
        ["INSERT", "UPDATE", "DELETE", "MERGE", "INTO", "FOR"]
            .iter()
            .any(|bad| word.eq_ignore_ascii_case(bad))
    });
    starts && !writes
}

/// A name with its quotes taken off: `"Order Lines"` is `Order Lines`, and `member` is `member`.
///
/// @param name - a name as it was written
pub fn unquote(name: &str) -> String {
    for quote in ['"', '`'] {
        if name.len() >= 2 && name.starts_with(quote) && name.ends_with(quote) {
            let inner = &name[1..name.len() - 1];
            return inner.replace(&format!("{quote}{quote}"), &quote.to_string());
        }
    }
    name.to_owned()
}

/// Whether a written name is quoted, which decides whether it is compared with its case or without.
///
/// @param name - a name as it was written
pub fn is_quoted(name: &str) -> bool {
    name.starts_with('"') || name.starts_with('`')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str) -> OneTable {
        OneTable { name: name.to_owned(), ..OneTable::default() }
    }

    #[test]
    fn a_select_of_every_column_is_one_table() {
        assert_eq!(read("select * from member"), Some(table("member")));
        assert_eq!(read("SELECT * FROM member;"), Some(table("member")));
        assert_eq!(
            read("select * from public.member"),
            Some(OneTable { schema: Some("public".to_owned()), ..table("member") })
        );
        assert_eq!(read("select * from \"Order \"\"Lines\"\"\""), Some(table("Order \"Lines\"")));
    }

    #[test]
    fn the_clauses_are_kept_as_written() {
        let read = read(
            "select id, \"Name\" from member where name like 'A%' and id in (1, 2) \
             order by name desc, id limit 10 offset 20",
        )
        .expect("one table");
        assert_eq!(read.columns, ["id", "\"Name\""]);
        assert_eq!(read.where_clause, "name like 'A%' and id in (1, 2)");
        assert_eq!(read.order_by, "name desc, id");
        assert_eq!(read.limit, Some(10));
        assert_eq!(read.offset, 20);
    }

    #[test]
    fn a_word_inside_a_string_a_comment_or_a_subquery_is_not_a_clause() {
        let read = read(
            "select * from member -- from other\n\
             where name = 'join from x order by y' and id in (select id from other group by id)",
        )
        .expect("one table");
        assert_eq!(read.name, "member");
        assert_eq!(
            read.where_clause,
            "name = 'join from x order by y' and id in (select id from other group by id)"
        );
        assert!(read.order_by.is_empty());
    }

    #[test]
    fn anything_that_is_not_one_tables_own_rows_is_refused() {
        for refused in [
            "select distinct name from member",
            "select count(*) from member",
            "select name || 'x' from member",
            "select name as n from member",
            "select name n from member",
            "select * from member m",
            "select * from member as m",
            "select * from member, other",
            "select * from member join other on true",
            "select * from member left join other on true",
            "select * from (select * from member) s",
            "select name from member group by name",
            "select * from member union select * from other",
            "select * from member limit all",
            "select * from member limit 5, 10",
            "select * from member order by id limit 5 where id = 1",
            "select 1",
            "update member set name = 'x'",
            "with x as (select 1) select * from x",
            "select * from member for update",
            "select * from generate_series(1, 3)",
            "select * from member where (id = 1",
        ] {
            assert_eq!(read(refused), None, "{refused}");
        }
    }

    #[test]
    fn only_a_statement_that_reads_is_read_again() {
        assert!(can_be_read_again("select a.x, b.y from a join b on a.id = b.id"));
        assert!(can_be_read_again("with x as (select 1 as a) select a from x"));
        assert!(can_be_read_again("values (1), (2)"));
        assert!(!can_be_read_again("with d as (delete from a returning *) select * from d"));
        assert!(!can_be_read_again("insert into a values (1) returning *"));
        assert!(!can_be_read_again("pragma table_info(a)"));
        assert!(!can_be_read_again("explain select 1"));
        assert!(!can_be_read_again("select * into b from a"));
    }

    #[test]
    fn a_name_loses_its_quotes() {
        assert_eq!(unquote("\"a\"\"b\""), "a\"b");
        assert_eq!(unquote("plain"), "plain");
        assert!(is_quoted("\"x\""));
        assert!(!is_quoted("x"));
    }
}
