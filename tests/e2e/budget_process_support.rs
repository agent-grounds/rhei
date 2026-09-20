/// A lock regression must fail finitely, including when the child never emits
/// output. File-backed output avoids blocking on inherited pipe descriptors.
/// §FS-rhei-budgets.12 §FS-rhei-run.3.2
pub fn bounded_budget_output(command: &mut std::process::Command) -> std::process::Output {
    use std::process::Stdio;
    let output_dir = unique_temp_dir("budget-command-output");
    let stdout_path = output_dir.join("stdout");
    let stderr_path = output_dir.join("stderr");
    command
        .stdout(Stdio::from(fs::File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(fs::File::create(&stderr_path).unwrap()));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().expect("start bounded budget command");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll bounded command") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            #[cfg(unix)]
            {
                use nix::{
                    sys::signal::{killpg, Signal},
                    unistd::Pid,
                };
                let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &child.id().to_string()])
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "budget command exceeded 15 seconds; terminated fixture tree\n{}\n{}",
                fs::read_to_string(&stdout_path).unwrap_or_default(),
                fs::read_to_string(&stderr_path).unwrap_or_default()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    std::process::Output {
        status,
        stdout: fs::read(stdout_path).unwrap(),
        stderr: fs::read(stderr_path).unwrap(),
    }
}
