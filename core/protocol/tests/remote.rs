//! What the deployment says to the apps between releases (api/remote.rs),
//! asked of a small server started in each test that answers
//! `GET /api/apps/remote` as internal/app/remote.go does, or does not.
//!
//! Guarded: the request is the one the deployment answers, from an app (its
//! user agent, which the deployment resolves the rules by); an answer is read
//! and kept whole, so that what an app reads back offline is what it was told;
//! and a refusal or an answer that is not one is an error, which the app
//! meets by going on with what it kept rather than with nothing.

use std::sync::{Arc, Mutex};

use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tomoshibi_protocol::{
    api::{parse_remote_config, remote_notice_text, Deployment},
    ProtocolError,
};

/// Answers every request with `status` and `body`, and remembers each
/// request's first line and user agent.
async fn answering(status: u16, body: String) -> (String, Arc<Mutex<Vec<(String, String)>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = Vec::new();
            let mut chunk = [0u8; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match socket.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                }
            }
            let text = String::from_utf8_lossy(&request).to_string();
            let line = text.lines().next().unwrap_or_default().to_owned();
            let agent = text
                .lines()
                .find_map(|l| {
                    l.strip_prefix("user-agent: ")
                        .or_else(|| l.strip_prefix("User-Agent: "))
                })
                .unwrap_or_default()
                .to_owned();
            kept.lock().unwrap().push((line, agent));
            let answer = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(answer.as_bytes()).await;
        }
    });
    (base, seen)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_deployment_is_asked_as_the_app_and_its_answer_kept_whole() {
    let body = json!({
        "revision": "0123456789abcdef",
        "phrases": { "Sign out": { "zh-Hans": "退出登录" } },
        "off": ["screen-share"],
        "params": { "screen_share_max_fps": 30 },
        "notice": { "id": "n1", "level": "info", "text": { "zh-Hans": "今晚维护", "en": "Down tonight" }, "until": "2099-01-01T00:00:00Z" },
    })
    .to_string();
    let (base, seen) = answering(200, body.clone()).await;

    let config = Deployment::new(base)
        .unwrap()
        .remote_config()
        .await
        .unwrap();
    assert_eq!(config.revision, "0123456789abcdef");
    assert_eq!(config.off, ["screen-share"]);
    assert_eq!(config.params["screen_share_max_fps"], 30.0);
    assert_eq!(
        remote_notice_text(config.notice.clone().unwrap(), "zh-Hans".into()).as_deref(),
        Some("今晚维护")
    );

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].0.starts_with("GET /api/apps/remote "),
        "asked {:?}",
        seen[0].0
    );
    assert!(
        seen[0].1.starts_with("Tomoshibi/"),
        "asked as {:?}, which the deployment refuses as a browser",
        seen[0].1
    );

    assert_eq!(config.kept, body, "kept as it came");
    assert_eq!(
        parse_remote_config(config.kept.clone()),
        Some(config),
        "read back the same offline"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_or_an_answer_that_is_not_one_is_an_error() {
    let (base, _) = answering(403, json!({ "error": "not_an_app" }).to_string()).await;
    assert!(Deployment::new(base)
        .unwrap()
        .remote_config()
        .await
        .is_err());

    let (base, _) = answering(200, "<html>a captive portal</html>".to_owned()).await;
    assert!(matches!(
        Deployment::new(base).unwrap().remote_config().await,
        Err(ProtocolError::Unexpected { .. })
    ));

    let nobody = Deployment::new("http://127.0.0.1:9".to_owned()).unwrap();
    assert!(matches!(
        nobody.remote_config().await,
        Err(ProtocolError::Network { .. })
    ));
}
