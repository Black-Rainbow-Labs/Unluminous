//! Files only Microsoft Word reads or writes (task-2359, TDD 5.6): a `.doc`, `.rtf`, `.odt`, `.wps` or
//! `.pdf` is opened by having Word save it as a `.docx`, and Save As `.rtf` or `.odt` has Word save the
//! `.docx` Unluminous wrote in that format. `tools/word-oracle/convert.ps1` is the script, compiled in, so
//! an installed Unluminous needs nothing beside it but Word.
//!
//! Without Word the answer is a sentence saying so, which is what the tab shows.

use std::path::Path;

/// The script that drives Word, written out to a temporary folder each time it is run.
const SCRIPT: &str = include_str!("../../../tools/word-oracle/convert.ps1");

/// How long Word is given before it is stopped: a document Word cannot open without a question would
/// otherwise wait for ever with no window to answer it in.
const LONGEST: std::time::Duration = std::time::Duration::from_secs(180);

/// Whether a file with this extension is opened by having Word convert it.
pub fn opened_through_word(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "doc" | "rtf" | "odt" | "wps" | "pdf")
}

/// Whether a document is saved in this format by having Word convert it.
pub fn saved_through_word(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "rtf" | "odt")
}

/// Have Word open `source` and save it as `target` in `format` (`docx`, `rtf` or `odt`).
/// `UNLUMINOUS_WORD_CONVERT=off` answers as a machine without Word does, for a test.
pub fn convert(source: &Path, target: &Path, format: &str) -> Result<(), String> {
    let off = std::env::var("UNLUMINOUS_WORD_CONVERT").as_deref() == Ok("off") || !cfg!(windows);
    convert_unless(off, source, target, format)
}

fn convert_unless(off: bool, source: &Path, target: &Path, format: &str) -> Result<(), String> {
    let name = source
        .file_name()
        .map_or_else(|| source.display().to_string(), |n| n.to_string_lossy().into_owned());
    if off {
        return Err(format!("{name} needs Microsoft Word to be converted, and Word is not installed on this machine"));
    }
    let source = std::fs::canonicalize(source)
        .map_err(|e| format!("{} could not be read: {e}", source.display()))?;
    let source = plain(&source);
    let target = match target.is_absolute() {
        true => target.to_path_buf(),
        false => std::env::current_dir().map_err(|e| e.to_string())?.join(target),
    };
    let folder = std::env::temp_dir().join(format!(
        "unluminous-convert-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("{} could not be made: {e}", folder.display()))?;
    let script = folder.join("convert.ps1");
    std::fs::write(&script, SCRIPT)
        .map_err(|e| format!("{} could not be written: {e}", script.display()))?;
    let answer = run(&script, &source, &target, format);
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_dir(&folder);
    match answer {
        Ok(said) if said.trim() == "converted" && target.is_file() => Ok(()),
        Ok(said) => Err(match said.trim() {
            "" => format!("Word did not convert {name}"),
            words if words.contains("not installed") => {
                format!("{name} needs Microsoft Word to be converted, and Word is not installed on this machine")
            }
            words => words.to_owned(),
        }),
        Err(problem) => Err(problem),
    }
}

/// A path without the `\\?\` prefix `canonicalize` gives on Windows, which Word does not read.
fn plain(path: &Path) -> std::path::PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => rest.into(),
        _ => path.to_path_buf(),
    }
}

fn run(script: &Path, source: &Path, target: &Path, format: &str) -> Result<String, String> {
    let mut command = std::process::Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(script)
        .arg("-Source")
        .arg(source)
        .arg("-Target")
        .arg(target)
        .arg("-Format")
        .arg(format)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashes up while Word works.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|e| format!("Word could not be started: {e}"))?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > LONGEST => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Word took too long to convert it and was stopped".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => return Err(format!("Word could not be waited for: {e}")),
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let said = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(said.lines().last().unwrap_or_default().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_without_word_is_told_what_is_needed() {
        let refused =
            convert_unless(true, Path::new("old.doc"), Path::new("out.docx"), "docx").unwrap_err();
        assert!(refused.contains("needs Microsoft Word"), "{refused}");
        assert!(
            opened_through_word("PDF")
                && opened_through_word("doc")
                && !opened_through_word("docx")
        );
        assert!(saved_through_word("odt") && !saved_through_word("txt"));
    }
}
