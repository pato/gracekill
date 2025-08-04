#![warn(clippy::all, clippy::pedantic, clippy::cargo)]

use std::convert::TryFrom;
use std::env;
use std::io::{self, Write};
use std::process;
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_GRACE_SECONDS: u64 = 30;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        process::exit(1);
    }

    let (pids, grace_period) = match parse_args(&args[1..]) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error: {e}");
            print_usage(&args[0]);
            process::exit(1);
        }
    };

    if pids.is_empty() {
        eprintln!("Error: No PIDs provided");
        process::exit(1);
    }

    log(&format!(
        "Starting graceful kill for {} process(es) with {}s grace period",
        pids.len(),
        grace_period.as_secs()
    ));

    // Send SIGTERM to all processes
    let mut active_pids = Vec::with_capacity(pids.len());
    for &pid in &pids {
        match send_signal(pid, Signal::Term) {
            Ok(()) => {
                log(&format!("Sent SIGTERM to PID {pid}"));
                active_pids.push(pid);
            }
            Err(e) => {
                log(&format!("Failed to send SIGTERM to PID {pid}: {e}"));
            }
        }
    }

    if active_pids.is_empty() {
        log("No processes to wait for");
        return;
    }

    // Wait for processes to exit gracefully
    let start = Instant::now();
    let mut remaining = active_pids;

    while !remaining.is_empty() && start.elapsed() < grace_period {
        thread::sleep(Duration::from_millis(100));
        remaining.retain(|&pid| {
            if is_process_running(pid) {
                true
            } else {
                log(&format!("Process {pid} exited gracefully"));
                false
            }
        });
    }

    // Send SIGKILL to remaining processes
    if remaining.is_empty() {
        log("All processes exited gracefully");
    } else {
        log(&format!(
            "{} process(es) still running after grace period, sending SIGKILL",
            remaining.len()
        ));

        for &pid in &remaining {
            match send_signal(pid, Signal::Kill) {
                Ok(()) => {
                    log(&format!("Sent SIGKILL to PID {pid}"));
                }
                Err(e) => {
                    log(&format!("Failed to send SIGKILL to PID {pid}: {e}"));
                }
            }
        }
    }
}

fn print_usage(program: &str) {
    eprintln!("Usage: {program} [options] <pid>[,pid...]");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  pid                    Process ID(s) to kill (comma or space separated)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -g, --grace-seconds    Grace period in seconds (default: {DEFAULT_GRACE_SECONDS})");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {program} 1234 5678");
    eprintln!("  {program} -g 10 1234 5678");
    eprintln!("  {program} --grace-seconds 30 1234,5678,9012");
}

fn parse_args(args: &[String]) -> Result<(Vec<u32>, Duration), String> {
    let mut pids = Vec::new();
    let mut grace_seconds = DEFAULT_GRACE_SECONDS;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        
        if arg == "-g" || arg == "--grace-seconds" {
            i += 1;
            if i >= args.len() {
                return Err("Missing value for grace-seconds".to_string());
            }
            grace_seconds = args[i]
                .parse::<u64>()
                .map_err(|_| format!("Invalid grace-seconds value: '{}'", args[i]))?;
        } else if arg.starts_with("--grace-seconds=") {
            let value = arg.strip_prefix("--grace-seconds=").unwrap();
            grace_seconds = value
                .parse::<u64>()
                .map_err(|_| format!("Invalid grace-seconds value: '{value}'"))?;
        } else if arg.starts_with('-') {
            return Err(format!("Unknown option: '{arg}'"));
        } else {
            // Parse PIDs (comma or space separated)
            if arg.contains(',') {
                for pid_str in arg.split(',') {
                    let pid = pid_str
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| format!("Invalid PID: '{pid_str}'"))?;
                    pids.push(pid);
                }
            } else {
                let pid = arg
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid PID: '{arg}'"))?;
                pids.push(pid);
            }
        }
        
        i += 1;
    }

    Ok((pids, Duration::from_secs(grace_seconds)))
}

#[derive(Copy, Clone)]
enum Signal {
    Term,
    Kill,
}

fn send_signal(pid: u32, signal: Signal) -> Result<(), String> {
    let sig_num = match signal {
        Signal::Term => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
    };

    unsafe {
        let result = libc::kill(
            libc::pid_t::try_from(pid).map_err(|_| "PID too large".to_string())?,
            sig_num,
        );
        if result == 0 {
            Ok(())
        } else {
            let err = io::Error::last_os_error();
            match err.raw_os_error() {
                Some(libc::ESRCH) => Err("Process not found".to_string()),
                Some(libc::EPERM) => Err("Permission denied".to_string()),
                _ => Err(format!("Failed to send signal: {err}")),
            }
        }
    }
}

fn is_process_running(pid: u32) -> bool {
    unsafe {
        // Send signal 0 to check if process exists
        libc::kill(libc::pid_t::try_from(pid).unwrap_or(-1), 0) == 0
    }
}

fn log(message: &str) {
    let _ = writeln!(io::stderr(), "[gracekill] {message}");
}
