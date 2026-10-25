//! Sheet protection, workbook structure protection and the passwords that unlock them.
//!
//! A protected sheet is one element of the worksheet (`sheetProtection`), Allow Edit Ranges is another
//! (`protectedRanges`), and a workbook whose structure is locked has `workbookProtection`. The model keeps
//! each as the XML the file had, so a file written by another producer comes back unchanged, and this module
//! reads and writes that XML as typed values: [`SheetProtection`], [`ProtectedRange`] and
//! [`WorkbookProtection`].
//!
//! A password is never stored. Excel stores a salted SHA-512 hash spun 100,000 times ([`Secret::new`]), and
//! a file written before Excel 2013 stores a 16 bit hash in the `password` attribute, which is read, and
//! written back only for an element that had one. [`Secret::matches`] checks a password against either.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use roxmltree::Document;
use sha2::{Digest, Sha512};

use crate::coord::Range;
use crate::model::{Sheet, Workbook};
use crate::xlsx::{attrs_text, escape};

/// How many times Excel spins the hash.
pub const SPIN_COUNT: u32 = 100_000;

/// What Excel says when a password is wrong.
pub const WRONG_PASSWORD: &str = "The password you supplied is not correct. Verify that the CAPS LOCK key is off and be sure to use the correct capitalization.";

/// What Excel says when a locked cell on a protected sheet is changed.
pub const LOCKED_CELL: &str = "The cell or chart you're trying to change is on a protected sheet. To make a change, unprotect the sheet. You might be requested to enter a password.";

/// What Excel says when a command is used that a protected sheet does not allow.
pub const COMMAND_REFUSED: &str = "You cannot use this command on a protected sheet. To use this command, you must first unprotect the sheet (Review tab, Protect group, Unprotect Sheet button). You may be prompted for a password.";

/// What Excel says when the structure of a workbook is locked and a sheet is copied. Adding, deleting,
/// renaming, moving, hiding and colouring a sheet are refused in the same circumstances.
pub const STRUCTURE_LOCKED: &str = "Workbook is protected and cannot be changed.";

/// A password as a file holds it: never the password itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Secret {
    /// A salted hash, `algorithmName="SHA-512"` and the rest.
    Hashed { algorithm: String, hash: String, salt: String, spin: u32 },
    /// The 16 bit hash of Excel 2010 and earlier, four hexadecimal digits.
    Legacy(String),
}

impl Secret {
    /// The secret for a password with a salt, hashed the way Excel hashes it.
    pub fn new(password: &str, salt: &[u8]) -> Secret {
        let hash = spin_hash(password, salt, SPIN_COUNT);
        Secret::Hashed {
            algorithm: "SHA-512".to_owned(),
            hash: BASE64.encode(hash),
            salt: BASE64.encode(salt),
            spin: SPIN_COUNT,
        }
    }

    /// A secret with a fresh random salt of sixteen bytes.
    pub fn with_random_salt(password: &str) -> Secret {
        let mut salt = [0u8; 16];
        if getrandom::fill(&mut salt).is_err() {
            // The system has no random source: the time and a counter still make the salt differ.
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            salt.copy_from_slice(&Sha512::digest(nanos.to_le_bytes())[..16]);
        }
        Secret::new(password, &salt)
    }

    /// Whether `password` is the one this secret was made from.
    pub fn matches(&self, password: &str) -> bool {
        match self {
            Secret::Legacy(hex) => legacy_hash(password).eq_ignore_ascii_case(hex),
            Secret::Hashed { algorithm, hash, salt, spin } => {
                let Ok(salt) = BASE64.decode(salt) else { return false };
                let digest = match algorithm.as_str() {
                    "SHA-512" => spin_hash(password, &salt, *spin),
                    _ => return false,
                };
                BASE64.encode(digest) == *hash
            }
        }
    }
}

