//! A read-only HTTP endpoint for what a running node knows about itself.
//!
//! The graph's store allows one process at a time, so nothing else can ask a
//! serving node which devices it lists or who is connected. This answers
//! `GET /status` (or `/`) with JSON, for a person or a page that wants to
//! see what the node is doing. It says nothing of the graph's content, but it
//! has no authentication: bind it to loopback, or somewhere only a trusted
//! proxy can reach.

use std::fmt::Write as _;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::Peer;

/// How long a client has to send its request line. The endpoint is for debugging,
/// so a client that stalls is dropped rather than held.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// The longest request line read: far more than `GET /status HTTP/1.1`.
const MAX_REQUEST_LINE: usize = 2048;

/// Serves `peer`'s status on `listener` until the returned task is aborted.
/// `device` is this node's own ID.
pub fn spawn(listener: TcpListener, peer: Peer, device: String) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                // A failed accept (out of descriptors, say) is usually
                // transient; don't spin on it.
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            let (peer, device) = (peer.clone(), device.clone());
            tokio::spawn(async move {
                // Nothing to tell a client that has gone away.
                drop(tokio::time::timeout(REQUEST_TIMEOUT, answer(stream, &peer, &device)).await);
            });
        }
    })
}

async fn answer(mut stream: TcpStream, peer: &Peer, device: &str) -> std::io::Result<()> {
    // Only the first line matters, and only as much of it as a request line can
    // be: what follows is never read.
    let mut request = Vec::new();
    BufReader::new((&mut stream).take(MAX_REQUEST_LINE as u64))
        .read_until(b'\n', &mut request)
        .await?;
    let line = String::from_utf8_lossy(&request);
    let line = line.lines().next().unwrap_or_default();
    let (status, body) = respond(line, || {
        // One snapshot, so that the state and the devices it rests on agree.
        let details = peer.sync_details();
        let connected: Vec<String> = details.connected.into_iter().map(|c| c.device).collect();
        let listed: Vec<(String, String)> = peer
            .graph()
            .devices()
            .iter()
            .map(|d| (d.id.clone(), d.seen.to_string()))
            .collect();
        render(device, details.state.as_str(), &connected, &listed)
    });
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

/// The status line and body for a request line. `status` is only called for a
/// request that is answered with it.
fn respond(request_line: &str, status: impl FnOnce() -> String) -> (&'static str, String) {
    let mut parts = request_line.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some("GET"), Some("/" | "/status")) => ("200 OK", status()),
        (Some("GET"), _) => ("404 Not Found", r#"{"error":"not found"}"#.to_owned()),
        _ => (
            "405 Method Not Allowed",
            r#"{"error":"only GET"}"#.to_owned(),
        ),
    }
}

