//! Codex-style patches for one note:
//!
//! ```text
//! *** Begin Patch
//! *** Update File: notes/<id>.md
//! @@ optional anchor line
//!  context
//! -removed
//! +added
//! *** End of File        (optional: the hunk ends at the end of the note)
//! *** End Patch
//! ```
//!
//! Each hunk's context and removed lines must match the note in exactly one
//! place, after the `@@` anchor when one is given; matching retries with
//! trailing and then surrounding whitespace ignored. A hunk with only added
//! lines appends to the note, or inserts after its anchor.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PatchError {
    #[error("patch must start with *** Begin Patch")]
    MissingBegin,
    #[error("patch must end with *** End Patch")]
    MissingEnd,
    #[error("patch must contain exactly one *** Update File section and at least one hunk")]
    InvalidUpdateCount,
    #[error("patch targets {actual}, expected {expected}")]
    WrongTarget { actual: String, expected: String },
    #[error("patch operation is not supported by hackmd_edit_note: {0}")]
    UnsupportedOperation(String),
    #[error("patch hunk must start with @@")]
    MissingHunk,
    #[error("hunk {hunk}: anchor `{anchor}` was not found in the note")]
    AnchorNotFound { hunk: usize, anchor: String },
    #[error(
        "hunk {hunk}: context was not found in the note; re-read it with hackmd_get_note and copy \
         the context lines exactly"
    )]
    ContextNotFound { hunk: usize },
    #[error(
        "hunk {hunk}: context matches {count} places in the note; add more context lines or an \
         `@@ <line>` anchor naming a unique line above the change"
    )]
    AmbiguousContext { hunk: usize, count: usize },
    #[error("patch line {line} must start with a space, + or -: {text}")]
    InvalidLine { line: usize, text: String },
}

#[derive(Debug)]
struct FilePatch {
    target: String,
    hunks: Vec<Hunk>,
}

#[derive(Debug, Default)]
struct Hunk {
    anchor: Option<String>,
    lines: Vec<HunkLine>,
    end_of_file: bool,
}

#[derive(Debug)]
enum HunkLine {
    Context(String),
    Add(String),
    Remove(String),
}

/// Applies `patch` to `content`, the note at `expected_target`. Keeps the
/// note's line endings and whether it ends with a newline.
pub fn apply_note_patch(
    content: &str,
    patch: &str,
    expected_target: &str,
) -> Result<String, PatchError> {
    let file_patch = parse_patch(patch)?;
    if file_patch.target.trim() != expected_target {
        return Err(PatchError::WrongTarget {
            actual: file_patch.target,
            expected: expected_target.to_owned(),
        });
    }

    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let trailing_newline = content.ends_with('\n');
    let body = content.strip_suffix('\n').unwrap_or(content);
    let body = body.strip_suffix('\r').unwrap_or(body);
    let mut lines: Vec<String> = if content.is_empty() {
        Vec::new()
    } else {
        body.split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
            .collect()
    };

    for (index, hunk) in file_patch.hunks.into_iter().enumerate() {
        apply_hunk(&mut lines, hunk, index + 1)?;
    }

    let mut output = lines.join(newline);
    if trailing_newline || (content.is_empty() && !output.is_empty()) {
        output.push_str(newline);
    }
    Ok(output)
}

fn parse_patch(patch: &str) -> Result<FilePatch, PatchError> {
    let lines: Vec<&str> = patch.trim().lines().collect();
    if lines.first().map(|line| line.trim_end()) != Some("*** Begin Patch") {
        return Err(PatchError::MissingBegin);
    }
    if lines.len() < 2 || lines.last().map(|line| line.trim_end()) != Some("*** End Patch") {
        return Err(PatchError::MissingEnd);
    }

    let mut target = None;
    let mut hunks = Vec::new();
    let mut current: Option<Hunk> = None;

    for (offset, line) in lines[1..lines.len() - 1].iter().enumerate() {
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            if target.replace(path.trim().to_owned()).is_some() {
                return Err(PatchError::InvalidUpdateCount);
            }
            continue;
        }
        if line.starts_with("*** Add File: ")
            || line.starts_with("*** Delete File: ")
            || line.starts_with("*** Move to: ")
        {
            return Err(PatchError::UnsupportedOperation((*line).to_owned()));
        }
        if line.trim_end() == "*** End of File" {
            current.as_mut().ok_or(PatchError::MissingHunk)?.end_of_file = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("@@") {
            hunks.extend(current.take());
            let anchor = rest.trim();
            current = Some(Hunk {
                anchor: (!anchor.is_empty()).then(|| anchor.to_owned()),
                ..Hunk::default()
            });
            continue;
        }

        let hunk = current.as_mut().ok_or(PatchError::MissingHunk)?;
        let entry = if let Some(text) = line.strip_prefix(' ') {
            HunkLine::Context(text.to_owned())
        } else if let Some(text) = line.strip_prefix('+') {
            HunkLine::Add(text.to_owned())
        } else if let Some(text) = line.strip_prefix('-') {
            HunkLine::Remove(text.to_owned())
        } else if line.trim().is_empty() {
            // Models often drop the space that marks a blank context line.
            HunkLine::Context(String::new())
        } else {
            return Err(PatchError::InvalidLine {
                line: offset + 2,
                text: (*line).to_owned(),
            });
        };
        hunk.lines.push(entry);
    }
    hunks.extend(current);

    match target {
        Some(target) if !hunks.is_empty() => Ok(FilePatch { target, hunks }),
        _ => Err(PatchError::InvalidUpdateCount),
    }
}

