//! A Word document encrypted with a password (task-2359, TDD 5.6): ECMA-376 agile encryption
//! ([MS-OFFCRYPTO] 2.3.4.10 to 2.3.4.15), which Word 2010 and later write. The package (the `.docx` zip) is
//! encrypted with AES-256 in CBC mode in 4096 byte segments, with a random key that is itself encrypted
//! with a key made from the password by 100,000 rounds of SHA-512, and the whole is held in an OLE
//! compound file with the `EncryptionInfo` and `EncryptedPackage` streams and the `DataSpaces` storage
//! that says how to read them. An HMAC over the encrypted package lets a reader tell it was not changed.

use std::io::{Read, Write};

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::Engine as _;
use hmac::Mac;
use sha2::{Digest, Sha512};

type Encryptor = cbc::Encryptor<aes::Aes256>;
type Decryptor = cbc::Decryptor<aes::Aes256>;

const SPIN_COUNT: u32 = 100_000;
const SEGMENT: usize = 4096;
const VERIFIER_INPUT: [u8; 8] = [0xfe, 0xa7, 0xd2, 0x76, 0x3b, 0x4b, 0x9e, 0x79];
const VERIFIER_VALUE: [u8; 8] = [0xd7, 0xaa, 0x0f, 0x6d, 0x30, 0x61, 0x34, 0x4e];
const KEY_VALUE: [u8; 8] = [0x14, 0x6e, 0x0b, 0xe7, 0xab, 0xac, 0xd0, 0xd6];
const HMAC_KEY: [u8; 8] = [0x5f, 0xb2, 0xad, 0x01, 0x0c, 0xb9, 0xe1, 0xf6];
const HMAC_VALUE: [u8; 8] = [0xa0, 0x67, 0x7f, 0x02, 0xb2, 0x2c, 0x84, 0x33];

/// Whether these bytes are an OLE compound file with an encrypted package in it, which is what a
/// password protected `.docx` is, rather than a zip.
pub fn is_encrypted(bytes: &[u8]) -> bool {
    if !bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        return false;
    }
    cfb::CompoundFile::open(std::io::Cursor::new(bytes)).is_ok_and(|file| file.is_stream("/EncryptionInfo"))
}

fn random(length: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0u8; length];
    getrandom::fill(&mut bytes).map_err(|e| format!("no random numbers to encrypt with: {e}"))?;
    Ok(bytes)
}

/// The key a password makes for one purpose (`block`), 32 bytes for AES-256.
fn password_key(password: &str, salt: &[u8], spin: u32, block: &[u8]) -> Vec<u8> {
    let utf16: Vec<u8> = password.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut hash = Sha512::new().chain_update(salt).chain_update(&utf16).finalize().to_vec();
    for round in 0..spin {
        hash = Sha512::new().chain_update(round.to_le_bytes()).chain_update(&hash).finalize().to_vec();
    }
    let mut key = Sha512::new().chain_update(&hash).chain_update(block).finalize().to_vec();
    key.truncate(32);
    key
}

/// The initialisation vector for a block, from the key data's salt: the first 16 bytes of its hash.
fn block_iv(salt: &[u8], block: &[u8]) -> Vec<u8> {
    let mut iv = Sha512::new().chain_update(salt).chain_update(block).finalize().to_vec();
    iv.truncate(16);
    iv
}

/// Pad to a whole number of AES blocks with zeros.
fn padded(data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    while out.len() % 16 != 0 {
        out.push(0);
    }
    out
}

fn encrypt_block(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let mut buffer = padded(data);
    let length = buffer.len();
    Encryptor::new_from_slices(key, iv)
        .map_err(|e| e.to_string())?
        .encrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buffer, length)
        .map_err(|e| e.to_string())?;
    Ok(buffer)
}

