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

/// `1` when [`TOKEN_VARIABLE`] is a password session for a board on another computer, which travels as a bearer
/// token, and `0` for the machine secret (task-2879).
pub const SESSION_VARIABLE: &str = "UNLUMINOUS_BOARD_SESSION";

/// The connection under the WebSocket: plain TCP to a board on this computer, or TLS to one on another.
pub enum Link {
    Plain(TcpStream),
    Tls(Box<native_tls::TlsStream<TcpStream>>),
}

impl Read for Link {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Link::Plain(stream) => stream.read(buffer),
            Link::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for Link {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        match self {
            Link::Plain(stream) => stream.write(buffer),
            Link::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Link::Plain(stream) => stream.flush(),
            Link::Tls(stream) => stream.flush(),
        }
    }
}

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
    let base = std::env::var(URL_VARIABLE)
        .map_err(|_| format!("{URL_VARIABLE} is not set, so there is no board to attach to"))?;
    let token = std::env::var(TOKEN_VARIABLE).unwrap_or_default();
    let session = std::env::var(SESSION_VARIABLE).is_ok_and(|value| value.trim() == "1");
    let mut socket = open(&base, id, &token, session)?;
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

/// The relay's address and the host and port to connect to, for an `http://` or `https://` board.
/// @param base - the service, such as `http://127.0.0.1:8096` or `https://tasks.jasonmcaffee.com`
/// @param id - the ticket
pub fn relay_address(base: &str, id: i64) -> Result<(String, String, u16, bool), String> {
    let base = base.trim_end_matches('/');
    let (secure, rest) = match (base.strip_prefix("https://"), base.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        _ => return Err(format!("{base} is not an http:// or https:// address")),
    };
    let host_port = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) => (
            host.to_owned(),
            port.parse().map_err(|_| format!("{base} has no port number after the colon"))?,
        ),
        None => (host_port.to_owned(), if secure { 443 } else { 80 }),
    };
    let url =
        format!("{}://{rest}/tasks/{id}/agent-terminal/ws", if secure { "wss" } else { "ws" });
    Ok((url, host, port, secure))
}

/// Open the relay's WebSocket: on loopback with the machine secret, or over TLS with a session for a board on
/// another computer (task-2879).
/// @param base - the service, as `http://host:port` or `https://host`
/// @param id - the ticket
/// @param token - the machine secret, or the session when `session` is set
/// @param session - whether `token` is a password session
fn open(base: &str, id: i64, token: &str, session: bool) -> Result<WebSocket<Link>, String> {
    let (url, host, port, secure) = relay_address(base, id)?;
    let mut request = url.as_str().into_client_request().map_err(|problem| problem.to_string())?;
    if !token.is_empty() {
        let (name, value) = match session {
            true => ("authorization", format!("Bearer {token}")),
            false => ("x-skip-token", token.to_owned()),
        };
        let value =
            value.parse().map_err(|_| "the board's credential is not a valid header".to_owned())?;
        request.headers_mut().insert(name, value);
    }
    let stream = TcpStream::connect((host.as_str(), port))
        .map_err(|problem| format!("agent-tasks-service at {base} did not answer: {problem}"))?;
    stream.set_read_timeout(Some(POLL)).map_err(|problem| problem.to_string())?;
    let link = match secure {
        true => {
            let connector =
                native_tls::TlsConnector::new().map_err(|problem| problem.to_string())?;
            // The handshake runs with the short read timeout already set, so it is retried until it finishes.
            let mut attempt = connector.connect(&host, stream);
            let tls = loop {
                match attempt {
                    Ok(tls) => break tls,
                    Err(native_tls::HandshakeError::WouldBlock(middle)) => {
                        attempt = middle.handshake()
                    }
                    Err(native_tls::HandshakeError::Failure(problem)) => {
                        return Err(format!("agent-tasks-service at {base} refused TLS: {problem}"))
                    }
                }
            };
            Link::Tls(Box::new(tls))
        }
        false => Link::Plain(stream),
    };
    let mut handshake = tungstenite::client(request, link);
    loop {
        match handshake {
            Ok((socket, _)) => return Ok(socket),
            Err(tungstenite::HandshakeError::Interrupted(middle)) => handshake = middle.handshake(),
            Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))) => {
                return Err(refusal(response.status().as_u16()))
            }
            Err(tungstenite::HandshakeError::Failure(other)) => {
                return Err(format!("the ticket's terminal could not be opened: {other}"))
            }
        }
    }
}

/// What the relay's refusal of the upgrade means to a person.
/// @param status - the HTTP status
pub fn refusal(status: u16) -> String {
    match status {
        401 => "agent-tasks-service refused this window's credential. On a board on another computer, sign in again with `plugins run agent-tasks sign-in`.".to_owned(),
        404 => "This ticket has no agent terminal on agent-tasks-service. Start it first.".to_owned(),
        409 => "The ticket's agent is not running on agent-tasks-service. Start or resume it.".to_owned(),
        other => format!("agent-tasks-service refused the terminal ({other})"),
    }
}

/// Read what has arrived and write it out. Answers `Some` with a sentence when the terminal has ended.
/// @param socket - the relay
fn read_the_socket(socket: &mut WebSocket<Link>) -> Result<Option<String>, String> {
    match socket.read() {
        Ok(Message::Text(text)) => Ok(show(parse_or_skip(&text))),
        Ok(Message::Close(_)) => Ok(Some("agent-tasks-service closed the terminal".to_owned())),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(problem))
            if matches!(
                problem.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            Ok(None)
        }
        Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
            Ok(Some("agent-tasks-service closed the terminal".to_owned()))
        }
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
        Frame::Exit(code) => {
            return Some(format!(
                "The agent exited{}",
                code.map(|code| format!(" with code {code}")).unwrap_or_default()
            ))
        }
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
fn send(socket: &mut WebSocket<Link>, frame: String) -> Result<(), String> {
    socket
        .send(Message::Text(frame))
        .map_err(|problem| format!("the ticket's terminal stopped taking keys: {problem}"))
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
            match libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) == 0
                && size.ws_col > 0
            {
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
        assert!(refusal(401).contains("credential"));
        assert!(refusal(500).contains("500"));
    }

    #[test]
    fn a_board_on_this_computer_is_plain_and_one_on_another_is_tls() {
        assert_eq!(
            relay_address("http://127.0.0.1:8096", 7).unwrap(),
            (
                "ws://127.0.0.1:8096/tasks/7/agent-terminal/ws".to_owned(),
                "127.0.0.1".to_owned(),
                8096,
                false
            )
        );
        assert_eq!(
            relay_address("https://tasks.jasonmcaffee.com/", 7).unwrap(),
            (
                "wss://tasks.jasonmcaffee.com/tasks/7/agent-terminal/ws".to_owned(),
                "tasks.jasonmcaffee.com".to_owned(),
                443,
                true
            )
        );
        assert!(relay_address("tasks.jasonmcaffee.com", 7).is_err());
    }

    #[test]
    fn an_exit_ends_the_terminal_and_output_does_not() {
        assert_eq!(
            show(Some(Frame::Exit(Some(0)))),
            Some("The agent exited with code 0".to_owned())
        );
        assert_eq!(show(Some(Frame::Data(String::new()))), None);
        assert_eq!(show(None), None);
    }
}