/// Excel's salted hash: the salt and the password as UTF-16, hashed, then the hash and a counter hashed
/// again `spin` times.
pub fn spin_hash(password: &str, salt: &[u8], spin: u32) -> Vec<u8> {
    let mut input = salt.to_vec();
    for unit in password.encode_utf16() {
        input.extend_from_slice(&unit.to_le_bytes());
    }
    let mut hash = Sha512::digest(&input).to_vec();
    for i in 0..spin {
        let mut next = hash.clone();
        next.extend_from_slice(&i.to_le_bytes());
        hash = Sha512::digest(&next).to_vec();
    }
    hash
}

/// The 16 bit password hash of Excel 2010 and earlier, as four hexadecimal digits.
pub fn legacy_hash(password: &str) -> String {
    let units: Vec<u32> = password.chars().map(|c| c as u32 & 0xFF).collect();
    let rotate = |v: u32| ((v >> 14) & 1) | ((v << 1) & 0x7FFF);
    let mut hash = 0u32;
    for unit in units.iter().rev() {
        hash = rotate(hash) ^ unit;
    }
    hash = rotate(hash) ^ units.len() as u32 ^ 0xCE4B;
    format!("{hash:04X}")
}

/// The actions a protected sheet can allow. A flag is true when the action is allowed, which is the
/// opposite of the file's own attributes (`formatCells="0"` allows formatting cells).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allow {
    pub format_cells: bool,
    pub format_columns: bool,
    pub format_rows: bool,
    pub insert_columns: bool,
    pub insert_rows: bool,
    pub insert_hyperlinks: bool,
    pub delete_columns: bool,
    pub delete_rows: bool,
    pub select_locked_cells: bool,
    pub sort: bool,
    pub auto_filter: bool,
    pub pivot_tables: bool,
    pub select_unlocked_cells: bool,
}

impl Default for Allow {
    /// What Excel's Protect Sheet offers ticked: selecting locked and unlocked cells.
    fn default() -> Self {
        Self {
            format_cells: false,
            format_columns: false,
            format_rows: false,
            insert_columns: false,
            insert_rows: false,
            insert_hyperlinks: false,
            delete_columns: false,
            delete_rows: false,
            select_locked_cells: true,
            sort: false,
            auto_filter: false,
            pivot_tables: false,
            select_unlocked_cells: true,
        }
    }
}

/// The attribute names of the allow flags in the order Excel writes them, with whether the schema's
/// default for the attribute is "protected".
const ALLOW_ATTRS: [(&str, bool); 13] = [
    ("formatCells", true),
    ("formatColumns", true),
    ("formatRows", true),
    ("insertColumns", true),
    ("insertRows", true),
    ("insertHyperlinks", true),
    ("deleteColumns", true),
    ("deleteRows", true),
    ("selectLockedCells", false),
    ("sort", true),
    ("autoFilter", true),
    ("pivotTables", true),
    ("selectUnlockedCells", false),
];

impl Allow {
    fn get(&self, index: usize) -> bool {
        [
            self.format_cells,
            self.format_columns,
            self.format_rows,
            self.insert_columns,
            self.insert_rows,
            self.insert_hyperlinks,
            self.delete_columns,
            self.delete_rows,
            self.select_locked_cells,
            self.sort,
            self.auto_filter,
            self.pivot_tables,
            self.select_unlocked_cells,
        ][index]
    }

    fn set(&mut self, index: usize, allowed: bool) {
        let slot = match index {
            0 => &mut self.format_cells,
            1 => &mut self.format_columns,
            2 => &mut self.format_rows,
            3 => &mut self.insert_columns,
            4 => &mut self.insert_rows,
            5 => &mut self.insert_hyperlinks,
            6 => &mut self.delete_columns,
            7 => &mut self.delete_rows,
            8 => &mut self.select_locked_cells,
            9 => &mut self.sort,
            10 => &mut self.auto_filter,
            11 => &mut self.pivot_tables,
            _ => &mut self.select_unlocked_cells,
        };
        *slot = allowed;
    }

