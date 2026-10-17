//! Structural validation of the portable `.agents/skills/QA` package.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Check the package, preserving independent portable leaves rather than generating them.
/// Metadata deliberately uses the package's two single-line scalar YAML fields. Complex YAML
/// is rejected so a format change requires an explicit validator update, not a silent bypass.
pub fn validate(root: &Path) -> Result<usize, String> {
    let root = root.canonicalize().map_err(|e| format!("{}: {e}", root.display()))?;
    let mut skills = Vec::new();
    let mut names = BTreeSet::new();
    for entry in entries(&root)? {
        if entry.is_dir() {
            let path = entry.join("SKILL.md");
            let name = metadata(&read(&path)?, &path)?;
            if !names.insert(name.clone()) {
                return Err(format!("{}: duplicate skill name {name}", path.display()));
            }
            skills.push((entry, name));
        }
    }
    if skills.is_empty() {
        return Err(format!("{}: no skills found", root.display()));
    }
    for (dir, name) in &skills {
        if dir.file_name().and_then(|n| n.to_str()) != Some(name) {
            return Err(format!("{}: directory must agree with skill name {name}", dir.display()));
        }
    }

    let mut notices: Option<(PathBuf, String)> = None;
    for (dir, _) in &skills {
        let path = dir.join("THIRD_PARTY_NOTICES.md");
        let text = read(&path)?;
        if text.trim().is_empty() {
            return Err(format!("{}: empty notice", path.display()));
        }
        if let Some((first, expected)) = &notices {
            if &text != expected {
                return Err(format!("portable notices differ: {} and {}", first.display(), path.display()));
            }
        } else {
            notices = Some((path, text));
        }
    }

    let mut markdown = Vec::new();
    markdown_files(&root, 0, &mut markdown)?;
    let mut references = BTreeMap::<String, (PathBuf, String)>::new();
    let mut covered = BTreeSet::new();
    for path in markdown {
        let text = read(&path)?;
        for (destination, used) in destinations(&text)? {
            if let Some(target) = local_link(&root, &path, &destination)?
                && used
                && path == root.join("README.md")
            {
                covered.insert(target);
            }
        }
        if path.strip_prefix(&root).is_ok_and(|relative| relative.components().any(|component| component.as_os_str() == "references")) {
            let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| format!("{}: non-UTF-8 reference name", path.display()))?;
            if let Some((first, expected)) = references.get(name) {
                if &text != expected {
                    return Err(format!("reference copies differ: {} and {}", first.display(), path.display()));
                }
            } else {
                references.insert(name.to_owned(), (path, text));
            }
        }
    }
    // Reading separately also makes a missing README an error when no other link names it.
    read(&root.join("README.md"))?;
    for (dir, _) in &skills {
        if !covered.contains(&dir.join("SKILL.md")) {
            return Err(format!("README coverage missing for {}", dir.display()));
        }
    }
    Ok(skills.len())
}

fn entries(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        if entry.file_type().map_err(|e| e.to_string())?.is_symlink() {
            return Err(format!("{}: portable package must not use symlinks", entry.path().display()));
        }
        paths.push(entry.path());
    }
    paths.sort();
    Ok(paths)
}

