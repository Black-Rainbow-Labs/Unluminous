//! The program a ticket's terminal runs when the board is agent-tasks-service's: `unluminous --board-terminal
//! <task id>`.
//!
//! The service's terminal daemon runs the agent, and `GET /tasks/:id/agent-terminal/ws` relays its screen. This
//! joins that relay to the pseudoterminal Unluminous gave it: what the relay sends is written to the output, and
//! what is typed is sent as `input` frames. So the ticket modal's own terminal, its scrollback, the menu buttons
//! and `plugins run agent-tasks terminal` all work on the service's agent, and closing the terminal in Unluminous
//! leaves the agent running in the service. `tasks/agent-tasks-through-the-service-tdd.md` is the design.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use tasks_client::terminal_frames::{self, Frame};
use tungstenite::{client::IntoClientRequest, Message, WebSocket};

/// The switch `main` looks for before anything else.
pub const SWITCH: &str = "--board-terminal";

/// The service's address, such as `http://127.0.0.1:8096`, in the attach client's environment.
pub const URL_VARIABLE: &str = "UNLUMINOUS_BOARD_URL";

/// The machine secret the request carries, in the attach client's environment.
pub const TOKEN_VARIABLE: &str = "UNLUMINOUS_BOARD_TOKEN";

/// How long a read of the socket waits before the loop looks at the keyboard and the size again.
const POLL: Duration = Duration::from_millis(20);

/// Join a ticket's terminal on the service to this process's terminal, and answer the exit code.
/// @param task - the ticket's id, as it was typed after the switch
pub fn run(task: &str) -> i32 {
    match attach(task) {
        Ok(said) => {
            println!("\r\n{said}\r");
            0
        }
        Err(problem) => {
            println!("\r\n{problem}\r");
            1
        }
    }
}

/// Connect, then pass frames until the agent exits or the connection ends. Answers what ended it.
/// @param task - the ticket's id
fn attach(task: &str) -> Result<String, String> {
    let id: i64 = task.trim().parse().map_err(|_| format!("`{task}` is not a ticket id"))?;
    let base = std::env::var(URL_VARIABLE).map_err(|_| format!("{URL_VARIABLE} is not set, so there is no board to attach to"))?;
    let token = std::env::var(TOKEN_VARIABLE).unwrap_or_default();
    let mut socket = open(&base, id, &token)?;
    let _raw = raw::Mode::enter();
    let keys = read_the_keyboard();
    let mut size = raw::size();
    if let Some((columns, rows)) = size {
        send(&mut socket, terminal_frames::resize_frame(columns, rows))?;
    }
    loop {
        if let Some(ended) = read_the_socket(&mut socket)? {
            return Ok(ended);
        }
        for bytes in keys.try_iter() {
            send(&mut socket, terminal_frames::input_frame(&bytes))?;
        }
        let now = raw::size();
        if now != size {
            size = now;
            if let Some((columns, rows)) = now {
                send(&mut socket, terminal_frames::resize_frame(columns, rows))?;
            }
        }
    }
}

/// Open the relay's WebSocket on loopback with the machine secret.
/// @param base - the service, as `http://host:port`
/// @param id - the ticket
/// @param token - the machine secret
fn open(base: &str, id: i64, token: &str) -> Result<WebSocket<TcpStream>, String> {
    let host = base.trim_end_matches('/').strip_prefix("http://").ok_or_else(|| format!("{base} is not an http:// address"))?;
    let url = format!("ws://{host}/tasks/{id}/agent-terminal/ws");
    let mut request = url.as_str().into_client_request().map_err(|problem| problem.to_string())?;
    if !token.is_empty() {
        let value = token.parse().map_err(|_| "the board's secret is not a valid header".to_owned())?;
        request.headers_mut().insert("x-skip-token", value);
    }
    let stream = TcpStream::connect(host).map_err(|problem| format!("agent-tasks-service at {base} did not answer: {problem}"))?;
    let (socket, _) = tungstenite::client(request, stream).map_err(|problem| match problem {
        tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response)) => refusal(response.status().as_u16()),
        other => format!("the ticket's terminal could not be opened: {other}"),
    })?;
    socket.get_ref().set_read_timeout(Some(POLL)).map_err(|problem| problem.to_string())?;
    Ok(socket)
}

/// What the relay's refusal of the upgrade means to a person.
/// @param status - the HTTP status
pub fn refusal(status: u16) -> String {
    match status {
        401 => "agent-tasks-service refused this window's secret".to_owned(),
        404 => "This ticket has no agent terminal on agent-tasks-service. Start it first.".to_owned(),
        409 => "The ticket's agent is not running on agent-tasks-service. Start or resume it.".to_owned(),
        other => format!("agent-tasks-service refused the terminal ({other})"),
    }
}