    /// The names of the flags, as the Protect Sheet dialog lists them, for the commands that set them.
    pub fn names() -> [&'static str; 13] {
        ALLOW_ATTRS.map(|(name, _)| name)
    }

    /// Set one flag by its attribute name (`formatCells`, `sort` and so on).
    pub fn set_by_name(&mut self, name: &str, allowed: bool) -> bool {
        match ALLOW_ATTRS.iter().position(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(index) => {
                self.set(index, allowed);
                true
            }
            None => false,
        }
    }
}

/// `sheetProtection`: whether the sheet is protected, what else is protected with it, what is allowed, and
/// the secret that unprotects it.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetProtection {
    pub secret: Option<Secret>,
    /// The sheet's cells are protected (`sheet="1"`).
    pub sheet: bool,
    /// Drawing objects are protected.
    pub objects: bool,
    /// Scenarios are protected.
    pub scenarios: bool,
    pub allow: Allow,
    /// Attributes this version does not model, kept.
    pub other: Vec<(String, String)>,
}

impl Default for SheetProtection {
    /// Excel's Protect Sheet with every box in its default state: contents, objects and scenarios.
    fn default() -> Self {
        Self {
            secret: None,
            sheet: true,
            objects: true,
            scenarios: true,
            allow: Allow::default(),
            other: Vec::new(),
        }
    }
}

fn flag(value: Option<&str>, default: bool) -> bool {
    match value {
        Some("1") | Some("true") => true,
        Some("0") | Some("false") => false,
        _ => default,
    }
}

/// The secret in an element whose attributes are named with `prefix` (`workbook` for the workbook
/// element, empty for the others).
fn read_secret(node: roxmltree::Node, prefix: &str, legacy: &str) -> Option<Secret> {
    let name = |n: &str| {
        if prefix.is_empty() {
            n.to_owned()
        } else {
            format!("{prefix}{}{}", n[..1].to_ascii_uppercase(), &n[1..])
        }
    };
    if let Some(hash) = node.attribute(name("hashValue").as_str()) {
        return Some(Secret::Hashed {
            algorithm: node
                .attribute(name("algorithmName").as_str())
                .unwrap_or("SHA-512")
                .to_owned(),
            hash: hash.to_owned(),
            salt: node.attribute(name("saltValue").as_str()).unwrap_or("").to_owned(),
            spin: node
                .attribute(name("spinCount").as_str())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        });
    }
    node.attribute(legacy).filter(|v| !v.is_empty()).map(|v| Secret::Legacy(v.to_owned()))
}

/// The attributes that write a secret, named with `prefix`.
fn secret_attrs(secret: &Option<Secret>, prefix: &str, legacy: &str) -> Vec<(String, String)> {
    let name = |n: &str| {
        if prefix.is_empty() {
            n.to_owned()
        } else {
            format!("{prefix}{}{}", n[..1].to_ascii_uppercase(), &n[1..])
        }
    };
    match secret {
        None => Vec::new(),
        Some(Secret::Legacy(hex)) => vec![(legacy.to_owned(), hex.clone())],
        Some(Secret::Hashed { algorithm, hash, salt, spin }) => vec![
            (name("algorithmName"), algorithm.clone()),
            (name("hashValue"), hash.clone()),
            (name("saltValue"), salt.clone()),
            (name("spinCount"), spin.to_string()),
        ],
    }
}

const SECRET_NAMES: [&str; 5] =
    ["algorithmName", "hashValue", "saltValue", "spinCount", "password"];