fn decrypt_block(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let mut buffer = padded(data);
    Decryptor::new_from_slices(key, iv)
        .map_err(|e| e.to_string())?
        .decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buffer)
        .map_err(|e| e.to_string())?;
    Ok(buffer)
}

/// The package encrypted segment by segment, after its length.
fn encrypt_package(package: &[u8], key: &[u8], salt: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = (package.len() as u64).to_le_bytes().to_vec();
    for (index, segment) in package.chunks(SEGMENT).enumerate() {
        out.extend(encrypt_block(key, &block_iv(salt, &(index as u32).to_le_bytes()), segment)?);
    }
    Ok(out)
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(text: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD.decode(text).map_err(|e| e.to_string())
}

/// A string as [MS-OFFCRYPTO]'s UNICODE-LP-P4: its length in bytes, its UTF-16, padded to four bytes.
fn unicode_lp(text: &str) -> Vec<u8> {
    let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = (utf16.len() as u32).to_le_bytes().to_vec();
    out.extend(&utf16);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

fn versions() -> Vec<u8> {
    [1u16, 0, 1, 0, 1, 0].iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Encrypt a `.docx` package with a password, as Word's Encrypt with Password does.
pub fn encrypt(package: &[u8], password: &str) -> Result<Vec<u8>, String> {
    if password.is_empty() {
        return Err("a password is needed to encrypt".into());
    }
    let key_salt = random(16)?;
    let password_salt = random(16)?;
    let key_value = random(32)?;
    let verifier = random(16)?;
    let hmac_key = random(64)?;
    let key_for = |block: &[u8]| password_key(password, &password_salt, SPIN_COUNT, block);
    let encrypted_verifier_input = encrypt_block(&key_for(&VERIFIER_INPUT), &password_salt, &verifier)?;
    let verifier_hash = Sha512::digest(&verifier).to_vec();
    let encrypted_verifier_value = encrypt_block(&key_for(&VERIFIER_VALUE), &password_salt, &verifier_hash)?;
    let encrypted_key_value = encrypt_block(&key_for(&KEY_VALUE), &password_salt, &key_value)?;
    let encrypted_package = encrypt_package(package, &key_value, &key_salt)?;
    let encrypted_hmac_key = encrypt_block(&key_value, &block_iv(&key_salt, &HMAC_KEY), &hmac_key)?;
    let mut mac = hmac::Hmac::<Sha512>::new_from_slice(&hmac_key).map_err(|e| e.to_string())?;
    mac.update(&encrypted_package);
    let hmac_value = mac.finalize().into_bytes().to_vec();
    let encrypted_hmac_value = encrypt_block(&key_value, &block_iv(&key_salt, &HMAC_VALUE), &hmac_value)?;
    let cipher = r#"saltSize="16" blockSize="16" keyBits="256" hashSize="64" cipherAlgorithm="AES" cipherChaining="ChainingModeCBC" hashAlgorithm="SHA512""#;
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<encryption xmlns=\"http://schemas.microsoft.com/office/2006/encryption\" xmlns:p=\"http://schemas.microsoft.com/office/2006/keyEncryptor/password\" xmlns:c=\"http://schemas.microsoft.com/office/2006/keyEncryptor/certificate\"><keyData {cipher} saltValue=\"{}\"/><dataIntegrity encryptedHmacKey=\"{}\" encryptedHmacValue=\"{}\"/><keyEncryptors><keyEncryptor uri=\"http://schemas.microsoft.com/office/2006/keyEncryptor/password\"><p:encryptedKey spinCount=\"{SPIN_COUNT}\" {cipher} saltValue=\"{}\" encryptedVerifierHashInput=\"{}\" encryptedVerifierHashValue=\"{}\" encryptedKeyValue=\"{}\"/></keyEncryptor></keyEncryptors></encryption>",
        b64(&key_salt),
        b64(&encrypted_hmac_key),
        b64(&encrypted_hmac_value),
        b64(&password_salt),
        b64(&encrypted_verifier_input),
        b64(&encrypted_verifier_value),
        b64(&encrypted_key_value),
    );
    let mut info = vec![0x04, 0x00, 0x04, 0x00, 0x40, 0x00, 0x00, 0x00];
    info.extend(xml.as_bytes());
    // The DataSpaces storage: which transform the encrypted package went through.
    let mut map = 8u32.to_le_bytes().to_vec();
    map.extend(1u32.to_le_bytes());
    let mut entry = 1u32.to_le_bytes().to_vec();
    entry.extend(0u32.to_le_bytes());
    entry.extend(unicode_lp("EncryptedPackage"));
    entry.extend(unicode_lp("StrongEncryptionDataSpace"));
    map.extend(((entry.len() + 4) as u32).to_le_bytes());
    map.extend(entry);
    let mut space = 8u32.to_le_bytes().to_vec();
    space.extend(1u32.to_le_bytes());
    space.extend(unicode_lp("StrongEncryptionTransform"));
    let id = unicode_lp("{FF9A3F03-56EF-4613-BDD5-5A41C1D07246}");
    let mut primary = ((8 + id.len()) as u32).to_le_bytes().to_vec();
    primary.extend(1u32.to_le_bytes());
    primary.extend(&id);
    primary.extend(unicode_lp("Microsoft.Container.EncryptionTransform"));
    primary.extend(versions());
    primary.extend(0u32.to_le_bytes());
    primary.extend(0u32.to_le_bytes());
    primary.extend(0u32.to_le_bytes());
    primary.extend(4u32.to_le_bytes());
    let mut version = unicode_lp("Microsoft.Container.DataSpaces");
    version.extend(versions());
    let cursor = std::io::Cursor::new(Vec::new());
    let mut file = cfb::CompoundFile::create(cursor).map_err(|e| e.to_string())?;
    let write = |file: &mut cfb::CompoundFile<std::io::Cursor<Vec<u8>>>, path: &str, data: &[u8]| -> Result<(), String> {
        let mut stream = file.create_stream(path).map_err(|e| e.to_string())?;
        stream.write_all(data).map_err(|e| e.to_string())
    };
    file.create_storage("/\u{6}DataSpaces").map_err(|e| e.to_string())?;
    file.create_storage("/\u{6}DataSpaces/DataSpaceInfo").map_err(|e| e.to_string())?;
    file.create_storage("/\u{6}DataSpaces/TransformInfo").map_err(|e| e.to_string())?;
    file.create_storage("/\u{6}DataSpaces/TransformInfo/StrongEncryptionTransform").map_err(|e| e.to_string())?;
    write(&mut file, "/\u{6}DataSpaces/Version", &version)?;
    write(&mut file, "/\u{6}DataSpaces/DataSpaceMap", &map)?;
    write(&mut file, "/\u{6}DataSpaces/DataSpaceInfo/StrongEncryptionDataSpace", &space)?;
    write(&mut file, "/\u{6}DataSpaces/TransformInfo/StrongEncryptionTransform/\u{6}Primary", &primary)?;
    write(&mut file, "/EncryptionInfo", &info)?;
    write(&mut file, "/EncryptedPackage", &encrypted_package)?;
    file.flush().map_err(|e| e.to_string())?;
    Ok(file.into_inner().into_inner())
}

/// Decrypt a password protected `.docx` into its package. A wrong password is refused, and so is a
/// package whose HMAC does not match, which means it was changed after it was encrypted.
pub fn decrypt(bytes: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let mut file = cfb::CompoundFile::open(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut read = |path: &str| -> Result<Vec<u8>, String> {
        let mut stream = file.open_stream(path).map_err(|e| format!("{path}: {e}"))?;
        let mut data = Vec::new();
        stream.read_to_end(&mut data).map_err(|e| e.to_string())?;
        Ok(data)
    };
    let info = read("/EncryptionInfo")?;
    let package = read("/EncryptedPackage")?;
    if info.len() < 8 || info[0] != 4 || info[2] != 4 {
        return Err("this document is encrypted in a way Unluminous does not read: only agile encryption".into());
    }
    let root = crate::xml::parse(&String::from_utf8_lossy(&info[8..])).map_err(|e| format!("its encryption information: {e}"))?;
    let key_data = find_local(&root, "keyData").ok_or("no key data")?;
    let encrypted_key = find_local(&root, "encryptedKey").ok_or("no password key")?;
    let integrity = find_local(&root, "dataIntegrity");
    let attr = |el: &crate::xml::El, name: &str| -> Result<Vec<u8>, String> {
        unb64(el.attr(name).ok_or_else(|| format!("no {name}"))?)
    };
    if key_data.attr("hashAlgorithm") != Some("SHA512") || key_data.attr("keyBits") != Some("256") {
        return Err("this document is encrypted with a cipher Unluminous does not read: only AES-256 with SHA-512".into());
    }
    let spin: u32 = encrypted_key.attr("spinCount").and_then(|s| s.parse().ok()).unwrap_or(SPIN_COUNT);
    let password_salt = attr(encrypted_key, "saltValue")?;
    let key_salt = attr(key_data, "saltValue")?;
    let key_for = |block: &[u8]| password_key(password, &password_salt, spin, block);
    let verifier = decrypt_block(&key_for(&VERIFIER_INPUT), &password_salt, &attr(encrypted_key, "encryptedVerifierHashInput")?)?;
    let verifier_hash = decrypt_block(&key_for(&VERIFIER_VALUE), &password_salt, &attr(encrypted_key, "encryptedVerifierHashValue")?)?;
    if Sha512::digest(&verifier[..16]).as_slice() != &verifier_hash[..64] {
        return Err("the password is not right".into());
    }
    let key_value = decrypt_block(&key_for(&KEY_VALUE), &password_salt, &attr(encrypted_key, "encryptedKeyValue")?)?;
    let key_value = &key_value[..32];
    if let Some(integrity) = integrity {
        let hmac_key = decrypt_block(key_value, &block_iv(&key_salt, &HMAC_KEY), &attr(integrity, "encryptedHmacKey")?)?;
        let expected = decrypt_block(key_value, &block_iv(&key_salt, &HMAC_VALUE), &attr(integrity, "encryptedHmacValue")?)?;
        let mut mac = hmac::Hmac::<Sha512>::new_from_slice(&hmac_key[..64]).map_err(|e| e.to_string())?;
        mac.update(&package);
        if mac.finalize().into_bytes().as_slice() != &expected[..64] {
            return Err("the encrypted document was changed after it was encrypted".into());
        }
    }
    let size = u64::from_le_bytes(package.get(..8).ok_or("the encrypted package is empty")?.try_into().expect("eight bytes")) as usize;
    let mut out = Vec::with_capacity(size);
    for (index, segment) in package[8..].chunks(SEGMENT).enumerate() {
        out.extend(decrypt_block(key_value, &block_iv(&key_salt, &(index as u32).to_le_bytes()), segment)?);
    }
    out.truncate(size);
    Ok(out)
}

fn find_local<'a>(el: &'a crate::xml::El, name: &str) -> Option<&'a crate::xml::El> {
    if el.local() == name {
        return Some(el);
    }
    el.elements().find_map(|child| find_local(child, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_encrypted_with_a_password_opens_with_it_and_not_without() {
        let document = crate::new::blank();
        let package = crate::write::save(&document).unwrap();
        let encrypted = encrypt(&package, "s3cret").unwrap();
        assert!(is_encrypted(&encrypted));
        assert!(!is_encrypted(&package));
        assert_eq!(decrypt(&encrypted, "s3cret").unwrap(), package);
        assert!(decrypt(&encrypted, "wrong").is_err());
    }
}