/// Read what has arrived and write it out. Answers `Some` with a sentence when the terminal has ended.
/// @param socket - the relay
fn read_the_socket(socket: &mut WebSocket<TcpStream>) -> Result<Option<String>, String> {
    match socket.read() {
        Ok(Message::Text(text)) => Ok(show(parse_or_skip(&text))),
        Ok(Message::Close(_)) => Ok(Some("agent-tasks-service closed the terminal".to_owned())),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(problem)) if matches!(problem.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Ok(None),
        Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => Ok(Some("agent-tasks-service closed the terminal".to_owned())),
        Err(problem) => Err(format!("the connection to the ticket's terminal ended: {problem}")),
    }
}

/// A text frame as a [`Frame`], or `None` for one this client does not use.
/// @param text - the frame
fn parse_or_skip(text: &str) -> Option<Frame> {
    terminal_frames::parse_frame(text)
}

/// Write one frame to the terminal. Answers `Some` when the agent exited.
/// @param frame - the frame, if it was one this client uses
fn show(frame: Option<Frame>) -> Option<String> {
    let mut out = std::io::stdout();
    match frame? {
        // The screen as it is now. Cleared first, so a replay after a resync is not drawn over the old one.
        Frame::Replay { data, .. } => {
            let _ = out.write_all(b"\x1b[H\x1b[2J");
            let _ = out.write_all(data.as_bytes());
        }
        Frame::Data(data) => {
            let _ = out.write_all(data.as_bytes());
        }
        Frame::Exit(code) => return Some(format!("The agent exited{}", code.map(|code| format!(" with code {code}")).unwrap_or_default())),
        Frame::Error(message) => {
            let _ = write!(out, "\r\n\x1b[31m{message}\x1b[0m\r\n");
        }
    }
    let _ = out.flush();
    None
}

/// Send one frame.
/// @param socket - the relay
/// @param frame - the frame's JSON
fn send(socket: &mut WebSocket<TcpStream>, frame: String) -> Result<(), String> {
    socket.send(Message::Text(frame)).map_err(|problem| format!("the ticket's terminal stopped taking keys: {problem}"))
}

/// Read the keyboard on a thread of its own, because a read of standard input blocks.
fn read_the_keyboard() -> Receiver<Vec<u8>> {
    let (keys, pressed) = channel();
    std::thread::spawn(move || {
        let mut input = std::io::stdin();
        let mut buffer = [0u8; 4096];
        while let Ok(count) = input.read(&mut buffer) {
            if count == 0 || keys.send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    pressed
}

/// The terminal's raw mode and size. Raw mode passes every key, Ctrl+C and Escape included, to the agent
/// rather than to this process.
#[cfg(unix)]
mod raw {
    /// Raw mode while it lives, and the mode before it when it is dropped.
    pub struct Mode(Option<libc::termios>);

    impl Mode {
        /// Put standard input in raw mode. Does nothing when it is not a terminal.
        pub fn enter() -> Mode {
            // SAFETY: `tcgetattr` and `tcsetattr` read and write a `termios` this function owns, on standard input.
            unsafe {
                let mut before: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(libc::STDIN_FILENO, &mut before) != 0 {
                    return Mode(None);
                }
                let mut raw = before;
                libc::cfmakeraw(&mut raw);
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
                Mode(Some(before))
            }
        }
    }

    impl Drop for Mode {
        fn drop(&mut self) {
            if let Some(before) = self.0 {
                // SAFETY: puts back the `termios` that `enter` read from the same descriptor.
                unsafe {
                    libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &before);
                }
            }
        }
    }

    /// The terminal's size as `(columns, rows)`, or `None` when it is not a terminal.
    pub fn size() -> Option<(u16, u16)> {
        // SAFETY: `TIOCGWINSZ` writes a `winsize` this function owns.
        unsafe {
            let mut size: libc::winsize = std::mem::zeroed();
            match libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) == 0 && size.ws_col > 0 {
                true => Some((size.ws_col, size.ws_row)),
                false => None,
            }
        }
    }
}

/// On Windows the released `unluminous.exe` is a windows subsystem program, which a pseudoconsole gives no
/// console to, so there is nothing to put in raw mode. agent-tasks-service runs on a Mac.
#[cfg(not(unix))]
mod raw {
    pub struct Mode;

    impl Mode {
        pub fn enter() -> Mode {
            Mode
        }
    }

    pub fn size() -> Option<(u16, u16)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_refusal_says_what_to_do() {
        assert!(refusal(409).contains("Start or resume"));
        assert!(refusal(404).contains("Start it first"));
        assert!(refusal(401).contains("secret"));
        assert!(refusal(500).contains("500"));
    }

    #[test]
    fn an_exit_ends_the_terminal_and_output_does_not() {
        assert_eq!(show(Some(Frame::Exit(Some(0)))), Some("The agent exited with code 0".to_owned()));
        assert_eq!(show(Some(Frame::Data(String::new()))), None);
        assert_eq!(show(None), None);
    }
}
