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
    assert!(stdout.contains("--snapshot"));
    assert!(stdout.contains("--layout"));
    assert!(stdout.contains("25%"));
    assert!(stdout.contains("sagittal/sag/x"));
    assert!(stdout.contains("Examples:"));
    assert!(stdout.contains("WezTerm"));
}

#[test]
fn snapshot_and_interactive_conflict_cleanly() {
    let output = Command::new(env!("CARGO_BIN_EXE_niiterm"))
        .args(["--snapshot", "mid3", "--interactive", "fake.nii.gz"])
        .output()
        .expect("conflict should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--interactive"));
    assert!(stderr.contains("--snapshot"));
}
