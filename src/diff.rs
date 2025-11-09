use anyhow::{Context, Result};
use colored::*;
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::Path;

/// Display a colored diff between two files
pub fn diff_files(path1: &str, path2: &str) -> Result<()> {
    let content1 = fs::read_to_string(path1)
        .with_context(|| format!("Failed to read first file: {}", path1))?;
    let content2 = fs::read_to_string(path2)
        .with_context(|| format!("Failed to read second file: {}", path2))?;

    display_diff(&content1, &content2, path1, path2);

    Ok(())
}

/// Display a colored diff between two strings
pub fn display_diff(old: &str, new: &str, old_label: &str, new_label: &str) {
    let diff = TextDiff::from_lines(old, new);

    println!(
        "{} {}",
        "Comparing:".cyan().bold(),
        format!("{} <-> {}", old_label, new_label).bright_white()
    );
    println!("{}", "=".repeat(80).bright_black());

    let mut has_changes = false;
    let mut line_num = 1;

    for change in diff.iter_all_changes() {
        let (sign, style_fn): (&str, fn(&str) -> ColoredString) = match change.tag() {
            ChangeTag::Delete => ("-", |s: &str| s.red()),
            ChangeTag::Insert => ("+", |s: &str| s.green()),
            ChangeTag::Equal => (" ", |s: &str| s.normal()),
        };

        print!(
            "{} {} │ {}",
            sign.bold(),
            format!("{:4}", line_num).truecolor(150, 150, 150),
            style_fn(&change.to_string_lossy())
        );

        if !change.to_string_lossy().ends_with('\n') {
            println!();
        }

        has_changes = has_changes || change.tag() != ChangeTag::Equal;

        if change.tag() != ChangeTag::Delete {
            line_num += 1;
        }
    }

    println!("{}", "=".repeat(80).bright_black());

    if !has_changes {
        println!("{} No differences found", "✓".green().bold());
    } else {
        let stats = diff_stats(&diff);
        println!(
            "\n{} {} additions, {} deletions",
            "Summary:".cyan().bold(),
            stats.additions.to_string().green(),
            stats.deletions.to_string().red()
        );
    }
}

struct DiffStats {
    additions: usize,
    deletions: usize,
}

fn diff_stats<'a>(diff: &TextDiff<'a, 'a, 'a, str>) -> DiffStats {
    let mut additions = 0;
    let mut deletions = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => additions += 1,
            ChangeTag::Delete => deletions += 1,
            ChangeTag::Equal => {}
        }
    }

    DiffStats {
        additions,
        deletions,
    }
}

/// Compare the current fstab with a backup or other file
pub fn compare_with_current(other_file: &str) -> Result<()> {
    let fstab_path = "/etc/fstab";

    if !Path::new(fstab_path).exists() {
        anyhow::bail!("/etc/fstab does not exist on this system");
    }

    diff_files(fstab_path, other_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_identical() {
        let text = "line1\nline2\nline3\n";
        let diff = TextDiff::from_lines(text, text);
        let stats = diff_stats(&diff);
        assert_eq!(stats.additions, 0);
        assert_eq!(stats.deletions, 0);
    }

    #[test]
    fn test_diff_additions() {
        let old = "line1\nline2\n";
        let new = "line1\nline2\nline3\n";
        let diff = TextDiff::from_lines(old, new);
        let stats = diff_stats(&diff);
        assert_eq!(stats.additions, 1);
        assert_eq!(stats.deletions, 0);
    }

    #[test]
    fn test_diff_deletions() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nline3\n";
        let diff = TextDiff::from_lines(old, new);
        let stats = diff_stats(&diff);
        assert_eq!(stats.additions, 0);
        assert_eq!(stats.deletions, 1);
    }

    #[test]
    fn test_diff_changes() {
        let old = "line1\nold line\nline3\n";
        let new = "line1\nnew line\nline3\n";
        let diff = TextDiff::from_lines(old, new);
        let stats = diff_stats(&diff);
        assert_eq!(stats.additions, 1);
        assert_eq!(stats.deletions, 1);
    }
}
