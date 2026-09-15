use clap::{Args, Subcommand};
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command};
use unlinger_daemon::{IpcClient, IpcCommand, IpcPayload, SessionOwnerLease, TaskLease};

#[derive(Debug, Args)]
pub struct TaskArgs {
    #[command(subcommand)]
    command: TaskCommand,
}

/// Host integration for an existing ordinary Playwright CLI session name.
///
/// The host command is the exact owner. This path never launches a browser on
/// its own and never sends a signal. Absence leaves the same session protected.
#[derive(Debug, Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    command: SessionCommand,
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    /// Run a host command while retaining its ordinary Playwright session name.
    Run(SessionRunArgs),
    /// Show the durable session-owner phase without changing anything.
    Status(StatusArgs),
}

#[derive(Debug, Args)]
pub struct SessionRunArgs {
    /// The ordinary Playwright session name to retain.
    #[arg(long)]
    session: String,
    /// Playwright's 16-hex registry namespace directory name.
    #[arg(long)]
    registry_namespace: String,
    /// Exact observed `playwright-core` version the host will use.
    #[arg(long)]
    controller_version: String,
    #[command(flatten)]
    exec: ExecArgs,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    lease_id: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum TaskCommand {
    /// Inherit normal input/output and return the command's exit code.
    Run(ExecArgs),
    /// Show the durable task phase and its exact browser incident identifiers.
    Status {
        task_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
pub struct ExecArgs {
    /// Command and literal arguments, after --. No shell interpretation is added.
    #[arg(last = true, required = true, num_args = 1..)]
    command: Vec<OsString>,
}

#[derive(Debug, Args)]
pub struct GatedExecArgs {
    #[arg(long, value_parser = clap::value_parser!(i32).range(0..))]
    gate_fd: i32,
    #[command(flatten)]
    exec: ExecArgs,
}

#[derive(Debug)]
pub struct CommandExit(pub u8);

impl fmt::Display for CommandExit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "task command exited with {}", self.0)
    }
}
impl Error for CommandExit {}

pub fn run(socket: &Path, arguments: TaskArgs) -> Result<(), Box<dyn Error>> {
    let client = IpcClient::new(socket);
    match arguments.command {
        TaskCommand::Run(arguments) => run_command(&client, arguments),
        TaskCommand::Status { task_id, json } => {
            let IpcPayload::TaskStatus(status) =
                client.request(IpcCommand::TaskStatus { task_id })?
            else {
                return Err("unexpected task status response".into());
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!("Task {}: {:?}", status.task_id, status.phase);
                println!("Playwright session: {}", status.session_name);
                println!(
                    "Released means command ownership ended; cleanup is reported by each incident."
                );
                for incident in status.incident_ids {
                    println!("  unlinger explain {incident}");
                }
            }
            Ok(())
        }
    }
}

fn run_command(client: &IpcClient, arguments: ExecArgs) -> Result<(), Box<dyn Error>> {
    let mut random = [0_u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let task_id: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    run_owned_command(client, arguments, &task_id)
}

/// Run or read one host-declared session owner. The command retains the supplied
/// ordinary Playwright selector; this surface never launches a browser by
/// itself and never sends a signal.
pub fn run_session(socket: &Path, arguments: SessionArgs) -> Result<(), Box<dyn Error>> {
    let client = IpcClient::new(socket);
    match arguments.command {
        SessionCommand::Run(arguments) => run_owned_session_command(&client, arguments),
        SessionCommand::Status(arguments) => {
            let IpcPayload::SessionOwnerStatus(status) =
                client.request(IpcCommand::SessionOwnerStatus {
                    lease_id: arguments.lease_id,
                })?
            else {
                return Err("unexpected session-owner status response".into());
            };
            if arguments.json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!("Session {}: {:?}", status.session_name, status.phase);
                println!(
                    "Selector {} (generation {})",
                    status.selector_fingerprint, status.generation
                );
                println!("Controller version: {}", status.controller_version);
                println!("Controller bound: {}", status.controller_bound);
                for incident in status.incident_ids {
                    println!("  unlinger explain {incident}");
                }
            }
            Ok(())
        }
    }
}

fn run_owned_session_command(
    client: &IpcClient,
    arguments: SessionRunArgs,
) -> Result<(), Box<dyn Error>> {
    let IpcPayload::SessionOwnerLease(lease) = client.request(IpcCommand::SessionOwnerDeclare {
        session_name: arguments.session,
        registry_namespace: arguments.registry_namespace,
        controller_version: arguments.controller_version,
    })?
    else {
        return Err("unexpected session-owner declaration response".into());
    };
    eprintln!(
        "Unlinger session owner: {} (generation {}, lease {})",
        lease.session_name, lease.generation, lease.lease_id
    );
    let (mut child, mut gate) = spawn_gated(&arguments.exec, &lease.session_name)?;
    let activated = client.request(IpcCommand::SessionOwnerActivate {
        lease_id: lease.lease_id.clone(),
        capability: lease.capability.clone(),
        owner_pid: child.id(),
    });
    if !matches!(activated, Ok(IpcPayload::SessionOwnerStatus(_))) {
        drop(gate);
        child.wait()?;
        return Err(match activated {
            Err(error) => error.into(),
            Ok(_) => "unexpected session-owner activation response".into(),
        });
    }
    gate.write_all(&[1])?;
    drop(gate);
    let status = child.wait()?;
    finish_session(client, lease);
    let code = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1));
    if code == 0 {
        Ok(())
    } else {
        Err(Box::new(CommandExit(code as u8)))
    }
}

