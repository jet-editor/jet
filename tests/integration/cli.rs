use std::process::Command;

#[test]
fn help_prints_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_jet"))
        .arg("--help")
        .output()
        .expect("run jet --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--headless"));
    assert!(stdout.contains("--search"));
}

#[test]
fn headless_quit_exits() {
    let output = Command::new(env!("CARGO_BIN_EXE_jet"))
        .arg("--headless")
        .arg("--quit")
        .output()
        .expect("run jet --headless --quit");
    assert!(output.status.success());
}