/// The status as JSON: this node's ID, its sync state, the devices connected
/// now, and the devices the graph lists with when each last saved it.
fn render(device: &str, state: &str, connected: &[String], listed: &[(String, String)]) -> String {
    let connected: Vec<String> = connected.iter().map(|id| quote(id)).collect();
    let listed: Vec<String> = listed
        .iter()
        .map(|(id, seen)| format!(r#"{{"id":{},"seen":{}}}"#, quote(id), quote(seen)))
        .collect();
    format!(
        r#"{{"device":{},"state":{},"connected":[{}],"devices":[{}]}}"#,
        quote(device),
        quote(state),
        connected.join(","),
        listed.join(",")
    )
}

/// `text` as a JSON string.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use quiddity_core::Graph;

    use super::*;
    use crate::Allowed;

    /// Sends `request` to a node serving on a loopback port, and returns
    /// everything it answers.
    async fn ask(request: &[u8]) -> String {
        let peer = Peer::new(Graph::new(), Allowed::Only(HashSet::new()), "me");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = spawn(listener, peer, "me".to_owned());
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(request).await.unwrap();
        // Having said all it will, the client waits for the answer.
        stream.shutdown().await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        server.abort();
        response
    }

    #[tokio::test]
    async fn a_listening_node_answers_a_request_with_its_status() {
        let response = ask(b"GET /status HTTP/1.1\r\nHost: x\r\n\r\n").await;
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        assert!(head.contains("Content-Type: application/json"), "{head}");
        assert!(
            head.contains(&format!("Content-Length: {}", body.len())),
            "{head}"
        );
        assert_eq!(
            body,
            r#"{"device":"me","state":"alone","connected":[],"devices":[]}"#
        );
    }

    #[tokio::test]
    async fn anything_else_is_refused() {
        assert!(
            ask(b"GET /graph HTTP/1.1\r\n\r\n")
                .await
                .starts_with("HTTP/1.1 404")
        );
        assert!(
            ask(b"DELETE / HTTP/1.1\r\n\r\n")
                .await
                .starts_with("HTTP/1.1 405")
        );
        // A client that sends nothing, or never ends its line, is answered
        // once it has said all it is going to, or has sent more than a request
        // line can be.
        assert!(ask(b"").await.starts_with("HTTP/1.1 405"));
        let long = vec![b'x'; MAX_REQUEST_LINE * 2];
        assert!(ask(&long).await.starts_with("HTTP/1.1 405"));
    }

    /// The request line is read up to a limit, exactly: a path that starts
    /// after it is never seen, and one that starts within it is.
    #[tokio::test]
    async fn the_request_line_is_read_only_up_to_its_limit() {
        let padded = |spaces: usize| format!("GET {}/status HTTP/1.1\r\n\r\n", " ".repeat(spaces));
        // "GET " and the padding fill the limit, so the path is not read.
        let beyond = padded(MAX_REQUEST_LINE - "GET ".len());
        assert!(ask(beyond.as_bytes()).await.starts_with("HTTP/1.1 404"));
        // Two fewer and the path is cut after `/s`: not a path it serves.
        let straddling = padded(MAX_REQUEST_LINE - "GET ".len() - 2);
        assert!(ask(straddling.as_bytes()).await.starts_with("HTTP/1.1 404"));
        // With room for the whole path, it is.
        let within = padded(MAX_REQUEST_LINE - "GET /status HTTP/1.1\r\n".len());
        assert!(ask(within.as_bytes()).await.starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn quoting_escapes_what_json_requires() {
        assert_eq!(quote(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(quote("a\nb\u{1}"), r#""a\u000ab\u0001""#);
        assert_eq!(quote("é"), "\"é\"");
    }

    #[test]
    fn rendering_lists_each_device() {
        let body = render(
            "me",
            "synced",
            &["a".to_owned(), "b".to_owned()],
            &[("a".to_owned(), "t".to_owned())],
        );
        assert_eq!(
            body,
            r#"{"device":"me","state":"synced","connected":["a","b"],"devices":[{"id":"a","seen":"t"}]}"#
        );
        assert_eq!(
            render("me", "alone", &[], &[]),
            r#"{"device":"me","state":"alone","connected":[],"devices":[]}"#
        );
    }

    #[test]
    fn only_get_of_the_status_is_answered_with_it() {
        let body = || "status".to_owned();
        assert_eq!(
            respond("GET /status HTTP/1.1", body),
            ("200 OK", "status".into())
        );
        assert_eq!(respond("GET / HTTP/1.1", body).0, "200 OK");
        assert_eq!(respond("GET /graph HTTP/1.1", body).0, "404 Not Found");
        assert_eq!(
            respond("POST /status HTTP/1.1", body).0,
            "405 Method Not Allowed"
        );
        assert_eq!(respond("", body).0, "405 Method Not Allowed");
        // The status is not worked out for a request it isn't sent to.
        assert_eq!(
            respond("GET /x HTTP/1.1", || unreachable!()).0,
            "404 Not Found"
        );
    }
}