fn read(path: &Path) -> Result<String, String> {
    if fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len() > 1024 * 1024 {
        return Err(format!("{}: skill text exceeds 1 MiB", path.display()));
    }
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn markdown_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 16 {
        return Err(format!("{}: package nesting exceeds 16 directories", dir.display()));
    }
    for path in entries(dir)? {
        if path.is_dir() {
            markdown_files(&path, depth.saturating_add(1), out)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
    Ok(())
}

fn metadata(text: &str, path: &Path) -> Result<String, String> {
    let mut lines = text.lines();
    let invalid = || format!("{}: expected name and description scalar metadata between --- lines", path.display());
    if lines.next() != Some("---") {
        return Err(invalid());
    }
    let mut values = BTreeMap::new();
    let mut closed = false;
    for line in lines {
        if line == "---" {
            closed = true;
            break;
        }
        let (key, value) = line.split_once(':').ok_or_else(invalid)?;
        let value = value.trim();
        if !matches!(key, "name" | "description") || value.starts_with(['|', '>', '[', '{', '\'', '"']) {
            return Err(invalid());
        }
        if values.insert(key, value).is_some() {
            return Err(format!("{}: duplicate metadata field {key}", path.display()));
        }
    }
    if !closed {
        return Err(invalid());
    }
    let name = values.get("name").copied().ok_or_else(invalid)?;
    if !plain_string(name)
        || name.len() > 64
        || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
    {
        return Err(format!("{}: invalid skill name {name:?}", path.display()));
    }
    let description = values.get("description").copied().ok_or_else(invalid)?;
    if !plain_string(description) || description.len() > 1024 {
        return Err(format!("{}: description must be a nonempty scalar of at most 1024 bytes", path.display()));
    }
    Ok(name.to_owned())
}

fn plain_string(value: &str) -> bool {
    !value.is_empty()
        && !value.eq_ignore_ascii_case("null")
        && !matches!(value, "~" | "true" | "false" | "True" | "False" | "TRUE" | "FALSE")
        && value.parse::<f64>().is_err()
        && !value.contains(": ")
        && !value.starts_with(['#', '!', '&', '*', '?', '%', '@', '`'])
}

/// Inline Markdown links and link definitions used by this package. Ignore fenced examples,
/// inline code and HTML comments; neither a commented link nor a code example covers a skill.
fn destinations(text: &str) -> Result<Vec<(String, bool)>, String> {
    let mut visible = String::new();
    let mut comment = false;
    let mut fence: Option<char> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        if marker.is_some() && (fence.is_none() || marker == fence) {
            fence = if fence.is_some() { None } else { marker };
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let mut rest = line;
        while !rest.is_empty() {
            if comment {
                if let Some((_, tail)) = rest.split_once("-->") {
                    comment = false;
                    rest = tail;
                } else {
                    break;
                }
            } else if rest.starts_with("<!--") {
                comment = true;
                rest = rest.strip_prefix("<!--").unwrap_or_default();
            } else if rest.starts_with('`') {
                let ticks = rest.chars().take_while(|c| *c == '`').count();
                let delimiter = "`".repeat(ticks);
                rest = rest.get(ticks..).unwrap_or_default().split_once(&delimiter).map_or("", |(_, tail)| tail);
            } else if let Some(c) = rest.chars().next() {
                visible.push(c);
                rest = rest.get(c.len_utf8()..).unwrap_or_default();
            }
        }
        visible.push('\n');
    }
    let mut out = Vec::new();
    let mut definitions = BTreeMap::new();
    let mut prose = String::new();
    for line in visible.lines() {
        let line = line.trim_start();
        if let Some(tail) = line.strip_prefix('[')
            && let Some((label, target)) = tail.split_once("]:")
        {
            let target = link_target(target.trim())?;
            definitions.insert(label.to_ascii_lowercase(), target.clone());
            out.push((target, false));
        } else {
            prose.push_str(line);
            prose.push('\n');
        }
    }
    let mut rest = prose.as_str();
    while let Some((_, tail)) = rest.split_once('[') {
        let Some((label, after)) = tail.split_once(']') else { break };
        rest = after;
        let Some(tail) = after.strip_prefix('(') else {
            let target = if let Some(reference) = after.strip_prefix('[') {
                let (reference, after) = reference.split_once(']').ok_or_else(|| "unterminated Markdown reference".to_owned())?;
                rest = after;
                let reference = if reference.is_empty() { label } else { reference };
                Some(definitions.get(&reference.to_ascii_lowercase()).ok_or_else(|| format!("undefined Markdown reference {reference:?}"))?)
            } else {
                definitions.get(&label.to_ascii_lowercase())
            };
            if let Some(target) = target {
                out.push((target.clone(), true));
            }
            continue;
        };
        let mut depth = 0usize;
        let end = tail
            .char_indices()
            .find_map(|(offset, c)| match c {
                '(' => {
                    depth = depth.saturating_add(1);
                    None
                }
                ')' if depth == 0 => Some(offset),
                ')' => {
                    depth = depth.saturating_sub(1);
                    None
                }
                _ => None,
            })
            .ok_or_else(|| "unterminated Markdown link destination".to_owned())?;
        let destination = tail.get(..end).unwrap_or_default().trim();
        out.push((link_target(destination)?, true));
        rest = tail.get(end.saturating_add(1)..).unwrap_or_default();
    }
    Ok(out)
}

fn link_target(destination: &str) -> Result<String, String> {
    if let Some(tail) = destination.strip_prefix('<') {
        return tail.split_once('>').map(|(target, _)| target.to_owned()).ok_or_else(|| "unterminated Markdown angle destination".to_owned());
    }
    Ok(destination.split_whitespace().next().unwrap_or_default().to_owned())
}

fn local_link(root: &Path, source: &Path, destination: &str) -> Result<Option<PathBuf>, String> {
    if destination.starts_with('/') && !destination.starts_with("//") {
        return Err(format!("{}: absolute local link is not portable: {destination}", source.display()));
    }
    if destination.starts_with('#')
        || destination.starts_with("//")
        || destination
            .split_once(':')
            .is_some_and(|(scheme, _)| !scheme.is_empty() && scheme.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')))
    {
        return Ok(None);
    }
    let relative = destination.split(['#', '?']).next().unwrap_or_default();
    if relative.is_empty() {
        return Ok(None);
    }
    let path = source.parent().ok_or_else(|| format!("{}: no parent", source.display()))?.join(relative);
    let target = path.canonicalize().map_err(|e| format!("{}: missing local link {destination:?}: {e}", source.display()))?;
    // README is repository navigation, not a portable skill leaf. It may link to the owning
    // repository's rules; each leaf must carry all of its own references and notices.
    if source != root.join("README.md") {
        if !target.starts_with(root) {
            return Err(format!("{}: local link escapes portable package: {destination}", source.display()));
        }
        let leaf = source
            .strip_prefix(root)
            .ok()
            .and_then(|relative| relative.components().next())
            .ok_or_else(|| format!("{}: source has no portable leaf", source.display()))?;
        if !target.starts_with(root.join(leaf)) {
            return Err(format!("{}: local link escapes portable leaf: {destination}", source.display()));
        }
    }
    Ok(Some(target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Package(PathBuf);

    impl Package {
        fn healthy() -> Self {
            let root = std::env::temp_dir().join(format!("pdfcraft-skills-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            // create_dir reserves this test's directory without accepting an existing one.
            fs::create_dir(&root).unwrap();
            let package = Self(root);
            package.write("README.md", "[Alpha](alpha/SKILL.md)\n[Beta](beta/SKILL.md)\n");
            for name in ["alpha", "beta"] {
                package.write(
                    &format!("{name}/SKILL.md"),
                    &format!(
                        "---\nname: {name}\ndescription: Use when a {name} case needs checking.\n---\n\n[Contract](references/contract.md)\n[Licence](THIRD_PARTY_NOTICES.md)\n"
                    ),
                );
                package.write(&format!("{name}/references/contract.md"), "# Shared contract\n");
                package.write(&format!("{name}/THIRD_PARTY_NOTICES.md"), "# Portable notice\nCopyright Example\n");
            }
            package
        }

        fn write(&self, relative: &str, text: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }

        fn reject(&self, expected: &str) {
            let error = validate(&self.0).expect_err("invalid package must fail");
            assert!(error.contains(expected), "expected {expected:?}, got {error:?}");
        }
    }

    impl Drop for Package {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn accepts_portable_package_and_deliberate_synchronized_updates() {
        let package = Package::healthy();
        assert_eq!(validate(&package.0).unwrap(), 2);
        for name in ["alpha", "beta"] {
            package.write(&format!("{name}/references/contract.md"), "# Revised shared contract\n");
            package.write(&format!("{name}/THIRD_PARTY_NOTICES.md"), "# Revised portable notice\n");
        }
        assert_eq!(validate(&package.0).unwrap(), 2);
    }

    #[test]
    fn rejects_reference_copy_drift() {
        let package = Package::healthy();
        package.write("beta/references/contract.md", "# Stale contract\n");
        package.reject("reference copies differ");
    }

    #[test]
    fn rejects_missing_local_reference_in_skill_or_shared_reference() {
        for file in ["alpha/SKILL.md", "alpha/references/contract.md"] {
            let package = Package::healthy();
            let text = fs::read_to_string(package.0.join(file)).unwrap();
            package.write(file, &(text + "\n[Missing](missing.md#section)\n"));
            package.reject("missing local link");
        }
    }

    #[test]
    fn rejects_invalid_metadata_and_directory_disagreement() {
        for (metadata, error) in [
            ("name: alpha\ndescription: \n", "description"),
            ("name: alpha\nname: alpha\ndescription: Good\n", "duplicate metadata"),
            ("name: Bad_Name\ndescription: Good\n", "invalid skill name"),
            ("name: other\ndescription: Good\n", "directory"),
            ("name alpha\ndescription: Good\n", "metadata"),
            ("name: alpha\ndescription: |\n  Multiline\n", "metadata"),
        ] {
            let package = Package::healthy();
            package.write("alpha/SKILL.md", &format!("---\n{metadata}---\n# Skill\n"));
            package.reject(error);
        }
        let package = Package::healthy();
        package.write("alpha/SKILL.md", "# No front matter\n");
        package.reject("metadata");
    }

    #[test]
    fn rejects_duplicate_skill_names() {
        let package = Package::healthy();
        package.write("beta/SKILL.md", "---\nname: alpha\ndescription: Duplicate\n---\n");
        package.reject("duplicate skill name");
    }

    #[test]
    fn rejects_missing_readme_coverage_and_ignores_commented_links() {
        for readme in ["[Alpha](alpha/SKILL.md)\n", "[Alpha](alpha/SKILL.md)\n<!-- [Beta](beta/SKILL.md) -->\n"] {
            let package = Package::healthy();
            package.write("README.md", readme);
            package.reject("README coverage");
        }
    }

    #[test]
    fn rejects_missing_empty_and_different_portable_notices() {
        let package = Package::healthy();
        fs::remove_file(package.0.join("beta/THIRD_PARTY_NOTICES.md")).unwrap();
        package.reject("THIRD_PARTY_NOTICES.md");
        package.write("beta/THIRD_PARTY_NOTICES.md", "");
        package.reject("empty notice");
        package.write("beta/THIRD_PARTY_NOTICES.md", "# Different notice\n");
        package.reject("portable notices differ");
    }

    #[test]
    fn accepts_crlf_metadata_and_external_and_fragment_links() {
        let package = Package::healthy();
        package.write("alpha/SKILL.md", "---\r\nname: alpha\r\ndescription: A valid description\r\n---\r\n[Anchor](#one) [Web](https://example.test/missing) [Mail](mailto:person@example.test)\r\n");
        assert_eq!(validate(&package.0).unwrap(), 2);
    }

    #[test]
    fn rejects_empty_package_and_missing_skill_file() {
        let package = Package::healthy();
        fs::remove_file(package.0.join("beta/SKILL.md")).unwrap();
        package.reject("SKILL.md");
        fs::remove_dir_all(package.0.join("alpha")).unwrap();
        fs::remove_dir_all(package.0.join("beta")).unwrap();
        package.write("README.md", "# Empty package\n");
        package.reject("no skills");
    }

    #[test]
    fn rejects_unused_readme_definitions_and_undefined_references() {
        let package = Package::healthy();
        package.write("README.md", "[Alpha](alpha/SKILL.md)\n[beta]: beta/SKILL.md\n");
        package.reject("README coverage");
        package.write("README.md", "[Alpha](alpha/SKILL.md)\n[Beta][beta]\n");
        package.reject("undefined Markdown reference");
        package.write("README.md", "[Alpha](alpha/SKILL.md)\n[Beta][beta]\n[beta]: beta/SKILL.md\n");
        assert_eq!(validate(&package.0).unwrap(), 2);
    }

    #[test]
    fn rejects_nonportable_absolute_local_links() {
        let package = Package::healthy();
        package.write("alpha/SKILL.md", "---\nname: alpha\ndescription: Valid\n---\n[Local](/missing.md)\n");
        package.reject("absolute local link");
    }

    #[test]
    fn rejects_invalid_yaml_scalar_descriptions() {
        for description in ["false", "1", "Null", "NULL", "Something: invalid YAML", "# only a comment"] {
            let package = Package::healthy();
            package.write("alpha/SKILL.md", &format!("---\nname: alpha\ndescription: {description}\n---\n"));
            package.reject("description");
        }
    }

    #[test]
    fn rejects_nonstring_yaml_names_even_when_directory_matches() {
        for name in ["null", "123"] {
            let package = Package::healthy();
            fs::rename(package.0.join("alpha"), package.0.join(name)).unwrap();
            package.write(&format!("{name}/SKILL.md"), &format!("---\nname: {name}\ndescription: A valid description\n---\n"));
            package.write("README.md", &format!("[Alpha]({name}/SKILL.md)\n[Beta](beta/SKILL.md)\n"));
            package.reject("invalid skill name");
        }
    }

    #[test]
    fn accepts_self_leaf_references_but_rejects_cross_leaf_links() {
        let package = Package::healthy();
        assert_eq!(validate(&package.0).unwrap(), 2);
        let text = fs::read_to_string(package.0.join("alpha/SKILL.md")).unwrap();
        package.write("alpha/SKILL.md", &(text + "[Sibling](../beta/references/contract.md)\n"));
        package.reject("escapes portable leaf");
    }

    #[test]
    fn accepts_guide_navigation_but_rejects_escaping_leaf_links() {
        let guide = Package::healthy();
        guide.write("AGENTS.md", "# Repository rules\n");
        let directory = guide.0.file_name().unwrap().to_str().unwrap();
        let package = Package::healthy();
        package.write("README.md", &format!("[Alpha](alpha/SKILL.md)\n[Beta](beta/SKILL.md)\n[Rules](../{directory}/AGENTS.md#rules)\n"));
        assert_eq!(validate(&package.0).unwrap(), 2);
        let text = fs::read_to_string(package.0.join("alpha/SKILL.md")).unwrap();
        package.write("alpha/SKILL.md", &(text + &format!("[Rules](../../{directory}/AGENTS.md)\n")));
        package.reject("escapes portable package");
    }
}
