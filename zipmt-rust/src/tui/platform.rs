use std::process::Command;

pub(super) struct ProcessMetrics {
    pub(super) cpu_ticks: Option<u64>,
    pub(super) resident_memory_bytes: Option<usize>,
}

pub(super) fn parse_process_cpu_ticks(stat: &str) -> Option<u64> {
    let fields = stat.get(stat.rfind(')')? + 1..)?.split_whitespace();
    let fields = fields.collect::<Vec<_>>();
    Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
}

pub(super) fn parse_resident_memory(status: &str) -> Option<usize> {
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib = line.split_whitespace().nth(1)?.parse::<usize>().ok()?;
    Some(kib.saturating_mul(1024))
}

pub(super) fn sample_process_metrics() -> ProcessMetrics {
    ProcessMetrics {
        cpu_ticks: std::fs::read_to_string("/proc/self/stat")
            .ok()
            .and_then(|stat| parse_process_cpu_ticks(&stat)),
        resident_memory_bytes: std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| parse_resident_memory(&status)),
    }
}

pub fn query_initial_size() -> (u16, u16) {
    env_size()
        .or_else(stty_size)
        .or_else(tput_size)
        .unwrap_or_else(|| crossterm::terminal::size().unwrap_or((80, 24)))
}

fn env_size() -> Option<(u16, u16)> {
    let cols = std::env::var("COLUMNS").ok()?.parse::<u16>().ok()?;
    let rows = std::env::var("LINES").ok()?.parse::<u16>().ok()?;
    Some((cols, rows))
}

fn stty_size() -> Option<(u16, u16)> {
    terminal_paths().find_map(|path| {
        let file = std::fs::File::open(path).ok()?;
        let output = Command::new("stty").arg("size").stdin(file).output().ok()?;
        let mut parts = std::str::from_utf8(&output.stdout).ok()?.split_whitespace();
        let rows = parts.next()?.parse::<u16>().ok()?;
        let cols = parts.next()?.parse::<u16>().ok()?;
        (cols > 0 && rows > 0).then_some((cols, rows))
    })
}

fn tput_size() -> Option<(u16, u16)> {
    terminal_paths().find_map(|path| {
        let file = std::fs::File::open(path).ok()?;
        let file_clone = file.try_clone().ok()?;
        let cols = Command::new("tput").arg("cols").stdin(file).output().ok()?;
        let rows = Command::new("tput")
            .arg("lines")
            .stdin(file_clone)
            .output()
            .ok()?;
        let cols = std::str::from_utf8(&cols.stdout)
            .ok()?
            .trim()
            .parse::<u16>()
            .ok()?;
        let rows = std::str::from_utf8(&rows.stdout)
            .ok()?
            .trim()
            .parse::<u16>()
            .ok()?;
        (cols > 0 && rows > 0).then_some((cols, rows))
    })
}

fn terminal_paths() -> impl Iterator<Item = &'static str> {
    ["/dev/tty", "/dev/stderr", "/proc/self/fd/2"].into_iter()
}
