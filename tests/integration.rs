use std::process::Command;

#[test]
fn help_mentions_interactive_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_niiterm"))
        .arg("--help")
        .output()
        .expect("help should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--interactive"));
    assert!(stdout.contains("--protocol"));
    assert!(stdout.contains("Examples:"));
    assert!(stdout.contains("WezTerm"));
}