fn finish_session(client: &IpcClient, lease: SessionOwnerLease) {
    match client.request(IpcCommand::SessionOwnerRelease {
        lease_id: lease.lease_id.clone(),
        capability: lease.capability,
    }) {
        Ok(IpcPayload::SessionOwnerStatus(_)) => eprintln!(
            "Unlinger session {} released. Eligible leftovers still require cooling and every cleanup gate.",
            lease.session_name
        ),
        Ok(_) => eprintln!(
            "Unlinger: unexpected session-owner release response; the daemon will independently check exact owner exit."
        ),
        Err(error) => eprintln!(
            "Unlinger: session-owner release was not confirmed ({error}); the daemon will independently check exact owner exit."
        ),
    }
}

fn run_owned_command(
    client: &IpcClient,
    arguments: ExecArgs,
    task_id: &str,
) -> Result<(), Box<dyn Error>> {
    let IpcPayload::TaskLease(lease) = client.request(IpcCommand::TaskReserve {
        task_id: task_id.to_owned(),
    })?
    else {
        return Err("unexpected task registration response".into());
    };
    eprintln!(
        "Unlinger task: {} (session {})",
        lease.task_id, lease.session_name
    );
    let (mut child, mut gate) = spawn_gated(&arguments, &lease.session_name)?;
    let activated = client.request(IpcCommand::TaskActivate {
        task_id: lease.task_id.clone(),
        capability: lease.capability.clone(),
        owner_pid: child.id(),
    });
    if !matches!(activated, Ok(IpcPayload::TaskStatus(_))) {
        // EOF prevents the command from executing, including when activation's
        // delivery was uncertain. No timed-out mutation is sent a second time.
        drop(gate);
        child.wait()?;
        return Err(match activated {
            Err(error) => error.into(),
            Ok(_) => "unexpected task activation response".into(),
        });
    }
    gate.write_all(&[1])?;
    drop(gate);
    let status = child.wait()?;
    finish(client, lease);
    let code = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1));
    if code == 0 {
        Ok(())
    } else {
        Err(Box::new(CommandExit(code as u8)))
    }
}

fn finish(client: &IpcClient, lease: TaskLease) {
    match client.request(IpcCommand::TaskFinish {
        task_id: lease.task_id.clone(),
        capability: lease.capability,
    }) {
        Ok(IpcPayload::TaskStatus(_)) => eprintln!(
            "Unlinger task {} released. Eligible leftovers still require cooling and verified cleanup; check task status or the App.",
            lease.task_id
        ),
        Ok(_) => eprintln!(
            "Unlinger: unexpected task finish response; the daemon will independently check command-owner exit."
        ),
        Err(error) => eprintln!(
            "Unlinger: task finish was not confirmed ({error}); the daemon will independently check command-owner exit."
        ),
    }
}

fn spawn_gated(arguments: &ExecArgs, session: &str) -> Result<(Child, File), Box<dyn Error>> {
    let mut descriptors = [-1; 2];
    if unsafe { libc::pipe(descriptors.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let read = unsafe { File::from_raw_fd(descriptors[0]) };
    let write = unsafe { File::from_raw_fd(descriptors[1]) };
    for descriptor in descriptors {
        if unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    let read_fd = read.as_raw_fd();
    let write_fd = write.as_raw_fd();
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("__task-exec")
        .arg("--gate-fd")
        .arg(read_fd.to_string())
        .arg("--")
        .args(&arguments.command)
        .env("PLAYWRIGHT_CLI_SESSION", session);
    unsafe {
        command.pre_exec(move || {
            // Keep the newly allocated descriptor: a caller may already own FD 3.
            if libc::fcntl(read_fd, libc::F_SETFD, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            libc::close(write_fd);
            Ok(())
        });
    }
    let child = command.spawn()?;
    drop(read);
    Ok((child, write))
}

pub fn exec(arguments: GatedExecArgs) -> Result<(), Box<dyn Error>> {
    if unsafe { libc::fcntl(arguments.gate_fd, libc::F_GETFD) } == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut gate = unsafe { File::from_raw_fd(arguments.gate_fd) };
    let mut permission = [0];
    gate.read_exact(&mut permission)?;
    drop(gate);
    if permission != [1] {
        return Err("task launch was not activated".into());
    }
    Err(Command::new(&arguments.exec.command[0])
        .args(&arguments.exec.command[1..])
        .exec()
        .into())
}
