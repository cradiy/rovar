#![cfg(windows)]

use std::{
    io::Write,
    process::{Command, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::ServerOptions,
};

#[test]
fn gui_executable_supports_captured_cli_output_and_exit_codes() {
    let help = Command::new(env!("CARGO_BIN_EXE_rovar"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("--connect")
    );
    assert!(help.stderr.is_empty());

    let invalid = Command::new(env!("CARGO_BIN_EXE_rovar"))
        .arg("--invalid-option")
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    assert!(!invalid.stderr.is_empty());

    let discovery = Command::new(env!("CARGO_BIN_EXE_rovar"))
        .arg("--list-sessions")
        .output()
        .unwrap();
    assert!(discovery.status.success());
    assert!(
        serde_json::from_slice::<serde_json::Value>(&discovery.stdout)
            .unwrap()
            .is_array()
    );
    assert!(discovery.stderr.is_empty());
}

#[test]
fn gui_executable_preserves_redirected_unicode_input_and_protocol_errors() {
    let endpoint = format!(r"\\.\pipe\rovar-cli-test-{}", uuid::Uuid::new_v4());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut listener = {
        let _entered = runtime.enter();
        ServerOptions::new()
            .first_pipe_instance(true)
            .create(&endpoint)
            .unwrap()
    };
    let server_endpoint = endpoint.clone();
    let worker = std::thread::spawn(move || {
        runtime.block_on(async move {
        tokio::time::timeout(Duration::from_secs(10), async {
            for index in 0..2 {
                listener.connect().await.unwrap();
                let next = ServerOptions::new().create(&server_endpoint).unwrap();
                let server = std::mem::replace(&mut listener, next);
                let mut stream = BufReader::new(server);
                let mut line = String::new();
                stream.read_line(&mut line).await.unwrap();
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["request_id"], format!("测试{index}"));
                let response = if index == 0 {
                    serde_json::json!({"request_id":request["request_id"],"result":{"text":"设计🦀"}})
                } else {
                    serde_json::json!({"request_id":request["request_id"],"error":{"code":"INVALID_ARGUMENT","message":"test error"}})
                };
                let mut bytes = serde_json::to_vec(&response).unwrap();
                bytes.push(b'\n');
                stream.get_mut().write_all(&bytes).await.unwrap();
                let _ = stream.read(&mut [0]).await;
            }
        }).await.unwrap();
    })
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_rovar"))
        .args(["--connect", &endpoint])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for index in 0..2 {
            writeln!(
                input,
                "{}",
                serde_json::json!({"request_id":format!("测试{index}"),"method":"session.describe"})
            )
            .unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(worker.join().is_ok(), "CLI output: {output:?}");
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    let responses: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["result"]["text"], "设计🦀");
    assert_eq!(responses[1]["error"]["code"], "INVALID_ARGUMENT");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("One or more agent requests failed")
    );
}
