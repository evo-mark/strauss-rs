//! Checking what a tool wrote, so a fast run that wrote nothing, or broken PHP,
//! doesn't pass for a good one.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize)]
pub struct Check {
    /// The `.php` files written.
    pub php_files: usize,

    /// Files PHP could not parse, with why. `None` when PHP wasn't asked.
    pub parse_errors: Option<Vec<String>>,
}

/// Counts the `.php` files under `directory`.
pub fn count_php_files(directory: &Path) -> usize {
    let Ok(entries) = fs::read_dir(directory) else {
        return 0;
    };

    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();

            if path.is_dir() {
                count_php_files(&path)
            } else {
                usize::from(path.extension().is_some_and(|extension| extension == "php"))
            }
        })
        .sum()
}

/// Every `.php` file in the directory is run through PHP's parser, in one
/// process.
const PARSE_SCRIPT: &str = r#"
$root = $argv[1];
$files = 0;
$errors = [];
$iterator = new RecursiveIteratorIterator(
    new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS)
);
foreach ($iterator as $file) {
    if ($file->getExtension() !== 'php') {
        continue;
    }
    $files++;
    try {
        token_get_all(file_get_contents($file->getPathname()), TOKEN_PARSE);
    } catch (ParseError $error) {
        $errors[] = substr($file->getPathname(), strlen($root) + 1) . ': ' . $error->getMessage();
    }
}
echo json_encode(['files' => $files, 'errors' => $errors]);
"#;

#[derive(Debug, Deserialize)]
struct ParseReport {
    errors: Vec<String>,
}

/// Counts the files, and has PHP parse them if a PHP is given.
pub fn check(directory: &Path, php: Option<&Path>) -> Check {
    let mut check = Check {
        php_files: count_php_files(directory),
        parse_errors: None,
    };

    let Some(php) = php else {
        return check;
    };

    let output = Command::new(php)
        .arg("-r")
        .arg(PARSE_SCRIPT)
        .arg(directory)
        .output();

    check.parse_errors = match output {
        Ok(output) if output.status.success() => {
            serde_json::from_slice::<ParseReport>(&output.stdout)
                .map(|report| report.errors)
                .ok()
        }
        _ => None,
    };

    check
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_files_are_counted_in_nested_directories() {
        let root = std::env::temp_dir().join(format!("strauss-rs-check-{}", std::process::id()));

        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("one.php"), "<?php").unwrap();
        fs::write(root.join("a/two.php"), "<?php").unwrap();
        fs::write(root.join("a/b/three.php"), "<?php").unwrap();
        fs::write(root.join("a/b/readme.md"), "").unwrap();

        let counted = count_php_files(&root);

        fs::remove_dir_all(&root).unwrap();

        assert_eq!(counted, 3);
    }

    #[test]
    fn a_missing_directory_has_no_files() {
        assert_eq!(count_php_files(Path::new("/no/such/directory")), 0);
    }

    #[test]
    fn without_a_php_only_the_files_are_counted() {
        let check = check(Path::new("/no/such/directory"), None);

        assert_eq!(check.php_files, 0);
        assert!(check.parse_errors.is_none());
    }
}
