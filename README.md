gracekill(1)
============

NAME
----
gracekill - send signals to processes with grace period

SYNOPSIS
--------
gracekill [options] <pid>[,pid...]

DESCRIPTION
-----------
gracekill sends SIGTERM to the specified process(es) and waits for a configurable
grace period before sending SIGKILL to any remaining processes.

The grace period defaults to 30 seconds if not specified.

Multiple PIDs may be specified either space-separated or comma-separated.

ARGUMENTS
---------
pid
    Process ID to signal. Multiple PIDs can be specified.

OPTIONS
-------
-g, --grace-seconds
    Time in seconds to wait between SIGTERM and SIGKILL. Default: 30

EXAMPLES
--------
Send SIGTERM to processes 1234 and 5678, wait 10 seconds before SIGKILL:

    gracekill -g 10 1234 5678

Send signals to comma-separated PIDs with 30 second grace period:

    gracekill --grace-seconds 30 1234,5678,9012

Use default 30 second grace period:

    gracekill 1234

Alternative syntax with equals sign:

    gracekill --grace-seconds=15 1234

SIGNALS
-------
The program sends the following signals in order:

1. SIGTERM - Allows the process to perform cleanup before exiting
2. SIGKILL - Forces immediate termination (only if process still running)

During the grace period, processes are checked every 100ms.

EXIT STATUS
-----------
0
    Success

1
    Invalid arguments or usage error

DIAGNOSTICS
-----------
All diagnostic output is written to stderr with "[gracekill]" prefix.

Error messages include:
- Process not found
- Permission denied
- Invalid PID

SEE ALSO
--------
[kill(1)](https://man7.org/linux/man-pages/man1/kill.1.html), [signal(7)](https://man7.org/linux/man-pages/man7/signal.7.html)

NOTES
-----
Requires appropriate permissions to signal target processes.
Only available on Unix-like systems.

BUILD
-----
cargo build --release

Binary will be at target/release/gracekill