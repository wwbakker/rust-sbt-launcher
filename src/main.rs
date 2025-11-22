use std::process::{Command, Child, Stdio};
use std::io::{self, BufRead, BufReader};
use std::thread;
use std::env;
use colored::{Colorize, Color};
use std::sync::mpsc;

/// Starts sbt in the background and returns the child process handle
fn start_sbt_background() -> io::Result<Child> {
    Command::new("sbt")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
}

fn strip_ansi_escapes(input: &str) -> String {
    // Regex: \x1B\[[0-9;?]*[A-Za-z] matches ANSI escape sequences
    let re = regex::Regex::new(r"\x1B.*\x2E").unwrap();
    re.replace_all(input, "").to_string()
}

fn spawn_and_color_sbt_stdout_notify(
    stdout: std::process::ChildStdout,
    color: Color,
    notify: mpsc::Sender<()>,
) {
    use std::sync::Arc;
    let stdout = Arc::new(std::sync::Mutex::new(BufReader::new(stdout)));
    let stdout_clone = Arc::clone(&stdout);
    thread::spawn(move || {
        let mut line = String::new();
        let mut reader = stdout_clone.lock().unwrap();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line).unwrap_or(0);
            if bytes == 0 {
                break;
            }
            let clean = strip_ansi_escapes(&line);
            print!("{}", clean.color(color));
            if clean.contains("started sbt server") {
                let _ = notify.send(());
            }
        }
    });
}

fn set_window_title_to_cwd() {
    if let Ok(cwd) = env::current_dir() {
        if let Some(dir) = cwd.file_name().and_then(|n| n.to_str()) {
            // ANSI escape: ESC ] 0 ; title BEL
            print!("\x1b]0;{} - sbt\x07", dir);
            // Ensure it is flushed to the terminal
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
    }
}

fn main() {
    set_window_title_to_cwd();

    let (tx, rx) = mpsc::channel();
    match start_sbt_background() {
        Ok(mut childbg) => {
            println!("background sbt started with PID: {}", childbg.id());
            if let Some(stdout) = childbg.stdout.take() {
                // Change Color::Green to any Color you want
                spawn_and_color_sbt_stdout_notify(stdout, Color::TrueColor { r: 75, g: 75, b: 75 }, tx);
            }

             // Wait for notification that sbt server has started
            rx.recv().expect("Failed to receive notification from sbt output thread");

            // Collect command line arguments, skipping the first (program name)
            let args: Vec<String> = env::args().skip(1).collect();
            match Command::new("sbt").args(&args).spawn() {
                Ok(mut childfg) => {
                    println!("foreground sbt started with PID: {} and args: {:?}", childfg.id(), args);
                    match childfg.wait() {
                        Ok(status) => {
                            println!("foreground sbt exited with status: {}", status);
                            match childbg.kill() {
                                Ok(_) => println!("background sbt with PID {} killed", childbg.id()),
                                Err(e) => eprintln!("Failed to kill background sbt: {}", e),
                            }
                        },
                        Err(e) => eprintln!("Failed to wait on sbt: {}", e),
                    }
                }
                Err(e) => {
                    eprintln!("Failed to start sbt: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to start sbt: {}", e);
        }
    }

}