impl SheetProtection {
    /// Read a `sheetProtection` element.
    pub fn parse(xml: &str) -> Option<Self> {
        let doc = Document::parse(xml).ok()?;
        let node = doc.root_element();
        let mut allow = Allow::default();
        for (i, (name, default)) in ALLOW_ATTRS.iter().enumerate() {
            allow.set(i, !flag(node.attribute(*name), *default));
        }
        let skip: Vec<&str> = ALLOW_ATTRS
            .iter()
            .map(|(n, _)| *n)
            .chain(SECRET_NAMES)
            .chain(["sheet", "objects", "scenarios"])
            .collect();
        Some(Self {
            secret: read_secret(node, "", "password"),
            sheet: flag(node.attribute("sheet"), false),
            objects: flag(node.attribute("objects"), false),
            scenarios: flag(node.attribute("scenarios"), false),
            allow,
            other: node
                .attributes()
                .filter(|a| !skip.contains(&a.name()))
                .map(|a| (a.name().to_owned(), a.value().to_owned()))
                .collect(),
        })
    }

    /// The element as Excel writes it: a flag only when it differs from the schema's default.
    pub fn to_xml(&self) -> String {
        let mut attrs = secret_attrs(&self.secret, "", "password");
        for (name, on) in
            [("sheet", self.sheet), ("objects", self.objects), ("scenarios", self.scenarios)]
        {
            if on {
                attrs.push((name.to_owned(), "1".to_owned()));
            }
        }
        for (i, (name, default)) in ALLOW_ATTRS.iter().enumerate() {
            let protected = !self.allow.get(i);
            if protected != *default {
                attrs.push(((*name).to_owned(), u8::from(protected).to_string()));
            }
        }
        attrs.extend(self.other.iter().cloned());
        format!("<sheetProtection{}/>", attrs_text(&attrs))
    }
}

/// One entry of Allow Edit Ranges: cells a person may edit on a protected sheet, with a password of their
/// own when the range has one.
#[derive(Debug, Clone, PartialEq)]
pub struct ProtectedRange {
    pub name: String,
    pub ranges: Vec<Range>,
    pub secret: Option<Secret>,
    pub security_descriptor: Option<String>,
    pub other: Vec<(String, String)>,
}

impl ProtectedRange {
    /// Read a `protectedRanges` element.
    pub fn parse_all(xml: &str) -> Vec<Self> {
        let Ok(doc) = Document::parse(xml) else { return Vec::new() };
        doc.root_element()
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "protectedRange")
            .map(|n| ProtectedRange {
                name: n.attribute("name").unwrap_or("").to_owned(),
                ranges: n
                    .attribute("sqref")
                    .unwrap_or("")
                    .split_whitespace()
                    .filter_map(Range::parse)
                    .collect(),
                secret: read_secret(n, "", "password"),
                security_descriptor: n.attribute("securityDescriptor").map(str::to_owned),
                other: n
                    .attributes()
                    .filter(|a| {
                        !["name", "sqref", "securityDescriptor"].contains(&a.name())
                            && !SECRET_NAMES.contains(&a.name())
                    })
                    .map(|a| (a.name().to_owned(), a.value().to_owned()))
                    .collect(),
            })
            .collect()
    }

    /// The `protectedRanges` element for a list of ranges, empty when there are none.
    pub fn to_xml_all(list: &[ProtectedRange]) -> String {
        if list.is_empty() {
            return String::new();
        }
        let mut out = String::from("<protectedRanges>");
        for p in list {
            let mut attrs = secret_attrs(&p.secret, "", "password");
            let sqref: Vec<String> = p.ranges.iter().map(Range::a1).collect();
            attrs.push(("sqref".to_owned(), sqref.join(" ")));
            attrs.push(("name".to_owned(), p.name.clone()));
            if let Some(sd) = &p.security_descriptor {
                attrs.push(("securityDescriptor".to_owned(), sd.clone()));
            }
            attrs.extend(p.other.iter().cloned());
            out.push_str(&format!("<protectedRange{}/>", attrs_text(&attrs)));
        }
        out.push_str("</protectedRanges>");
        out
    }
}

/// `workbookProtection`: the structure and windows locks, with the workbook's secret.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkbookProtection {
    pub secret: Option<Secret>,
    /// Sheets cannot be added, deleted, renamed, moved, hidden or shown.
    pub lock_structure: bool,
    /// Windows cannot be moved or resized.
    pub lock_windows: bool,
    /// Revision tracking is locked.
    pub lock_revision: bool,
    pub other: Vec<(String, String)>,
}

