//! The wire: `Content-Length` framed JSON in, and either framing out.
//!
//! LSP frames both directions. tsserver reads one JSON object a line and writes frames. The decoder
//! is pure (bytes in, messages out) so every split a pipe produces is a test with no process behind it.
//! It looks for the next `Content-Length:` rather than assuming a frame starts where the last one ended,
//! because a server may print a warning on standard output before its first frame, and tsserver ends its
//! body with a line break that its length counts.

use serde_json::Value;

const HEADER: &[u8] = b"Content-Length:";
const SEPARATOR: &[u8] = b"\r\n\r\n";
/// The largest body read; a corrupted length must not allocate the machine's memory.
const LIMIT: usize = 64 * 1024 * 1024;

/// Reads frames out of whatever bytes have arrived.
#[derive(Default)]
pub(crate) struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    /// Adds bytes and returns every message now complete, oldest first. A frame that is not JSON is an
    /// `Err` carrying what was seen, and reading goes on after it.
    ///
    /// @param bytes - what the pipe just gave
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<Result<Value, String>> {
        self.buffer.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(message) = self.next_frame() {
            out.push(message);
        }
        out
    }

    /// The next complete frame, or `None` when more bytes are needed.
    fn next_frame(&mut self) -> Option<Result<Value, String>> {
        let Some(start) = find_ignoring_case(&self.buffer, HEADER) else {
            let keep = HEADER.len() - 1;
            if self.buffer.len() > keep {
                self.buffer.drain(..self.buffer.len() - keep);
            }
            return None;
        };
        self.buffer.drain(..start);
        let separator = find(&self.buffer, SEPARATOR)?;
        let length = match header_length(&self.buffer[HEADER.len()..separator]) {
            Some(length) if length <= LIMIT => length,
            _ => {
                let seen = String::from_utf8_lossy(&self.buffer[..separator]).into_owned();
                self.buffer.drain(..separator + SEPARATOR.len());
                return Some(Err(format!("a bad Content-Length: {seen}")));
            }
        };
        let body_start = separator + SEPARATOR.len();
        if self.buffer.len() < body_start + length {
            return None;
        }
        let parsed = serde_json::from_slice(&self.buffer[body_start..body_start + length])
            .map_err(|e| format!("not JSON: {e}"));
        self.buffer.drain(..body_start + length);
        Some(parsed)
    }
}

/// The number on the `Content-Length` line, which ends at the first line break.
fn header_length(after_name: &[u8]) -> Option<usize> {
    let line = after_name.split(|b| *b == b'\r' || *b == b'\n').next()?;
    std::str::from_utf8(line).ok()?.trim().parse().ok()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn find_ignoring_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle))
}

/// A message as a `Content-Length` frame.
///
/// @param message - the JSON to send
pub(crate) fn encode_frame(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

/// A message as one line, which is how tsserver is spoken to.
///
/// @param message - the JSON to send
pub(crate) fn encode_line(message: &Value) -> Vec<u8> {
    let mut out = message.to_string().into_bytes();
    out.push(b'\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stream() -> Vec<u8> {
        let mut bytes = b"a warning before the first frame\n".to_vec();
        bytes.extend(encode_frame(&json!({"id": 1, "text": "caf\u{e9} \u{1F600}"})));
        bytes.extend(encode_frame(&json!({"method": "second"})));
        let body = b"{\"seq\":3}\n";
        bytes.extend(format!("content-length: {}\r\n\r\n", body.len()).into_bytes());
        bytes.extend_from_slice(body);
        bytes
    }

    fn expected() -> Vec<Value> {
        vec![
            json!({"id": 1, "text": "caf\u{e9} \u{1F600}"}),
            json!({"method": "second"}),
            json!({"seq": 3}),
        ]
    }

    #[test]
    fn a_stream_in_one_read_gives_every_message() {
        let got: Vec<Value> =
            Decoder::default().push(&stream()).into_iter().map(Result::unwrap).collect();
        assert_eq!(got, expected());
    }

    #[test]
    fn the_same_stream_split_at_every_byte_gives_the_same_messages() {
        let bytes = stream();
        for cut in 0..=bytes.len() {
            let mut decoder = Decoder::default();
            let mut got = decoder.push(&bytes[..cut]);
            got.extend(decoder.push(&bytes[cut..]));
            let got: Vec<Value> = got.into_iter().map(Result::unwrap).collect();
            assert_eq!(got, expected(), "split at {cut}");
        }
    }

    #[test]
    fn one_byte_at_a_time_gives_the_same_messages() {
        let mut decoder = Decoder::default();
        let mut got = Vec::new();
        for byte in stream() {
            got.extend(decoder.push(&[byte]).into_iter().map(Result::unwrap));
        }
        assert_eq!(got, expected());
    }

    #[test]
    fn a_body_that_is_not_json_is_an_error_and_reading_goes_on() {
        let mut bytes = b"Content-Length: 3\r\n\r\nabc".to_vec();
        bytes.extend(encode_frame(&json!(1)));
        let got = Decoder::default().push(&bytes);
        assert!(got[0].is_err());
        assert_eq!(got[1], Ok(json!(1)));
    }

    #[test]
    fn a_line_is_json_and_a_break() {
        assert_eq!(encode_line(&json!({"a": 1})), b"{\"a\":1}\n");
    }
}
