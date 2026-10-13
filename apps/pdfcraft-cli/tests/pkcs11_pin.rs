//! A PKCS #11 PIN must never be accepted from a script file or a `key=value` argument — only
//! from a terminal prompt. PKCS #12, Keychain and Windows store signing are unaffected.

use std::process::{Command, Output, Stdio};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pdfcraft-cli"))
}

/// A script's `pin` for a `pkcs11:` identity is refused before the tool even runs — there is no
/// such token, but the rejection has to happen regardless of what's actually on the system.
#[test]
fn a_pin_in_a_script_is_refused() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-cli-pkcs11-pin-script-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("sign.json");
    let steps = serde_json::json!([
        { "tool": "sign_document", "args": { "doc": 1, "id": "pkcs11:0:01", "pin": "123456", "out": "signed.pdf" } },
    ]);
    std::fs::write(&script, steps.to_string()).unwrap();
    let out: Output = cli().current_dir(&dir).arg("run").arg("--script").arg(&script).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("can't be given as a script or key=value argument"), "{stderr}");
}

/// Same refusal through the one-shot `run <tool> key=value` form.
#[test]
fn a_pin_in_a_one_shot_argument_is_refused() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-cli-pkcs11-pin-oneshot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out: Output =
        cli().current_dir(&dir).args(["run", "sign_document", "doc=1", "id=pkcs11:0:01", "pin=123456", "out=signed.pdf"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("can't be given as a script or key=value argument"), "{stderr}");
}

/// Without a `pin`, the CLI prompts on the terminal instead of silently using something else.
/// This process has no controlling terminal (its stdin is piped and closed), so the prompt fails
/// fast — proof it never falls back to argv/env, and never hangs waiting on a TTY that isn't
/// there.
#[test]
fn without_a_pin_it_tries_to_prompt_and_fails_fast_with_no_terminal() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-cli-pkcs11-pin-prompt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out: Output =
        cli().current_dir(&dir).args(["run", "sign_document", "doc=1", "id=pkcs11:0:01", "out=signed.pdf"]).stdin(Stdio::null()).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(!stderr.contains("can't be given as"), "{stderr}");
    assert!(stderr.contains("couldn't read the PIN"), "{stderr}");
}

/// PKCS #12 and Keychain signing keep taking their credential as a normal argument.
#[test]
fn non_pkcs11_ids_are_not_affected() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-cli-pkcs11-pin-other-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out: Output =
        cli().current_dir(&dir).args(["run", "sign_document", "doc=1", "id=no-such-file.p12", "password=secret", "out=signed.pdf"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Fails for an unrelated reason (the file doesn't exist, no document 1 either), never the
    // PKCS #11 PIN rejection.
    assert!(!out.status.success(), "{stderr}");
    assert!(!stderr.contains("can't be given as"), "{stderr}");
    assert!(!stderr.contains("couldn't read the PIN"), "{stderr}");
}