impl Default for WorkbookProtection {
    fn default() -> Self {
        Self {
            secret: None,
            lock_structure: true,
            lock_windows: false,
            lock_revision: false,
            other: Vec::new(),
        }
    }
}

impl WorkbookProtection {
    /// Read a `workbookProtection` element.
    pub fn parse(xml: &str) -> Option<Self> {
        let doc = Document::parse(xml).ok()?;
        let node = doc.root_element();
        let skip = [
            "workbookAlgorithmName",
            "workbookHashValue",
            "workbookSaltValue",
            "workbookSpinCount",
            "workbookPassword",
            "lockStructure",
            "lockWindows",
            "lockRevision",
        ];
        Some(Self {
            secret: read_secret(node, "workbook", "workbookPassword"),
            lock_structure: flag(node.attribute("lockStructure"), false),
            lock_windows: flag(node.attribute("lockWindows"), false),
            lock_revision: flag(node.attribute("lockRevision"), false),
            other: node
                .attributes()
                .filter(|a| !skip.contains(&a.name()))
                .map(|a| (a.name().to_owned(), a.value().to_owned()))
                .collect(),
        })
    }

    pub fn to_xml(&self) -> String {
        let mut attrs = secret_attrs(&self.secret, "workbook", "workbookPassword");
        for (name, on) in [
            ("lockStructure", self.lock_structure),
            ("lockWindows", self.lock_windows),
            ("lockRevision", self.lock_revision),
        ] {
            if on {
                attrs.push((name.to_owned(), "1".to_owned()));
            }
        }
        attrs.extend(self.other.iter().cloned());
        format!("<workbookProtection{}/>", attrs_text(&attrs))
    }
}

impl Sheet {
    /// The sheet's protection, when the file has `sheetProtection`.
    pub fn protection(&self) -> Option<SheetProtection> {
        let xml = self.raw.elements.iter().find(|(n, _)| n == "sheetProtection")?;
        SheetProtection::parse(&xml.1)
    }

    /// Whether the sheet's cells are protected (Review, Protect Sheet is on).
    pub fn is_protected(&self) -> bool {
        self.protection().is_some_and(|p| p.sheet)
    }

    /// Replace the sheet's protection, or remove it.
    pub fn set_protection(&mut self, protection: Option<&SheetProtection>) {
        self.raw.elements.retain(|(n, _)| n != "sheetProtection");
        if let Some(p) = protection {
            self.raw.elements.push(("sheetProtection".to_owned(), p.to_xml()));
        }
    }

    /// Allow Edit Ranges.
    pub fn protected_ranges(&self) -> Vec<ProtectedRange> {
        self.raw
            .elements
            .iter()
            .find(|(n, _)| n == "protectedRanges")
            .map(|(_, xml)| ProtectedRange::parse_all(xml))
            .unwrap_or_default()
    }

    pub fn set_protected_ranges(&mut self, list: &[ProtectedRange]) {
        self.raw.elements.retain(|(n, _)| n != "protectedRanges");
        let xml = ProtectedRange::to_xml_all(list);
        if !xml.is_empty() {
            self.raw.elements.push(("protectedRanges".to_owned(), xml));
        }
    }
}

impl Workbook {
    /// The workbook's protection, when the file has `workbookProtection`.
    pub fn protection(&self) -> Option<WorkbookProtection> {
        let xml = self.raw.iter().find(|(n, _)| n == "workbookProtection")?;
        WorkbookProtection::parse(&xml.1)
    }

    /// Whether the structure is locked.
    pub fn structure_locked(&self) -> bool {
        self.protection().is_some_and(|p| p.lock_structure)
    }

