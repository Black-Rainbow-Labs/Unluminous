// What `mcp tools`, `mcp config` and `mcp install` were asked for, read from a call's arguments.
//
// **One copy, read by both halves** (`task-1984` L4 and §5.6). The commands `unluminous-cli` answers
// without a window are implemented twice -- once in `main.rs` for a person's command line and once
// in `mcp::driver::locally` for an agent's tool call -- and the two already differed: `mcp tools`,
// `mcp config` and `mcp install` were declared `local: true`, offered as tools, answered by neither,
// and refused by the window with `unknown-command`. Three tools that resolve and can never succeed.
//
// So the reading of the arguments and the answer's own data live here, where both call them, and the
// only thing either half still owns is how it *prints*: a person gets a table and an agent gets JSON.
// Every function takes a `Map<String, Value>` because that is what a tool call carries and what
// `Typed::arguments` already is.

use serde_json::{json, Map, Value};

use super::install::{Client, Scope, Wanted};
use super::tools::Areas;
use super::{Shape, Transport, DEFAULT_PORT, MIN_PORT};

/// Which tool shape was asked for, or the default.
pub fn shape_from(arguments: &Map<String, Value>) -> Result<Shape, String> {
    match arguments.get("tools").and_then(Value::as_str) {
        Some(named) => Shape::parse(named)
            .ok_or_else(|| format!("`{named}` is not a tool shape. It is `grouped` or `every`.")),
        None => Ok(Shape::default()),
    }
}

/// Which areas the server was equipped with, or all of them.
///
/// `task-1804` §4.2. Refused rather than ignored when a name is not an area -- see
/// `mcp::tools::Areas::parse` for why.
pub fn areas_from(arguments: &Map<String, Value>) -> Result<Areas, String> {
    match arguments.get("areas").and_then(Value::as_str) {
        Some(named) => Areas::parse(named),
        None => Ok(Areas::all()),
    }
}

pub fn transport_from(arguments: &Map<String, Value>) -> Result<Transport, String> {
    match arguments.get("transport").and_then(Value::as_str) {
        Some(named) => Transport::parse(named)
            .ok_or_else(|| format!("`{named}` is not a transport. It is `stdio` or `http`.")),
        None => Ok(Transport::default()),
    }
}

pub fn port_from(arguments: &Map<String, Value>) -> Result<u16, String> {
    let Some(named) = arguments.get("port") else {
        return Ok(DEFAULT_PORT);
    };
    let named = match named {
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.trim().to_owned(),
        other => other.to_string(),
    };
    let port: u16 = named.parse().map_err(|_| format!("`{named}` is not a port number."))?;
    if port < MIN_PORT {
        return Err(format!(
            "{port} is below {MIN_PORT}, which needs privileges and is never what was meant."
        ));
    }
    Ok(port)
}

/// The server a caller wants written down: its name, its transport, its port and its scope.
pub fn wanted_from(arguments: &Map<String, Value>) -> Result<Wanted, String> {
    let mut wanted = Wanted {
        transport: transport_from(arguments)?,
        port: port_from(arguments)?,
        ..Wanted::default()
    };
    if let Some(name) = arguments.get("name").and_then(Value::as_str) {
        let name = name.trim();
        if name.is_empty() {
            return Err("A server needs a name.".to_owned());
        }
        wanted.name = name.to_owned();
    }
    if let Some(scope) = arguments.get("scope").and_then(Value::as_str) {
        wanted.scope = Scope::parse(scope)
            .ok_or_else(|| format!("`{scope}` is not a scope. It is `user` or `project`."))?;
    }
    Ok(wanted)
}

/// Which agents a caller named, or both of them.
pub fn clients_from(named: Option<&str>) -> Result<Vec<Client>, String> {
    match named.map(str::trim) {
        None | Some("") | Some("both") | Some("all") => Ok(Client::ALL.to_vec()),
        Some(name) => Client::parse(name).map(|client| vec![client]).ok_or_else(|| {
            format!("`{name}` is not a client Unluminous can write to. It is `claude`, `codex`, or `both`.")
        }),
    }
}

/// What `mcp tools --count` answers with: both shapes, always.
///
/// Both, because the number that matters is the comparison rather than either figure on its own.
/// Bytes divided by four is the usual rule of thumb for tokens and is called that rather than
/// dressed up as a measurement.
pub fn counted(areas: &Areas) -> Vec<Value> {
    [Shape::Grouped, Shape::Every]
        .iter()
        .map(|shape| {
            let tools = super::tools::as_json_in(*shape, areas);
            let bytes = serde_json::to_string(&tools).unwrap_or_default().len();
            json!({
                "shape": shape.name(),
                "tools": tools.len(),
                "bytes": bytes,
                "roughTokens": bytes / 4,
            })
        })
        .collect()
}

/// What `mcp config` answers with: one block per agent, and where each agent's file is.
pub fn configurations(clients: &[Client], wanted: &Wanted) -> Vec<Value> {
    clients
        .iter()
        .map(|client| {
            json!({
                "client": client.name(),
                "title": client.title(),
                "file": match client {
                    Client::Claude => {
                        super::install::claude_file(wanted).to_string_lossy().into_owned()
                    }
                    Client::Codex => super::install::codex_file().to_string_lossy().into_owned(),
                },
                "configuration": wanted.example(*client),
            })
        })
        .collect()
}
