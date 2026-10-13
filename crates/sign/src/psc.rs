//! Cloud certificates of Brazil's ICP-Brasil trust service providers (PSC: Prestador de Serviço de
//! Confiança): BirdID, VIDaaS, NeoID, SafeID, … all expose the API of DOC-ICP-17.01 (v0), so one
//! client serves them. The holder authorizes in the provider's page (OAuth 2.0 with PKCE), and the
//! provider signs hashes with the key it keeps; the key never leaves it.
//!
//! This module is the protocol only: URLs, request bodies and the parsing of responses. The
//! network is the caller's ([`Transport`]), as for timestamps: this crate never opens a socket.

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

use crate::keys::{DigestAlg, ExternalKey, PrivateKey};
use crate::{Certificate, DigitalId, SignError};

/// The providers whose API base is known, by name. Signing with one needs the app registered with
/// it: the provider issues a client ID and secret to the app (not to the holder, who only signs in
/// on the provider's page), and a build passes them in.
pub const KNOWN: &[(&str, &str)] = &[("Certisign", "https://remoteidcertisign.com.br/api/v0/")];

/// A provider the app is registered with: its API base (the "URI-base", ending in the API
/// version, e.g. `https://psc.example/v0/`) and the client credentials it issued.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Provider {
    /// A name for the user (e.g. "BirdID").
    pub name: String,
    pub base: String,
    pub client_id: String,
    pub client_secret: String,
    /// The redirect URI registered with the provider; a loopback one (`http://127.0.0.1:PORT/…`)
    /// lets PdfCraft receive the authorization code itself.
    pub redirect_uri: String,
}

impl Provider {
    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base.trim_end_matches('/'))
    }
}

/// The network calls the protocol needs; the app implements them over HTTPS.
pub trait Transport: Send + Sync {
    /// GET `url` with a bearer token; the response body.
    fn get(&self, url: &str, bearer: &str) -> Result<String, String>;
    /// POST a form (`application/x-www-form-urlencoded`); the response body.
    fn post_form(&self, url: &str, form: &str) -> Result<String, String>;
    /// POST JSON with a bearer token; the response body.
    fn post_json(&self, url: &str, bearer: &str, json: &str) -> Result<String, String>;
}

/// A PKCE verifier and its S256 challenge (RFC 7636).
#[derive(Clone, Debug)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    /// From 32 random bytes.
    pub fn from_random(random: [u8; 32]) -> Self {
        let verifier = URL_SAFE_NO_PAD.encode(random);
        let challenge = URL_SAFE_NO_PAD.encode(DigestAlg::Sha256.digest(&[verifier.as_bytes()]));
        Self { verifier, challenge }
    }
}

/// `application/x-www-form-urlencoded` encoding of one value.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The page where the holder authorizes signing for `lifetime` seconds (a signature session, so
/// several signatures need one approval). `login_hint` is a CPF or CNPJ to preselect.
pub fn authorize_url(p: &Provider, pkce: &Pkce, state: &str, lifetime: u32, login_hint: Option<&str>) -> String {
    let mut url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&state={}&lifetime={lifetime}&scope=signature_session&code_challenge={}&code_challenge_method=S256",
        p.url("oauth/authorize"),
        encode(&p.client_id),
        encode(&p.redirect_uri),
        encode(state),
        encode(&pkce.challenge),
    );
    if let Some(hint) = login_hint.filter(|h| !h.is_empty()) {
        url.push_str(&format!("&login_hint={}", encode(hint)));
    }
    url
}

/// What the provider sent back to the redirect URI: the code, or why there is none.
pub fn parse_redirect(query: &str, state: &str) -> Result<String, SignError> {
    let mut code = None;
    let mut got_state = None;
    let mut error = None;
    for pair in query.trim_start_matches('?').split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = decode(v);
        match k {
            "code" => code = Some(v),
            "state" => got_state = Some(v),
            "error" => error = Some(v),
            _ => {}
        }
    }
    if let Some(e) = error {
        return Err(SignError::Crypto(if e == "user_denied" {
            "the holder didn't authorize signing".into()
        } else {
            format!("the provider refused: {e}")
        }));
    }
    if got_state.as_deref() != Some(state) {
        return Err(SignError::Crypto("the provider's answer doesn't match this request".into()));
    }
    code.filter(|c| !c.is_empty()).ok_or_else(|| SignError::Crypto("the provider sent no authorization code".into()))
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b, hex) {
            (b'+', _) => out.push(b' '),
            (b'%', Some(v)) => {
                out.push(v);
                i += 2;
            }
            _ => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// An access token and when it ends.
#[derive(Clone, Debug)]
pub struct Session {
    pub provider: Provider,
    pub access_token: String,
    /// Seconds since the Unix epoch.
    pub expires_at: u64,
    /// The holder's CPF or CNPJ, as the provider authorized it.
    pub holder: String,
}

fn json(body: &str) -> Result<serde_json::Value, SignError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| SignError::Crypto(format!("the provider's answer isn't JSON: {e}")))?;
    if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
        let detail = v.get("error_description").and_then(|d| d.as_str()).unwrap_or("");
        return Err(SignError::Crypto(format!("the provider refused: {e} {detail}").trim_end().to_string()));
    }
    Ok(v)
}

