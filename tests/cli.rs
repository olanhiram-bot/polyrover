use std::process::Command;

#[test]
fn help_remains_available_without_credentials() {
    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .arg("help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("polyrover read-only Polymarket CLI"));
    assert!(stdout.contains("gamma search"));
    assert!(stdout.contains("clob simulate"));
    assert!(stdout.contains("analytics positions"));
}
