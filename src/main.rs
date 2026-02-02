#![warn(clippy::all, clippy::pedantic, clippy::cargo)]

use std::env;
use std::io::{self, Write};
use std::process;
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{self, Signal as NixSignal};
use nix::unistd::Pid;

const DEFAULT_GRACE_SECONDS: u64 = 25;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        process::exit(1);
    }

    let (pids, grace_period, exit_non_zero_if_sigkill_required) = match parse_args(&args[1..]) {
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
    let active_pids = send_signal_to_all(&pids, Signal::Term);

    if active_pids.is_empty() {
        log("No processes to wait for");
        process::exit(2); // No processes could be signaled
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
        process::exit(0);
    } else {
        log(&format!(
            "{} process(es) still running after grace period, sending SIGKILL",
            remaining.len()
        ));

        // Filter to only processes still running, then send SIGKILL
        let still_running: Vec<u32> = remaining
            .into_iter()
            .filter(|&pid| {
                if is_process_running(pid) {
                    true
                } else {
                    log(&format!("Process {pid} exited before SIGKILL"));
                    false
                }
            })
            .collect();

        send_signal_to_all(&still_running, Signal::Kill);
        if exit_non_zero_if_sigkill_required {
            process::exit(3);
        } else {
            process::exit(0);
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
    eprintln!(
        "  -g, --grace-seconds                    Grace period in seconds (default: {DEFAULT_GRACE_SECONDS})"
    );
    eprintln!("  --exit-non-zero-if-sigkill-required   Exit with code 3 if SIGKILL was required");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {program} 1234 5678");
    eprintln!("  {program} -g 10 1234 5678");
    eprintln!("  {program} --grace-seconds 30 1234,5678,9012");
}

fn parse_and_validate_pid(pid_str: &str) -> Result<u32, String> {
    let pid = pid_str
        .parse::<u32>()
        .map_err(|_| format!("Invalid PID: '{pid_str}'"))?;
    if pid == 0 {
        return Err("PID 0 not allowed (affects process group)".to_string());
    }
    Ok(pid)
}

fn send_signal_to_all(pids: &[u32], signal: Signal) -> Vec<u32> {
    let signal_name = match signal {
        Signal::Term => "SIGTERM",
        Signal::Kill => "SIGKILL",
    };

    let mut successful_pids = Vec::with_capacity(pids.len());
    for &pid in pids {
        match send_signal(pid, signal) {
            Ok(()) => {
                log(&format!("Sent {signal_name} to PID {pid}"));
                successful_pids.push(pid);
            }
            Err(e) => {
                log(&format!("Failed to send {signal_name} to PID {pid}: {e}"));
            }
        }
    }
    successful_pids
}

fn parse_args(args: &[String]) -> Result<(Vec<u32>, Duration, bool), String> {
    let mut pids = Vec::new();
    let mut grace_seconds = DEFAULT_GRACE_SECONDS;
    let mut exit_non_zero_if_sigkill_required = false;
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
        } else if arg == "--exit-non-zero-if-sigkill-required" {
            exit_non_zero_if_sigkill_required = true;
        } else if arg.starts_with('-') {
            return Err(format!("Unknown option: '{arg}'"));
        } else {
            // Parse PIDs (comma or space separated)
            let pid_strings = if arg.contains(',') {
                arg.split(',').map(str::trim).collect::<Vec<_>>()
            } else {
                vec![arg.as_str()]
            };

            for pid_str in pid_strings {
                let pid = parse_and_validate_pid(pid_str)?;
                pids.push(pid);
            }
        }

        i += 1;
    }

    Ok((
        pids,
        Duration::from_secs(grace_seconds),
        exit_non_zero_if_sigkill_required,
    ))
}

#[derive(Copy, Clone)]
enum Signal {
    Term,
    Kill,
}

fn send_signal(pid: u32, signal: Signal) -> Result<(), String> {
    let nix_signal = match signal {
        Signal::Term => NixSignal::SIGTERM,
        Signal::Kill => NixSignal::SIGKILL,
    };

    let nix_pid =
        Pid::from_raw(i32::try_from(pid).map_err(|_| "PID too large for system".to_string())?);

    match signal::kill(nix_pid, nix_signal) {
        Ok(()) => Ok(()),
        Err(nix::errno::Errno::ESRCH) => Err("Process not found".to_string()),
        Err(nix::errno::Errno::EPERM) => Err("Permission denied".to_string()),
        Err(e) => Err(format!("Failed to send signal: {e}")),
    }
}

fn is_process_running(pid: u32) -> bool {
    let Ok(nix_pid) = i32::try_from(pid).map(Pid::from_raw) else {
        return false; // PID too large, can't exist
    };
    // Send signal 0 to check if process exists
    signal::kill(nix_pid, None).is_ok()
}

fn log(message: &str) {
    let _ = writeln!(io::stderr(), "[gracekill] {message}");
}