/// Exchange the authorization code for an access token. `now` is seconds since the Unix epoch.
pub fn token(t: &dyn Transport, p: &Provider, code: &str, pkce: &Pkce, now: u64) -> Result<Session, SignError> {
    let form = format!(
        "grant_type=authorization_code&client_id={}&client_secret={}&code={}&redirect_uri={}&code_verifier={}",
        encode(&p.client_id),
        encode(&p.client_secret),
        encode(code),
        encode(&p.redirect_uri),
        encode(&pkce.verifier),
    );
    let v = json(&t.post_form(&p.url("oauth/token"), &form).map_err(SignError::Crypto)?)?;
    let access_token = v.get("access_token").and_then(|a| a.as_str()).ok_or_else(|| SignError::Crypto("the provider sent no access token".into()))?;
    let expires_in = v.get("expires_in").and_then(|e| e.as_u64()).unwrap_or(0);
    let holder = match v.get("authorized_identification") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    Ok(Session { provider: p.clone(), access_token: access_token.to_string(), expires_at: now.saturating_add(expires_in), holder })
}

/// The holder's certificates on the provider, with their aliases.
pub fn certificates(t: &dyn Transport, s: &Session) -> Result<Vec<(String, Certificate)>, SignError> {
    let v = json(&t.get(&s.provider.url("oauth/certificate-discovery"), &s.access_token).map_err(SignError::Crypto)?)?;
    if v.get("status").and_then(|s| s.as_str()) == Some("N") {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for c in v.get("certificates").and_then(|c| c.as_array()).into_iter().flatten() {
        let (Some(alias), Some(pem)) = (c.get("alias").and_then(|a| a.as_str()), c.get("certificate").and_then(|c| c.as_str())) else { continue };
        let b64: String = pem.lines().filter(|l| !l.contains("-----")).flat_map(str::chars).filter(|c| !c.is_whitespace()).collect();
        let Ok(der) = STANDARD.decode(b64.as_bytes()) else { continue };
        let Ok(certificate) = Certificate::parse(&der) else { continue };
        // ICP-Brasil signing certificates are RSA, and the provider's RAW signature is used as is.
        if !matches!(certificate.public_key, crate::keys::PublicKey::Rsa { .. }) {
            continue;
        }
        out.push((alias.trim().to_string(), certificate));
    }
    Ok(out)
}

/// The JSON of a signature request for one hash (RAW: the provider returns the bare signature).
pub fn signature_request(alias: &str, alg: DigestAlg, digest: &[u8], label: &str) -> String {
    serde_json::json!({
        "certificate_alias": alias,
        "hashes": [{
            "id": "1",
            "alias": label,
            "hash": STANDARD.encode(digest),
            "hash_algorithm": alg.oid(),
            "signature_format": "RAW",
        }],
    })
    .to_string()
}

/// The bare signature in a signature response.
pub fn parse_signature(body: &str) -> Result<Vec<u8>, SignError> {
    let v = json(body)?;
    let raw = v
        .get("signatures")
        .and_then(|s| s.as_array())
        .and_then(|s| s.first())
        .and_then(|s| s.get("raw_signature"))
        .and_then(|r| r.as_str())
        .ok_or_else(|| SignError::Crypto("the provider sent no signature".into()))?;
    let raw: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    STANDARD.decode(raw.as_bytes()).map_err(|e| SignError::Crypto(format!("the provider's signature isn't base64: {e}")))
}

struct CloudKey {
    session: Session,
    alias: String,
    transport: Arc<dyn Transport>,
}

impl ExternalKey for CloudKey {
    fn sign(&self, alg: DigestAlg, msg: &[u8]) -> Result<Vec<u8>, SignError> {
        let digest = alg.digest(&[msg]);
        let body = signature_request(&self.alias, alg, &digest, "PdfCraft signature");
        let response =
            self.transport.post_json(&self.session.provider.url("oauth/signature"), &self.session.access_token, &body).map_err(SignError::Crypto)?;
        parse_signature(&response)
    }
}

/// The identity that signs with the certificate `alias` of an authorized session.
pub fn digital_id(session: &Session, alias: &str, certificate: Certificate, transport: Arc<dyn Transport>) -> DigitalId {
    let name = format!("{} ({})", certificate.display_name(), session.provider.name);
    let key =
        PrivateKey::external(certificate.public_key.clone(), Arc::new(CloudKey { session: session.clone(), alias: alias.to_string(), transport }));
    DigitalId { key, certificate, chain: Vec::new(), friendly_name: Some(name) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn provider() -> Provider {
        Provider {
            name: "Test PSC".into(),
            base: "https://psc.example/v0/".into(),
            client_id: "app id".into(),
            client_secret: "s3cr&t".into(),
            redirect_uri: "http://127.0.0.1:43821/callback".into(),
        }
    }

    #[test]
    fn pkce_challenge_is_rfc7636_s256() {
        // RFC 7636 appendix B.
        let verifier_bytes = [
            116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212, 37, 77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132,
            141, 121,
        ];
        let pkce = Pkce::from_random(verifier_bytes);
        assert_eq!(pkce.verifier, "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(pkce.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn the_authorize_url_asks_for_a_signature_session_with_pkce() {
        let pkce = Pkce::from_random([7; 32]);
        let url = authorize_url(&provider(), &pkce, "st 1", 3600, Some("00000000001"));
        assert!(url.starts_with("https://psc.example/v0/oauth/authorize?response_type=code&client_id=app%20id&"), "{url}");
        for part in [
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A43821%2Fcallback",
            "state=st%201",
            "lifetime=3600",
            "scope=signature_session",
            "code_challenge_method=S256",
            "login_hint=00000000001",
        ] {
            assert!(url.contains(part), "{part} in {url}");
        }
        assert!(url.contains(&format!("code_challenge={}", pkce.challenge)));
    }

    #[test]
    fn the_redirect_gives_the_code_or_why_not() {
        assert_eq!(parse_redirect("?code=abc%2B1&state=st%201", "st 1").unwrap(), "abc+1");
        assert!(parse_redirect("code=abc&state=other", "st 1").is_err(), "a state that doesn't match");
        let denied = parse_redirect("error=user_denied&state=st", "st").unwrap_err().to_string();
        assert!(denied.contains("didn't authorize"), "{denied}");
        assert!(parse_redirect("state=st", "st").is_err());
    }

    /// A provider that answers like the DOC-ICP-17.01 examples, recording what it was sent.
    struct Fake {
        sent: Mutex<Vec<String>>,
        certificate_pem: String,
    }

    impl Transport for Fake {
        fn get(&self, url: &str, bearer: &str) -> Result<String, String> {
            self.sent.lock().unwrap().push(format!("GET {url} {bearer}"));
            Ok(serde_json::json!({"status": "S", "certificates": [{"alias": "CERTIFICADO TESTE 1:00000000001", "certificate": self.certificate_pem}]}).to_string())
        }
        fn post_form(&self, url: &str, form: &str) -> Result<String, String> {
            self.sent.lock().unwrap().push(format!("POST {url} {form}"));
            Ok(r#"{"access_token":"tok","expires_in":300,"token_type":"Bearer","authorized_identification_type":"CPF","authorized_identification":"00000000001"}"#.into())
        }
        fn post_json(&self, url: &str, bearer: &str, json: &str) -> Result<String, String> {
            self.sent.lock().unwrap().push(format!("POST {url} {bearer} {json}"));
            Ok(r#"{"certificate_alias":"CERTIFICADO TESTE 1:00000000001","signatures":[{"id":"1","raw_signature":"AQID"}]}"#.into())
        }
    }

    #[test]
    fn token_discovery_and_signature_follow_the_api() {
        let pem = include_str!("../tests/data/rsa.crt.pem").to_string();
        let fake = Arc::new(Fake { sent: Mutex::new(Vec::new()), certificate_pem: pem });
        let pkce = Pkce::from_random([9; 32]);
        let session = token(fake.as_ref(), &provider(), "the code", &pkce, 1_000).unwrap();
        assert_eq!((session.access_token.as_str(), session.expires_at, session.holder.as_str()), ("tok", 1_300, "00000000001"));
        let certs = certificates(fake.as_ref(), &session).unwrap();
        assert_eq!(certs.len(), 1);
        assert_eq!(certs[0].0, "CERTIFICADO TESTE 1:00000000001");
        let id = digital_id(&session, &certs[0].0, certs[0].1.clone(), fake.clone());
        assert_eq!(id.key.sign(DigestAlg::Sha256, b"signed attributes").unwrap(), vec![1, 2, 3]);
        let sent = fake.sent.lock().unwrap().join("\n");
        assert!(
            sent.contains(
                "POST https://psc.example/v0/oauth/token grant_type=authorization_code&client_id=app%20id&client_secret=s3cr%26t&code=the%20code"
            ),
            "{sent}"
        );
        assert!(sent.contains(&format!("code_verifier={}", pkce.verifier)));
        assert!(sent.contains("GET https://psc.example/v0/oauth/certificate-discovery tok"));
        assert!(sent.contains(r#""hash_algorithm":"2.16.840.1.101.3.4.2.1""#) && sent.contains(r#""signature_format":"RAW""#), "{sent}");
    }

    #[test]
    fn provider_errors_are_explained() {
        let e = parse_signature(r#"{"error":"invalid_request","error_description":"code missing"}"#).unwrap_err().to_string();
        assert!(e.contains("invalid_request") && e.contains("code missing"), "{e}");
        assert!(parse_signature("not json").is_err());
        assert!(parse_signature(r#"{"signatures":[]}"#).is_err());
    }
}
