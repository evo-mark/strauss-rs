//! The tools that are compared, and how each is started.
//!
//! Both are asked for the same number of workers, so one worker is compared
//! with one worker and many with many. The number is always given outright.
//! Strauss PHP's own default is "one per spare CPU" and Strauss-RS's is one per
//! CPU, which would make the "multi" rows differ by a worker for no reason.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::ValueEnum;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
pub enum Tool {
    /// This program.
    #[value(name = "rs")]
    #[serde(rename = "strauss-rs")]
    StraussRs,

    /// Strauss PHP, from its phar.
    #[value(name = "php")]
    #[serde(rename = "strauss-php")]
    StraussPhp,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::StraussRs => "Strauss-RS",
            Tool::StraussPhp => "Strauss PHP",
        }
    }
}

/// What it takes to start either tool.
#[derive(Debug, Clone)]
pub struct Launcher {
    /// This program, for Strauss-RS.
    pub exe: PathBuf,

    /// The PHP binary, and the phar to run with it, for Strauss PHP.
    pub php: PathBuf,
    pub phar: PathBuf,
}

impl Launcher {
    /// The command that prefixes `project_dir` with `workers` workers.
    pub fn command(&self, tool: Tool, project_dir: &Path, workers: usize) -> Command {
        match tool {
            Tool::StraussRs => {
                let mut command = Command::new(&self.exe);

                command
                    .arg("--quiet")
                    .arg("prefix")
                    .arg("-C")
                    .arg(project_dir)
                    .arg(format!("--parallel={workers}"));

                command
            }

            Tool::StraussPhp => {
                let mut command = Command::new(&self.php);

                // No memory limit and no OPcache, the way a build script would
                // most likely run it from the command line.
                command
                    .args(["-d", "memory_limit=-1", "-d", "opcache.enable_cli=0"])
                    .arg(&self.phar)
                    .arg(format!("--parallel={workers}"))
                    .arg("--silent")
                    .current_dir(project_dir);

                command
            }
        }
    }
}

/// The path of a program: as given if it has a directory in it, else the first
/// match on `PATH`.
pub fn locate(program: &str) -> Option<PathBuf> {
    let given = Path::new(program);

    if given.components().count() > 1 {
        return given.is_file().then(|| given.to_path_buf());
    }

    env::split_paths(&env::var_os("PATH")?)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

/// Whether the file is a script and not a program. A `php` that is a script
/// usually starts PHP somewhere else, in a container say, and then the time it
/// takes to start is measured along with the work, and the memory is the
/// script's and not PHP's.
pub fn is_wrapper_script(path: &Path) -> bool {
    use std::io::Read;

    let mut start = [0u8; 2];

    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut start))
        .map(|()| &start == b"#!")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launcher() -> Launcher {
        Launcher {
            exe: PathBuf::from("/bin/strauss-rs"),
            php: PathBuf::from("/usr/bin/php"),
            phar: PathBuf::from("/tools/strauss.phar"),
        }
    }

    fn arguments(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn strauss_rs_is_given_the_project_and_the_workers() {
        let command = launcher().command(Tool::StraussRs, Path::new("/p"), 4);

        assert_eq!(command.get_program(), "/bin/strauss-rs");
        assert_eq!(
            arguments(&command),
            ["--quiet", "prefix", "-C", "/p", "--parallel=4"]
        );
    }

    #[test]
    fn strauss_php_runs_in_the_project_with_the_same_workers() {
        let command = launcher().command(Tool::StraussPhp, Path::new("/p"), 4);

        assert_eq!(command.get_program(), "/usr/bin/php");
        assert_eq!(command.get_current_dir(), Some(Path::new("/p")));

        let arguments = arguments(&command);

        assert!(arguments.contains(&"memory_limit=-1".to_string()));
        assert!(arguments.contains(&"opcache.enable_cli=0".to_string()));
        assert!(arguments.contains(&"/tools/strauss.phar".to_string()));
        assert!(arguments.contains(&"--parallel=4".to_string()));
    }

    #[test]
    fn one_worker_is_asked_for_outright() {
        for tool in [Tool::StraussRs, Tool::StraussPhp] {
            let command = launcher().command(tool, Path::new("/p"), 1);

            assert!(arguments(&command).contains(&"--parallel=1".to_string()));
        }
    }

    #[test]
    fn a_program_with_a_directory_is_not_looked_for_on_the_path() {
        assert_eq!(locate("/no/such/php"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_script_is_a_wrapper() {
        let path = env::temp_dir().join(format!("strauss-rs-wrapper-{}", std::process::id()));

        std::fs::write(&path, "#!/bin/sh\nexec php \"$@\"\n").unwrap();
        assert!(is_wrapper_script(&path));

        std::fs::write(&path, b"\x7fELF").unwrap();
        assert!(!is_wrapper_script(&path));

        std::fs::remove_file(&path).unwrap();
    }
}
