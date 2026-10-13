//! ICP-Brasil cloud certificates (Configure a Digital ID ▸ Use a cloud certificate): the holder
//! authorizes in the provider's page, the provider redirects to a loopback address PdfCraft
//! listens on, and the session's certificates become digital IDs for the rest of the run.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use pdfcraft_engine::sign::Certificate;
use pdfcraft_engine::sign::psc::{self, Pkce, Provider, Session, Transport};

/// A finished sign-in: the session and its certificates (alias, certificate).
pub type SignedIn = (Session, Vec<(String, Certificate)>);

/// A sign-in waiting for the browser.
pub struct Pending {
    pub provider: String,
    pub result: Receiver<Result<SignedIn, String>>,
}

/// How long the browser has to come back.
const WAIT: Duration = Duration::from_secs(300);

/// The loopback address of a redirect URI (`http://127.0.0.1:PORT/…` or `http://localhost:PORT/…`).
pub fn loopback(redirect_uri: &str) -> Option<SocketAddr> {
    let rest = redirect_uri.strip_prefix("http://")?;
    let host_port = rest.split('/').next()?;
    let (host, port) = host_port.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    matches!(host, "127.0.0.1" | "localhost").then(|| SocketAddr::from(([127, 0, 0, 1], port)))
}

/// Start signing in to `provider`: listen on its redirect address, return the page to open, and
/// finish (code → token → certificates) on a thread. `notify` wakes the UI when it's done.
pub fn start(provider: Provider, transport: Arc<dyn Transport>, notify: impl Fn() + Send + 'static) -> Result<(String, Pending), String> {
    let addr = loopback(&provider.redirect_uri)
        .ok_or_else(|| tl!("The redirect URI must be http://127.0.0.1 or http://localhost with a port.").to_string())?;
    let listener = TcpListener::bind(addr).map_err(|e| format!("{addr}: {e}"))?;
    let mut random = [0u8; 64];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let mut verifier = [0u8; 32];
    verifier.copy_from_slice(&random[..32]);
    let pkce = Pkce::from_random(verifier);
    let state: String = random[32..48].iter().map(|b| format!("{b:02x}")).collect();
    let url = psc::authorize_url(&provider, &pkce, &state, 3600, None);
    let (tx, rx) = std::sync::mpsc::channel();
    let name = provider.name.clone();
    std::thread::spawn(move || {
        let result = (|| {
            let query = wait_for_redirect(&listener, WAIT)?;
            let code = psc::parse_redirect(&query, &state).map_err(|e| e.to_string())?;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let session = psc::token(transport.as_ref(), &provider, &code, &pkce, now).map_err(|e| e.to_string())?;
            let certificates = psc::certificates(transport.as_ref(), &session).map_err(|e| e.to_string())?;
            if certificates.is_empty() {
                return Err(tl!("No certificate was found with this provider.").to_string());
            }
            Ok((session, certificates))
        })();
        let _ = tx.send(result);
        notify();
    });
    Ok((url, Pending { provider: name, result: rx }))
}

/// The query of the first request to the redirect address, answered with a page that sends the
/// user back to PdfCraft.
fn wait_for_redirect(listener: &TcpListener, wait: Duration) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + wait;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let target = request.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("");
                let Some((_, query)) = target.split_once('?') else {
                    // A favicon or a stray request: keep waiting for the redirect.
                    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                };
                let page = "<!doctype html><meta charset=utf-8><title>PdfCraft</title><p style=\"font:16px system-ui;margin:3em\">PdfCraft: you can close this page.</p>";
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                );
                return Ok(query.to_string());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(tl!("The provider's page didn't come back in time. Try again.").to_string());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_redirects_with_a_port_are_accepted() {
        assert_eq!(loopback("http://127.0.0.1:43821/callback"), Some(SocketAddr::from(([127, 0, 0, 1], 43821))));
        assert_eq!(loopback("http://localhost:8080"), Some(SocketAddr::from(([127, 0, 0, 1], 8080))));
        assert_eq!(loopback("https://127.0.0.1:43821/callback"), None, "the listener speaks plain HTTP");
        assert_eq!(loopback("http://example.com:43821/callback"), None);
        assert_eq!(loopback("http://127.0.0.1/callback"), None, "a port is needed");
    }

    #[test]
    fn the_redirect_query_is_read_and_answered() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut s = std::net::TcpStream::connect(addr).unwrap();
            s.write_all(b"GET /favicon.ico HTTP/1.1\r\n\r\n").unwrap();
            let mut s = std::net::TcpStream::connect(addr).unwrap();
            s.write_all(b"GET /callback?code=abc&state=xyz HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut answer = String::new();
            let _ = s.read_to_string(&mut answer);
            answer
        });
        let query = wait_for_redirect(&listener, Duration::from_secs(5)).unwrap();
        assert_eq!(query, "code=abc&state=xyz");
        assert!(client.join().unwrap().starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn nobody_coming_back_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(wait_for_redirect(&listener, Duration::from_millis(300)).is_err());
    }
}
