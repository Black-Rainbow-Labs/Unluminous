//! The obfuscation Excel puts on a workbook it protects without a password to open.
//!
//! A workbook with its structure protected, or marked read only, is saved with the stream encrypted by
//! RC4 under a password everyone knows (`VelvetSweatshop`), so Excel opens it without asking. A workbook
//! with a password to open uses the same record and a password only its owner knows. The record is a
//! `FILEPASS`: the salt and an encrypted check value, from which the key follows.
//!
//! This module opens the first kind. The key is made from the default password, the check value must
//! decrypt to the hash of the verifier, and when it does not the password is someone's own and the
//! workbook is refused as protected with a password.

/// The password Excel uses when a person gave none.
const DEFAULT_PASSWORD: &str = "VelvetSweatshop";

/// The size of a block the cipher is restarted for.
const BLOCK: usize = 1024;

/// MD5 of a message.
pub fn md5(message: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> =
        (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32).collect();
    let mut state: [u32; 4] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476];
    let mut data = message.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend(((message.len() as u64) * 8).to_le_bytes());
    for chunk in data.chunks_exact(64) {
        let m: Vec<u32> =
            chunk.chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let rotated = a.wrapping_add(f).wrapping_add(k[i]).wrapping_add(m[g]).rotate_left(S[i]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(rotated);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

/// The RC4 stream cipher.
struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    fn new(key: &[u8]) -> Rc4 {
        let mut s = [0u8; 256];
        for (i, v) in s.iter_mut().enumerate() {
            *v = i as u8;
        }
        let mut j = 0u8;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, j as usize);
        }
        Rc4 { s, i: 0, j: 0 }
    }

    fn next(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.s[self.i as usize]);
        self.s.swap(self.i as usize, self.j as usize);
        self.s[self.s[self.i as usize].wrapping_add(self.s[self.j as usize]) as usize]
    }
}

/// The key for one block, from the salt and the intermediate hash.
fn block_key(intermediate: &[u8; 5], block: u32, length: usize) -> Vec<u8> {
    let mut message = intermediate.to_vec();
    message.extend(block.to_le_bytes());
    md5(&message)[..length].to_vec()
}

/// The first hash: the password, then sixteen copies of its start and the salt.
fn intermediate_hash(password: &str, salt: &[u8]) -> [u8; 5] {
    let utf16: Vec<u8> = password.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let h0 = md5(&utf16);
    let mut buffer = Vec::new();
    for _ in 0..16 {
        buffer.extend_from_slice(&h0[..5]);
        buffer.extend_from_slice(salt);
    }
    let h1 = md5(&buffer);
    let mut out = [0u8; 5];
    out.copy_from_slice(&h1[..5]);
    out
}

/// Whether the key made from the default password decrypts the check value to its own hash.
fn verifies(intermediate: &[u8; 5], length: usize, verifier: &[u8], verifier_hash: &[u8]) -> bool {
    let mut cipher = Rc4::new(&block_key(intermediate, 0, length));
    let plain: Vec<u8> = verifier.iter().chain(verifier_hash).map(|b| b ^ cipher.next()).collect();
    md5(&plain[..16])[..] == plain[16..32]
}

/// Records whose bodies are written plain even in an encrypted stream.
fn is_plain(id: u16) -> bool {
    matches!(id, 0x0809 | 0x002F | 0x0194 | 0x0195 | 0x00E1 | 0x0196 | 0x0138)
}

/// Decrypt a workbook stream whose `FILEPASS` record is at `filepass` (the offset of its header) and
/// uses the default password. Returns `Err` when the key does not verify, which is a password of
/// someone's own, or when the record is of a kind this version does not read.
pub fn decrypt_with_default_password(stream: &mut [u8], filepass: usize) -> Result<(), String> {
    let refuse = || crate::cfb::ENCRYPTED.to_owned();
    let body = filepass + 4;
    let data = stream.get(body..body + 54).ok_or_else(refuse)?.to_vec();
    let kind = u16::from_le_bytes([data[0], data[1]]);
    let major = u16::from_le_bytes([data[2], data[3]]);
    let minor = u16::from_le_bytes([data[4], data[5]]);
    if kind != 1 || major != 1 || minor != 1 {
        return Err(refuse());
    }
    let (salt, verifier, verifier_hash) = (&data[6..22], &data[22..38], &data[38..54]);
    let intermediate = intermediate_hash(DEFAULT_PASSWORD, salt);
    let length = [16usize, 5]
        .into_iter()
        .find(|&l| verifies(&intermediate, l, verifier, verifier_hash))
        .ok_or_else(refuse)?;
    let len = u16::from_le_bytes([stream[filepass + 2], stream[filepass + 3]]) as usize;
    let mut at = body + len;
    while at + 4 <= stream.len() {
        let id = u16::from_le_bytes([stream[at], stream[at + 1]]);
        let size = u16::from_le_bytes([stream[at + 2], stream[at + 3]]) as usize;
        let end = (at + 4 + size).min(stream.len());
        if !is_plain(id) {
            let skip = if id == 0x0085 { 4 } else { 0 };
            decrypt_range(stream, at + 4 + skip, end, &intermediate, length);
        }
        at = end;
    }
    Ok(())
}

/// Decrypt the bytes of one record body. The cipher is restarted at each block of the stream and run
/// forward to the byte, so a record that begins part way through a block lines up with its neighbours.
fn decrypt_range(stream: &mut [u8], from: usize, to: usize, intermediate: &[u8; 5], length: usize) {
    let mut position = from;
    while position < to {
        let block = position / BLOCK;
        let block_end = ((block + 1) * BLOCK).min(to);
        let mut cipher = Rc4::new(&block_key(intermediate, block as u32, length));
        for _ in 0..position % BLOCK {
            cipher.next();
        }
        for byte in &mut stream[position..block_end] {
            *byte ^= cipher.next();
        }
        position = block_end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn md5_matches_the_published_test_vectors() {
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(&md5(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            )),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn rc4_matches_the_published_test_vector() {
        let mut cipher = Rc4::new(b"Key");
        let out: Vec<u8> = b"Plaintext".iter().map(|b| b ^ cipher.next()).collect();
        assert_eq!(hex(&out), "bbf316e8d940af0ad3");
    }
}
