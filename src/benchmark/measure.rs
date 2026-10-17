//! Running a command and measuring it: how long it took, and how much memory.
//!
//! Wall time is the clock time from starting the command to its end. Memory is
//! two figures, because a tool that spreads its work over several processes
//! doesn't have one memory use:
//!
//! * the peak resident set size of the largest single process, which the
//!   kernel reports exactly when the command ends;
//! * the peak of the whole process tree, added up while the command runs. It
//!   uses proportional set size where the system offers it, so memory shared
//!   between forked workers is counted once. It is sampled, so it can miss a
//!   short peak, and it is only taken on Linux.

use std::io;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Measured {
    pub wall: Duration,

    /// The command ended with exit code 0.
    pub success: bool,

    /// The exit code, if it ended by exiting.
    pub exit_code: Option<i32>,

    /// Peak resident set size of the largest process, in bytes.
    pub peak_process: Option<u64>,

    /// Peak memory of the command and everything it started, in bytes. Only
    /// when sampling was asked for, and on Linux.
    pub peak_tree: Option<u64>,
}

/// Runs `command` with its output thrown away, and measures it.
pub fn measure(command: &mut Command, sample_tree: bool) -> io::Result<Measured> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let start = Instant::now();
    let child = command.spawn()?;
    let pid = child.id();

    let sampler = sample_tree.then(|| tree::Sampler::start(pid)).flatten();

    let waited = wait(pid, child);
    let wall = start.elapsed();
    let peak_tree = sampler.map(tree::Sampler::stop);

    let (exit_code, peak_process) = waited?;

    Ok(Measured {
        wall,
        success: exit_code == Some(0),
        exit_code,
        peak_process,
        peak_tree: peak_tree.filter(|peak| *peak > 0),
    })
}

/// Waits for the child and returns its exit code and the peak memory of its
/// largest process.
#[cfg(unix)]
fn wait(pid: u32, child: std::process::Child) -> io::Result<(Option<i32>, Option<u64>)> {
    let mut status: libc::c_int = 0;
    // SAFETY: `rusage` is plain data that `wait4` fills in, and zeroed is a
    // valid value for it.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };

    loop {
        // SAFETY: both pointers are to live locals, and `pid` is our child. It
        // is waited for here and nowhere else: dropping `Child` doesn't wait.
        let result = unsafe { libc::wait4(pid as libc::pid_t, &mut status, 0, &mut usage) };

        if result >= 0 {
            break;
        }

        let error = io::Error::last_os_error();

        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }

    drop(child);

    let exit_code = libc::WIFEXITED(status).then(|| libc::WEXITSTATUS(status));

    // The kernel counts the larger of what this process held when the command
    // started and what the command held, a few MiB at most for this program,
    // far less than either tool uses.
    //
    // Kibibytes on Linux, bytes on macOS.
    let peak = if cfg!(target_os = "macos") {
        usage.ru_maxrss as u64
    } else {
        usage.ru_maxrss as u64 * 1024
    };

    Ok((exit_code, Some(peak)))
}

#[cfg(not(unix))]
fn wait(_pid: u32, mut child: std::process::Child) -> io::Result<(Option<i32>, Option<u64>)> {
    let status = child.wait()?;

    Ok((status.code(), None))
}

#[cfg(target_os = "linux")]
mod tree {
    //! Adding up the memory of a process and its descendants while it runs.

    use std::collections::HashMap;
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    const INTERVAL: Duration = Duration::from_millis(5);

    pub struct Sampler {
        stop: Arc<AtomicBool>,
        handle: JoinHandle<u64>,
    }

    impl Sampler {
        pub fn start(root: u32) -> Option<Sampler> {
            let stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);

            let handle = thread::Builder::new()
                .name("benchmark-sampler".to_string())
                .spawn(move || {
                    let mut peak = 0;

                    loop {
                        peak = peak.max(tree_memory(root));

                        if flag.load(Ordering::Relaxed) {
                            return peak;
                        }

                        thread::sleep(INTERVAL);
                    }
                })
                .ok()?;

            Some(Sampler { stop, handle })
        }