fn apply_hunk(lines: &mut Vec<String>, hunk: Hunk, number: usize) -> Result<(), PatchError> {
    let old: Vec<&str> = hunk
        .lines
        .iter()
        .filter_map(|line| match line {
            HunkLine::Context(text) | HunkLine::Remove(text) => Some(text.as_str()),
            HunkLine::Add(_) => None,
        })
        .collect();
    let new: Vec<String> = hunk
        .lines
        .iter()
        .filter_map(|line| match line {
            HunkLine::Context(text) | HunkLine::Add(text) => Some(text.clone()),
            HunkLine::Remove(_) => None,
        })
        .collect();

    let start = match &hunk.anchor {
        Some(anchor) => {
            Some(
                find_anchor(lines, anchor).ok_or_else(|| PatchError::AnchorNotFound {
                    hunk: number,
                    anchor: anchor.clone(),
                })?,
            )
        }
        None => None,
    };

    if old.is_empty() {
        // Pure insertion: after the anchor, otherwise at the end.
        let at = match start {
            Some(anchor) if !hunk.end_of_file => anchor + 1,
            _ => lines.len(),
        };
        lines.splice(at..at, new);
        return Ok(());
    }

    let position = find_match(lines, &old, start, hunk.end_of_file, number)?;
    lines.splice(position..position + old.len(), new);
    Ok(())
}

/// The first line equal to the anchor, ignoring surrounding whitespace, or
/// failing that the first line containing it.
fn find_anchor(lines: &[String], anchor: &str) -> Option<usize> {
    lines
        .iter()
        .position(|line| line.trim() == anchor)
        .or_else(|| lines.iter().position(|line| line.contains(anchor)))
}

type Normalize = fn(&str) -> &str;

const LEVELS: [Normalize; 3] = [|line| line, str::trim_end, str::trim];

/// Where `needle` sits in `lines`. With an anchor, the first match at or
/// after it wins; without one, the match must be unique.
fn find_match(
    lines: &[String],
    needle: &[&str],
    anchor: Option<usize>,
    end_of_file: bool,
    hunk: usize,
) -> Result<usize, PatchError> {
    for normalize in LEVELS {
        let matches: Vec<usize> = lines
            .windows(needle.len())
            .enumerate()
            .filter(|(index, window)| {
                anchor.is_none_or(|anchor| *index >= anchor)
                    && (!end_of_file || index + needle.len() == lines.len())
                    && window
                        .iter()
                        .zip(needle)
                        .all(|(line, expected)| normalize(line) == normalize(expected))
            })
            .map(|(index, _)| index)
            .collect();
        match matches.as_slice() {
            [] => continue,
            [only] => return Ok(*only),
            [first, ..] if anchor.is_some() => return Ok(*first),
            many => {
                return Err(PatchError::AmbiguousContext {
                    hunk,
                    count: many.len(),
                });
            }
        }
    }
    Err(PatchError::ContextNotFound { hunk })
}

#[cfg(test)]
mod tests {
    use super::{PatchError, apply_note_patch};

    fn patch(body: &str) -> String {
        format!("*** Begin Patch\n*** Update File: notes/a.md\n{body}\n*** End Patch")
    }

    #[test]
    fn applies_update_hunk() {
        let output = apply_note_patch(
            "title\nold\nend\n",
            &patch("@@\n title\n-old\n+new\n end"),
            "notes/a.md",
        );
        assert_eq!(output.unwrap(), "title\nnew\nend\n");
    }

    #[test]
    fn rejects_wrong_target() {
        let patch = "*** Begin Patch\n*** Update File: notes/b.md\n@@\n-old\n+new\n*** End Patch";
        assert_eq!(
            apply_note_patch("old", patch, "notes/a.md"),
            Err(PatchError::WrongTarget {
                actual: "notes/b.md".to_owned(),
                expected: "notes/a.md".to_owned()
            })
        );
    }

    #[test]
    fn rejects_missing_context() {
        assert_eq!(
            apply_note_patch("other", &patch("@@\n-old\n+new"), "notes/a.md"),
            Err(PatchError::ContextNotFound { hunk: 1 })
        );
    }

