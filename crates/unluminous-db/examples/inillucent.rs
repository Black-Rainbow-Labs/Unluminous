//! Build a real Inillucent database and read every part of it back.
//!
//! The unit tests in `src/inillucent/` are evidence that the seam is right; this is how a person
//! looks at the whole thing at once, and it is the command to reach for when the engine is upgraded
//! and something needs checking by hand. It is a command rather than a test for the reason
//! `examples/connect.rs` gives about a server — except that here the reason is the opposite one:
//! this *always* works, because the engine is a library in the process, and what a person wants from
//! it is to read the output rather than a pass or a fail.
//!
//! ```text
//! cargo run -p unluminous-db --example inillucent
//! cargo run -p unluminous-db --example inillucent -- C:\somewhere\notes.rdb
//! ```
//!
//! With a path it opens that database and reads it; with none it builds one in the temporary folder
//! first, with a search index and three vectors in it.

use unluminous_db::source::Source;
use unluminous_db::{Database, Kind, Value, Vector};

fn main() {
    let path = match std::env::args().nth(1) {
        Some(path) => std::path::PathBuf::from(path),
        None => build_one(),
    };
    println!("reading {}", path.display());

    let source = match Source::parse("check", &path.to_string_lossy()) {
        Ok(source) => source,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(1);
        }
    };
    println!("engine: {}   {}", source.engine.name(), source.where_it_points());
    if !source.engine.can_stop_a_statement() {
        println!("a statement on this engine cannot be stopped, so no Stop button is drawn for it");
    }

    let mut database = match Database::connect(&source, None) {
        Ok(database) => database,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(1);
        }
    };
    println!("connected to {}", database.version());

    for entry in database.capabilities().iter().filter(|entry| entry.support.name() != "yes") {
        println!("  not fully supported: {:<16} {}", entry.name, entry.support.name());
    }

    let schemas = database.schemas().expect("schemas");
    for schema in &schemas {
        let items = database.items(schema).expect("items");
        println!("\n{schema}: {} items", items.len());
        for item in &items {
            let mark = match item.kind {
                Kind::Search => "  <-- a search index",
                Kind::Shadow => "  (its state)",
                _ => "",
            };
            println!("  {:<14} {}{mark}", item.kind.name(), item.name);
        }
        for item in items.iter().filter(|item| item.kind == Kind::Search) {
            let index = database.search_index(&item.name).expect("asked").expect("a search index");
            println!("\n{} declares {}", item.name, index.summary());
            let table = database.table(schema, &item.name).expect("a table");
            println!(
                "  columns: {}",
                table
                    .columns
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!("  vector columns: {:?}", table.vector_columns);
            println!(
                "  editable: {} ({})",
                table.can_be_changed(),
                table.why_not_changeable().unwrap_or_default()
            );

            // The vectors themselves, which is what the whole thing is for.
            let statement = format!(
                "select rowid, {} from {}",
                unluminous_db::catalog::quoted(
                    table.vector_columns.first().map_or("vector", String::as_str),
                    '"'
                ),
                unluminous_db::catalog::quoted(&item.name, '"')
            );
            let rows = database.query(&statement, 10).expect("rows");
            for row in &rows.rows {
                let at = row.first().and_then(Value::text).unwrap_or_default();
                match row.get(1).and_then(Value::bytes).and_then(Vector::decode) {
                    Some(vector) => println!("  row {at}: {}", vector.summary()),
                    None => println!("  row {at}: no vector"),
                }
            }

            // And the engine's own search, which is the other half of what a search table is for.
            let asked = format!(
                "select rowid, rank from {} where {} match 'release' order by rank limit 3",
                unluminous_db::catalog::quoted(&item.name, '"'),
                unluminous_db::catalog::quoted(&item.name, '"')
            );
            match database.query(&asked, 10) {
                Ok(found) => println!(
                    "  match 'release': {} row(s), best rank {}",
                    found.rows.len(),
                    found
                        .rows
                        .first()
                        .and_then(|row| row.get(1))
                        .and_then(Value::text)
                        .unwrap_or("-")
                ),
                Err(why) => println!("  match 'release' refused: {why}"),
            }
        }
    }

    // What the engine has not built keeps its own words, which is the distinction the driver exists
    // to make and the one a caller must not lose.
    match database.query("VACUUM", 0) {
        Ok(_) => println!("\nVACUUM ran, so the engine has grown it since this was written"),
        Err(why) => println!("\nVACUUM -> [{}] {}", why.code, why.message),
    }
}

/// Build a database with a search index and three vectors in it.
fn build_one() -> std::path::PathBuf {
    let folder = std::env::temp_dir().join("unluminous-db-inillucent-example");
    let _ = std::fs::create_dir_all(&folder);
    let file = folder.join("notes.rdb");
    let _ = std::fs::remove_file(&file);
    let database = inillucent_driver::Database::open(&file).expect("a database");
    let connection = database.connect();
    connection
        .execute_batch(
            "create table member (id integer primary key, name text not null, joined text);
             insert into member (id, name, joined) values (1, 'Jason', '2026-01-04'), (2, 'Ada', null);
             create virtual table docs using inillucent_search(title, body, dims = 8, mode = 'exact', metric = 'cosine');",
        )
        .expect("a schema");
    for (nth, (title, body)) in [
        ("the release process", "how a release is cut, tagged and published"),
        ("the buffer pool", "frames, version latches and the cooling FIFO"),
        ("drawing a vector", "an embedding is a blob until something reads it as numbers"),
    ]
    .iter()
    .enumerate()
    {
        // Unit vectors, so every norm reads 1.000 and an unnormalised corpus would stand out.
        let mut values = [0.0f32; 8];
        for (at, slot) in values.iter_mut().enumerate() {
            *slot = ((nth * 8 + at) as f32 * 0.37).sin();
        }
        let length: f32 = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        let mut bytes = Vec::new();
        for value in values.iter() {
            bytes.extend_from_slice(&(value / length).to_le_bytes());
        }
        connection
            .execute(
                "insert into docs(title, body, vector) values(?1, ?2, ?3)",
                &[
                    inillucent_driver::Value::Text((*title).to_owned()),
                    inillucent_driver::Value::Text((*body).to_owned()),
                    inillucent_driver::Value::Blob(bytes),
                ],
            )
            .expect("a row with a vector");
    }
    database.checkpoint().expect("checkpointed");
    file
}