        pub fn stop(self) -> u64 {
            self.stop.store(true, Ordering::Relaxed);
            self.handle.join().unwrap_or(0)
        }
    }

    /// The memory of `root` and its descendants, in bytes.
    fn tree_memory(root: u32) -> u64 {
        let Ok(entries) = fs::read_dir("/proc") else {
            return 0;
        };

        let mut parents: HashMap<u32, u32> = HashMap::new();

        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };

            if let Some(parent) = fs::read_to_string(entry.path().join("stat"))
                .ok()
                .and_then(|stat| parent_of(&stat))
            {
                parents.insert(pid, parent);
            }
        }

        parents
            .keys()
            .copied()
            .filter(|pid| descends_from(&parents, *pid, root))
            .map(process_memory)
            .sum()
    }

    pub(super) fn descends_from(parents: &HashMap<u32, u32>, mut pid: u32, root: u32) -> bool {
        // Bounded, in case the table changes under us into a loop.
        for _ in 0..64 {
            if pid == root {
                return true;
            }

            match parents.get(&pid) {
                Some(parent) if *parent != 0 && *parent != pid => pid = *parent,
                _ => return false,
            }
        }

        false
    }

    /// The parent in the text of `/proc/<pid>/stat`: the name is in brackets
    /// and may hold anything, so the fields are counted from the last bracket.
    pub(super) fn parent_of(stat: &str) -> Option<u32> {
        let after_name = &stat[stat.rfind(')')? + 1..];

        after_name.split_whitespace().nth(1)?.parse().ok()
    }

    /// Proportional set size where there is one, else resident set size.
    fn process_memory(pid: u32) -> u64 {
        let path = format!("/proc/{pid}");

        fs::read_to_string(format!("{path}/smaps_rollup"))
            .ok()
            .and_then(|text| kilobytes(&text, "Pss:"))
            .or_else(|| {
                fs::read_to_string(format!("{path}/status"))
                    .ok()
                    .and_then(|text| kilobytes(&text, "VmRSS:"))
            })
            .map_or(0, |kb| kb * 1024)
    }

    /// The number on the line that starts with `label`, which is in kB.
    pub(super) fn kilobytes(text: &str, label: &str) -> Option<u64> {
        text.lines()
            .find_map(|line| line.strip_prefix(label))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    }
}

#[cfg(not(target_os = "linux"))]
mod tree {
    pub struct Sampler;

    impl Sampler {
        pub fn start(_root: u32) -> Option<Sampler> {
            None
        }

        pub fn stop(self) -> u64 {
            0
        }
    }
}

/// The middle value, or the mean of the two middle ones.
pub fn median(durations: &[Duration]) -> Option<Duration> {
    let mut sorted = durations.to_vec();

    sorted.sort();

    match sorted.len() {
        0 => None,
        length if length % 2 == 1 => Some(sorted[length / 2]),
        length => Some((sorted[length / 2 - 1] + sorted[length / 2]) / 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millis(values: &[u64]) -> Vec<Duration> {
        values.iter().map(|v| Duration::from_millis(*v)).collect()
    }

    #[test]
    fn the_median_is_the_middle_value() {
        assert_eq!(
            median(&millis(&[30, 10, 20])),
            Some(Duration::from_millis(20))
        );
    }

    #[test]
    fn the_median_of_an_even_number_is_the_mean_of_the_middle_two() {
        assert_eq!(
            median(&millis(&[10, 40, 20, 30])),
            Some(Duration::from_millis(25))
        );
    }

    #[test]
    fn there_is_no_median_of_nothing() {
        assert_eq!(median(&[]), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_command_is_timed() {
        let measured = measure(Command::new("sh").args(["-c", "sleep 0.2"]), false).unwrap();

        assert!(measured.success);
        assert_eq!(measured.exit_code, Some(0));
        assert!(measured.wall >= Duration::from_millis(200), "{measured:?}");
        assert!(measured.wall < Duration::from_secs(5), "{measured:?}");
        assert!(measured.peak_process.is_some_and(|peak| peak > 0));
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_command_is_reported_as_one() {
        let measured = measure(Command::new("sh").args(["-c", "exit 3"]), false).unwrap();

        assert!(!measured.success);
        assert_eq!(measured.exit_code, Some(3));
    }

    #[cfg(unix)]
    #[test]
    fn a_command_that_cannot_start_is_an_error() {
        assert!(measure(&mut Command::new("/no/such/command"), false).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn memory_is_added_up_across_the_processes_a_command_starts() {
        let measured = measure(
            Command::new("sh").args(["-c", "sleep 0.3 & sleep 0.3 & wait"]),
            true,
        )
        .unwrap();

        assert!(
            measured.peak_tree.is_some_and(|peak| peak > 0),
            "{measured:?}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_belongs_to_the_tree_of_its_ancestors_only() {
        use std::collections::HashMap;

        // 10 starts 20 and 30, and 20 starts 40. 50 is somebody else's.
        let parents: HashMap<u32, u32> = [(20, 10), (30, 10), (40, 20), (50, 1), (10, 1)]
            .into_iter()
            .collect();

        assert!(tree::descends_from(&parents, 10, 10));
        assert!(tree::descends_from(&parents, 40, 10));
        assert!(tree::descends_from(&parents, 30, 10));
        assert!(!tree::descends_from(&parents, 50, 10));
        assert!(!tree::descends_from(&parents, 1, 10));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_parent_is_read_from_a_name_with_brackets_in_it() {
        assert_eq!(tree::parent_of("123 (a) b) S 45 1 1 0"), Some(45));
        assert_eq!(tree::parent_of("1 (sh) S 0 1 1"), Some(0));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kilobytes_are_read_from_a_labelled_line() {
        let text = "Rss:       100 kB\nPss:       64 kB\n";

        assert_eq!(tree::kilobytes(text, "Pss:"), Some(64));
        assert_eq!(tree::kilobytes(text, "Missing:"), None);
    }
}