    #[test]
    fn rejects_ambiguous_context() {
        assert_eq!(
            apply_note_patch("old\nold", &patch("@@\n-old\n+new"), "notes/a.md"),
            Err(PatchError::AmbiguousContext { hunk: 1, count: 2 })
        );
    }

    #[test]
    fn anchors_disambiguate() {
        let content = "# A\nitem\n# B\nitem\n";
        let output = apply_note_patch(content, &patch("@@ # B\n-item\n+changed"), "notes/a.md");
        assert_eq!(output.unwrap(), "# A\nitem\n# B\nchanged\n");
        assert_eq!(
            apply_note_patch(content, &patch("@@ # C\n-item\n+x"), "notes/a.md"),
            Err(PatchError::AnchorNotFound {
                hunk: 1,
                anchor: "# C".into()
            })
        );
    }

    #[test]
    fn rejects_add_file_operation() {
        let patch = "*** Begin Patch\n*** Add File: notes/a.md\n+new\n*** End Patch";
        assert!(matches!(
            apply_note_patch("", patch, "notes/a.md"),
            Err(PatchError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn applies_multiple_hunks() {
        let output = apply_note_patch(
            "one\ntwo\nthree\n",
            &patch("@@\n-one\n+1\n@@\n-three\n+3"),
            "notes/a.md",
        );
        assert_eq!(output.unwrap(), "1\ntwo\n3\n");
    }

    #[test]
    fn applies_delete_only_hunk() {
        let output = apply_note_patch(
            "keep\nremove\nend\n",
            &patch("@@\n keep\n-remove\n end"),
            "notes/a.md",
        );
        assert_eq!(output.unwrap(), "keep\nend\n");
    }

    #[test]
    fn preserves_missing_trailing_newline() {
        let output = apply_note_patch("old", &patch("@@\n-old\n+new"), "notes/a.md");
        assert_eq!(output.unwrap(), "new");
    }

    #[test]
    fn add_only_hunks_append_or_insert_after_the_anchor() {
        let output = apply_note_patch("", &patch("@@\n+# Title\n+body"), "notes/a.md");
        assert_eq!(output.unwrap(), "# Title\nbody\n");
        let output = apply_note_patch("a\nb\n", &patch("@@\n+c"), "notes/a.md");
        assert_eq!(output.unwrap(), "a\nb\nc\n");
        let output = apply_note_patch("a\nb\n", &patch("@@ a\n+between"), "notes/a.md");
        assert_eq!(output.unwrap(), "a\nbetween\nb\n");
    }

    #[test]
    fn end_of_file_pins_the_match_to_the_end() {
        let output = apply_note_patch(
            "x\nend\nx\n",
            &patch("@@\n-x\n+y\n*** End of File"),
            "notes/a.md",
        );
        assert_eq!(output.unwrap(), "x\nend\ny\n");
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let output = apply_note_patch(
            "a\r\nold\r\nb\r\n",
            "*** Begin Patch\r\n*** Update File: notes/a.md\r\n@@\r\n a\r\n-old\r\n+new\r\n*** End Patch\r\n",
            "notes/a.md",
        );
        assert_eq!(output.unwrap(), "a\r\nnew\r\nb\r\n");
    }

    #[test]
    fn tolerates_blank_context_without_a_space_and_whitespace_drift() {
        let output = apply_note_patch(
            "a\n\nb  \nc\n",
            &patch("@@\n a\n\n-b\n+B\n c"),
            "notes/a.md",
        );
        assert_eq!(output.unwrap(), "a\n\nB\nc\n");
    }

    #[test]
    fn rejects_missing_begin() {
        let patch = "*** Update File: notes/a.md\n@@\n-old\n+new\n*** End Patch";
        assert_eq!(
            apply_note_patch("old", patch, "notes/a.md"),
            Err(PatchError::MissingBegin)
        );
    }

    #[test]
    fn rejects_missing_end() {
        let patch = "*** Begin Patch\n*** Update File: notes/a.md\n@@\n-old\n+new";
        assert_eq!(
            apply_note_patch("old", patch, "notes/a.md"),
            Err(PatchError::MissingEnd)
        );
    }

    #[test]
    fn rejects_duplicate_update_sections() {
        let patch = "*** Begin Patch\n*** Update File: notes/a.md\n*** Update File: notes/a.md\n@@\n-old\n+new\n*** End Patch";
        assert_eq!(
            apply_note_patch("old", patch, "notes/a.md"),
            Err(PatchError::InvalidUpdateCount)
        );
    }

    #[test]
    fn rejects_missing_hunk() {
        assert_eq!(
            apply_note_patch("old", &patch("-old\n+new"), "notes/a.md"),
            Err(PatchError::MissingHunk)
        );
    }

    #[test]
    fn rejects_invalid_line() {
        assert_eq!(
            apply_note_patch("old", &patch("@@\nold"), "notes/a.md"),
            Err(PatchError::InvalidLine {
                line: 4,
                text: "old".into()
            })
        );
    }
}
