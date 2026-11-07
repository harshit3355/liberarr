//! `liberarr process` end to end against an in-process mock provider, then `serve` on the result.
#![allow(clippy::unwrap_used)] // a test: a panic is the failure report
use axum::Router;
use axum::routing::post;
use liberarr_core::exports::{END, START};
use liberarr_core::recordings::{RecordingRepo, RecordingStatus};
use liberarr_core::summaries::SummaryRepo;
use liberarr_core::transcription::TranscriptRepo;
use liberarr_infra::sqlite::Db;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const API_KEY: &str = "0123456789abcdef0123456789abcdef-api";

struct TempHome(PathBuf);
impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Bitwise Ogg CRC (poly 0x04C11DB7, init 0, no reflection).
fn crc(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |mut c, &b| {
        c ^= u32::from(b) << 24;
        for _ in 0..8 {
            c = if c & 0x8000_0000 != 0 {
                (c << 1) ^ 0x04C1_1DB7
            } else {
                c << 1
            };
        }
        c
    })
}

fn page(seq: u32, flags: u8, granule: i64, body: &[u8]) -> Vec<u8> {
    let mut p = b"OggS\0".to_vec();
    p.push(flags);
    p.extend_from_slice(&granule.to_le_bytes());
    p.extend_from_slice(&7u32.to_le_bytes());
    p.extend_from_slice(&seq.to_le_bytes());
    p.extend_from_slice(&[0; 4]);
    p.extend_from_slice(&[1, body.len() as u8]);
    p.extend_from_slice(body);
    let c = crc(&p);
    p[22..26].copy_from_slice(&c.to_le_bytes());
    p
}

/// A tiny synthetic Ogg Opus stream: OpusHead, OpusTags, one audio page.
fn synthetic_ogg() -> Vec<u8> {
    let mut head = b"OpusHead".to_vec();
    head.extend_from_slice(&[1, 1, 0x38, 0x01, 0x80, 0xBB, 0, 0, 0, 0, 0]);
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&4u32.to_le_bytes());
    tags.extend_from_slice(b"test");
    tags.extend_from_slice(&0u32.to_le_bytes());
    [
        page(0, 2, 0, &head),
        page(1, 0, 0, &tags),
        page(2, 4, 48_000, &[0xF8, 0xFF, 0xFE]),
    ]
    .concat()
}

const SUMMARY: &str = r###"{"title":"Point hebdo","summary_md":"## Contexte\nTest.","mindmap":"mindmap\n  root((Point))\n    Budget","actions":[{"action":"Envoyer le budget","owner":"Alice","due":null,"start":1.0}]}"###;

async fn mock_provider() -> String {
    let app = Router::new()
        .route(
            "/v1/audio/transcriptions",
            post(|| async {
                r#"{"language":"french","text":"Bonjour. Hello.","segments":[{"start":0.0,"end":1.0,"text":"Bonjour."},{"start":1.0,"end":2.0,"text":"Hello."}]}"#
            }),
        )
        .route(
            "/v1/chat/completions",
            post(|| async {
                serde_json::json!({"choices":[{"message":{"role":"assistant","content":SUMMARY}}]})
                    .to_string()
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/v1")
}

fn liberarr(home: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_liberarr"));
    // The developer's own LIBERARR_* variables would override the test's .env.
    for (k, _) in std::env::vars().filter(|(k, _)| k.starts_with("LIBERARR_")) {
        c.env_remove(k);
    }
    c.env("LIBERARR_HOME", home);
    c
}

/// Status code of one raw HTTP/1.1 request.
fn http(port: u16, request: &str) -> std::io::Result<u16> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.write_all(request.as_bytes())?;
    let mut reply = String::new();
    s.read_to_string(&mut reply)?;
    Ok(reply
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0))
}

fn patch_action(port: u16, index: usize) -> u16 {
    let body = r#"{"done":true}"#;
    http(
        port,
        &format!(
            "PATCH /api/recordings/1/actions/{index} HTTP/1.1\r\nHost: localhost\r\nX-Api-Key: {API_KEY}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn process_e2e() {
    let home = TempHome(std::env::temp_dir().join(format!(
        "liberarr-e2e-{}-{}",
        std::process::id(),
        jiff::Timestamp::now().as_nanosecond()
    )));
    std::fs::create_dir_all(&home.0).unwrap();
    let base = mock_provider().await;
    std::fs::write(
        home.0.join(".env"),
        format!(
            "LIBERARR_SECRET_KEY=fedcba9876543210fedcba9876543210-secret\n\
             LIBERARR_API_KEY={API_KEY}\n\
             LIBERARR_ADMIN_PASSWORD=pw-for-tests\n\
             LIBERARR_TRANSCRIBE_BASE_URL={base}\n\
             LIBERARR_TRANSCRIBE_API_KEY=sk-test\n\
             LIBERARR_TRANSCRIBE_MODEL=whisper-1\n\
             LIBERARR_LLM_BASE_URL={base}\n\
             LIBERARR_LLM_API_KEY=sk-test\n\
             LIBERARR_LLM_MODEL=gpt-test\n"
        ),
    )
    .unwrap();
    let audio = home.0.join("meeting.ogg");
    std::fs::write(&audio, synthetic_ogg()).unwrap();

    let out = liberarr(&home.0)
        .arg("process")
        .arg(&audio)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let db = Db::open(&home.0.join("liberarr.db")).unwrap();
    let items = db.list_items().await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, RecordingStatus::Done);
    let t = TranscriptRepo::get(&db, items[0].id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(t.segments.len(), 2);
    let s = SummaryRepo::get(&db, items[0].id).await.unwrap().unwrap();
    assert_eq!(s.actions.len(), 1);
    let note = std::fs::read_dir(home.0.join("obsidian"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "md"))
        .expect("an Obsidian note");
    let note = std::fs::read_to_string(note).unwrap();
    assert!(note.contains(START) && note.contains(END) && note.contains("```mermaid"));
    // The original audio is left where it was.
    assert!(audio.exists());

    // `serve` on the same home: the action toggle goes through the real backend.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut server = liberarr(&home.0)
        .args(["serve", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let health = "GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let up = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        http(port, health).is_ok_and(|c| c == 200)
    });
    let (in_range, out_of_range) = (patch_action(port, 0), patch_action(port, 5));
    server.kill().unwrap();
    let _ = server.wait();
    assert!(up, "serve did not come up");
    assert_eq!(in_range, 200);
    assert_eq!(out_of_range, 404);
}