    pub fn set_protection(&mut self, protection: Option<&WorkbookProtection>) {
        self.raw.retain(|(n, _)| n != "workbookProtection");
        if let Some(p) = protection {
            self.raw.push(("workbookProtection".to_owned(), p.to_xml()));
        }
    }
}

/// The text of an attribute value as it is written, for messages that name a range.
pub fn quoted(name: &str) -> String {
    format!("\"{}\"", escape(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash, salt and spin count Excel 16 wrote for the password `secret`.
    #[test]
    fn the_hash_is_the_one_excel_wrote() {
        let secret = Secret::Hashed {
            algorithm: "SHA-512".to_owned(),
            hash: "lN9bVIfWZ3393zWASTkaowY9S/FItBb+jw0p8pyw4cknPlFeqcF0v9XzSAd0TDLKsb/wwASFQQVi752H+Vsl1A==".to_owned(),
            salt: "rJvSUzklw1j9igdafza6rg==".to_owned(),
            spin: 100_000,
        };
        assert!(secret.matches("secret"));
        assert!(!secret.matches("Secret"));
        assert!(!secret.matches(""));
    }

    #[test]
    fn a_new_secret_matches_only_its_password() {
        let secret = Secret::with_random_salt("pässword");
        assert!(secret.matches("pässword"));
        assert!(!secret.matches("password"));
        let again = Secret::with_random_salt("pässword");
        assert_ne!(secret, again, "two salts differ");
    }

    #[test]
    fn the_legacy_hash_matches_published_values() {
        // The empty password hashes to the constant the algorithm ends with.
        assert_eq!(legacy_hash(""), "CE4B");
        // Hashes of 'test' and 'a' that Excel accepted as the passwords (tests/protection.rs opens them in Excel).
        assert!(Secret::Legacy(legacy_hash("test")).matches("test"));
        assert!(!Secret::Legacy(legacy_hash("test")).matches("Test"));
    }

    #[test]
    fn sheet_protection_reads_and_writes_what_excel_writes() {
        let xml = r#"<sheetProtection sheet="1" objects="1" scenarios="1" formatCells="0" formatColumns="0" formatRows="0" insertColumns="0" insertRows="0" insertHyperlinks="0" deleteColumns="0" deleteRows="0" sort="0" autoFilter="0" pivotTables="0"/>"#;
        let p = SheetProtection::parse(xml).unwrap();
        assert!(p.sheet && p.objects && p.scenarios);
        assert!(p.allow.format_cells && p.allow.sort && p.allow.pivot_tables);
        assert!(p.allow.select_locked_cells && p.allow.select_unlocked_cells);
        assert_eq!(p.to_xml(), xml);
        let strict =
            SheetProtection::parse(r#"<sheetProtection sheet="1" selectLockedCells="1"/>"#)
                .unwrap();
        assert!(!strict.allow.select_locked_cells && !strict.allow.format_cells);
    }

    #[test]
    fn workbook_protection_names_its_attributes_with_the_workbook_prefix() {
        let xml = r#"<workbookProtection workbookAlgorithmName="SHA-512" workbookHashValue="aGFzaA==" workbookSaltValue="c2FsdA==" workbookSpinCount="100000" lockStructure="1"/>"#;
        let p = WorkbookProtection::parse(xml).unwrap();
        assert!(p.lock_structure && !p.lock_windows);
        assert_eq!(p.to_xml(), xml);
    }

    #[test]
    fn allow_edit_ranges_keep_their_own_secrets() {
        let xml = r#"<protectedRanges><protectedRange sqref="A1:A2" name="Open range"/><protectedRange algorithmName="SHA-512" hashValue="aA==" saltValue="cw==" spinCount="100000" sqref="B1:B2" name="Locked range"/></protectedRanges>"#;
        let list = ProtectedRange::parse_all(xml);
        assert_eq!(list.len(), 2);
        assert!(list[0].secret.is_none() && list[1].secret.is_some());
        assert_eq!(ProtectedRange::to_xml_all(&list), xml);
    }
}
