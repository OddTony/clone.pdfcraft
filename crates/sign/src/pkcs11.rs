//! PKCS #11 tokens and smart cards (plan: `sign.pkcs11`).
//!
//! The first step is deliberately small and has no hardware in it: the reference a token
//! identity is kept under (`pkcs11:<SHA-256 of the certificate DER>`, like the
//! `windows:` and `keychain:` references) and the `DigestInfo` a `CKM_RSA_PKCS` signature
//! is computed over. Module loading (explicit user-chosen paths only), slot listing and
//! signing through `cryptoki` come later; nothing here opens a DLL.

use crate::keys::DigestAlg;
#[cfg(not(target_arch = "wasm32"))]
use crate::keys::{ExternalKey, PrivateKey, PublicKey};
use crate::{Certificate, DigitalId, SignError};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

/// The `pkcs11:<SHA-256 of the certificate DER>` reference kept for an identity.
pub fn reference(certificate: &Certificate) -> String {
    let digest = DigestAlg::Sha256.digest(&[&certificate.raw]);
    let mut out = String::with_capacity("pkcs11:".len() + digest.len() * 2);
    out.push_str("pkcs11:");
    for b in digest {
        out.push(hex(b >> 4));
        out.push(hex(b & 0x0F));
    }
    out
}

fn hex(nibble: u8) -> char {
    (b'0' + nibble + 7 * (nibble / 10)) as char
}

/// The fingerprint a `pkcs11:` reference names, as 32 bytes.
pub fn parse_reference(reference_or_name: &str) -> Result<[u8; 32], SignError> {
    let hex = reference_or_name
        .strip_prefix("pkcs11:")
        .ok_or_else(|| SignError::Malformed("a PKCS #11 identity reference looks like pkcs11:<64 hex chars of the certificate SHA-256>".into()))?;
    if hex.len() != 64 {
        return Err(SignError::Malformed(format!("a PKCS #11 identity reference holds 64 hex chars, not {}", hex.len())));
    }
    let mut out = [0u8; 32];
    let bytes = hex.as_bytes();
    for (i, slot) in out.iter_mut().enumerate() {
        let Some(pair) = bytes.get(i * 2..i * 2 + 2) else {
            return Err(SignError::Malformed("a PKCS #11 identity reference holds hex chars".into()));
        };
        *slot = (nibble(pair[0]).ok_or_else(|| SignError::Malformed("a PKCS #11 identity reference holds hex chars".into()))? << 4)
            | nibble(pair[1]).ok_or_else(|| SignError::Malformed("a PKCS #11 identity reference holds hex chars".into()))?;
    }
    Ok(out)
}

fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// The `DigestInfo` (RFC 8017 §9.2) a `CKM_RSA_PKCS` signature is computed over: `SEQUENCE {
/// AlgorithmIdentifier (with the NULL parameter), OCTET STRING digest }`.
///
/// Unlike CNG — which adds this itself when given PKCS #1 padding — a PKCS #11 token signs
/// exactly these bytes, so the caller builds them first. New signatures use SHA-256/384/512
/// only, like [`crate::keys::PrivateKey::sign`].
pub fn digest_info(alg: DigestAlg, digest: &[u8]) -> Result<Vec<u8>, SignError> {
    if !matches!(alg, DigestAlg::Sha256 | DigestAlg::Sha384 | DigestAlg::Sha512) {
        return Err(SignError::Unsupported(format!("{} for new signatures", alg.name())));
    }
    if digest.len() != alg.output_len() {
        return Err(SignError::Malformed(format!("a {} digest holds {} bytes, not {}", alg.name(), alg.output_len(), digest.len())));
    }
    let id = crate::der::algorithm(alg.oid(), Some(&crate::der::null()));
    Ok(crate::der::seq(&[&id, &crate::der::octets(digest)]))
}

// ── modules and tokens (native targets only; the browser can't load a DLL) ──────────────

/// A token in a slot: what the settings UI lists and what a `pkcs11:` reference resolves
/// against. Plain data, so the UI can show it without touching the module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// The token's label, without the space padding PKCS #11 strings carry.
    pub label: String,
    /// The device serial number, unpadded.
    pub serial: String,
    /// The device manufacturer, unpadded.
    pub manufacturer: String,
    /// Whether signing needs a PIN entered: login is required and the token has no pinpad
    /// reading it itself.
    pub needs_pin: bool,
    /// Whether the token reads the PIN itself (`PROTECTED_AUTHENTICATION_PATH`): login
    /// passes no PIN.
    pub pinpad: bool,
    /// Whether the user PIN is locked.
    pub pin_locked: bool,
}

/// A loaded PKCS #11 module. Lives as long as the app (never finalized); the path is the
/// one the user chose, kept for error messages.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub struct Module {
    inner: cryptoki::context::Pkcs11,
    path: PathBuf,
}

/// Load the module at `path` — a DLL the user picked explicitly. Modules are never searched
/// for or loaded on their own: a DLL executes code.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_module(path: &Path) -> Result<Module, SignError> {
    let inner = cryptoki::context::Pkcs11::new(path)
        .map_err(|e| SignError::Crypto(format!("the PKCS #11 module at {} couldn't be loaded: {}", path.display(), describe(&e))))?;
    inner
        .initialize(cryptoki::context::CInitializeArgs::new(cryptoki::context::CInitializeFlags::OS_LOCKING_OK))
        .map_err(|e| SignError::Crypto(format!("the PKCS #11 module at {} couldn't be started: {}", path.display(), describe(&e))))?;
    Ok(Module { inner, path: path.to_path_buf() })
}

/// Every slot holding a token: its label, serial number, manufacturer, and whether a PIN
/// is needed or read by a pinpad.
#[cfg(not(target_arch = "wasm32"))]
pub fn tokens(module: &Module) -> Result<Vec<Token>, SignError> {
    let slots = module
        .inner
        .get_slots_with_token()
        .map_err(|e| SignError::Crypto(format!("the PKCS #11 module at {} couldn't list its slots: {}", module.path.display(), describe(&e))))?;
    let mut out = Vec::new();
    for slot in slots {
        let info = module.inner.get_token_info(slot).map_err(|e| {
            SignError::Crypto(format!(
                "a token in the PKCS #11 module at {} couldn't be read (was it removed?): {}",
                module.path.display(),
                describe(&e)
            ))
        })?;
        let pinpad = info.protected_authentication_path();
        out.push(Token {
            label: info.label().trim_end().to_string(),
            serial: info.serial_number().trim_end().to_string(),
            manufacturer: info.manufacturer_id().trim_end().to_string(),
            needs_pin: info.login_required() && !pinpad,
            pinpad,
            pin_locked: info.user_pin_locked(),
        });
    }
    Ok(out)
}

/// Where the signing PIN comes from. Asked once per signature; the PIN lives only in that
/// call and is never stored.
#[derive(Clone)]
pub enum PinSource {
    /// The token needs no PIN, or reads it itself (pinpad).
    None,
    /// Read the PIN from this environment variable at signing time, so it never lands in
    /// logs or scripts.
    Env(String),
    /// Ask through this callback (the UI's PIN dialog) at signing time.
    Ask(std::sync::Arc<dyn Fn() -> Result<secrecy::SecretString, SignError> + Send + Sync>),
}

impl std::fmt::Debug for PinSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PinSource::None => write!(f, "None"),
            PinSource::Env(name) => write!(f, "Env({name})"),
            PinSource::Ask(_) => write!(f, "Ask(..)"),
        }
    }
}

/// A token certificate PdfCraft can't sign with, and why — shown greyed, like the
/// Windows store's unusable entries (issue #179).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unusable {
    /// The subject's display name, or a placeholder when the certificate can't be read.
    pub subject: String,
    /// Why it can't sign, as a clause that follows the name.
    pub reason: String,
}

/// Every token's signing identities and the certificates that can't sign.
#[derive(Default)]
pub struct Listing {
    pub ids: Vec<DigitalId>,
    pub unusable: Vec<Unusable>,
}

/// The signing identities on every token of `module`: each X.509 certificate paired with
/// its private key by `CKA_ID`. `pin` is kept in the identities and asked only when one
/// signs; listing never prompts. Certificates without a key, or with one PdfCraft doesn't
/// sign with, land in `unusable` with the reason instead of being left out silently.
#[cfg(not(target_arch = "wasm32"))]
pub fn list(module: &Module, pin: &PinSource) -> Result<Listing, SignError> {
    use cryptoki::object::{Attribute, AttributeType, CertificateType, ObjectClass};

    let slots = module
        .inner
        .get_slots_with_token()
        .map_err(|e| SignError::Crypto(format!("the PKCS #11 module at {} couldn't list its slots: {}", module.path.display(), describe(&e))))?;
    let mut ids = Vec::new();
    let mut unusable = Vec::new();
    for slot in slots {
        let mut skip = |subject: String, reason: String| unusable.push(Unusable { subject, reason });
        let info = match module.inner.get_token_info(slot) {
            Ok(info) => info,
            Err(e) => {
                skip("(unreadable token)".to_string(), format!("the token couldn't be read (was it removed?): {}", describe(&e)));
                continue;
            }
        };
        let session = match module.inner.open_ro_session(slot) {
            Ok(session) => session,
            Err(e) => {
                skip("(unreadable token)".to_string(), format!("the token couldn't be opened (was it removed?): {}", describe(&e)));
                continue;
            }
        };
        let handles = match session.find_objects(&[Attribute::Class(ObjectClass::CERTIFICATE), Attribute::CertificateType(CertificateType::X_509)]) {
            Ok(handles) => handles,
            Err(e) => {
                skip(info.label().trim_end().to_string(), format!("the token couldn't be searched: {}", describe(&e)));
                continue;
            }
        };
        let pinpad = info.protected_authentication_path();
        let login_required = info.login_required();
        for handle in handles {
            let attrs = match session.get_attributes(handle, &[AttributeType::Value, AttributeType::Id]) {
                Ok(attrs) => attrs,
                Err(e) => {
                    skip("(unreadable certificate)".to_string(), format!("the certificate can't be read: {}", describe(&e)));
                    continue;
                }
            };
            let Some(value) = cert_value(&attrs) else {
                skip("(unreadable certificate)".to_string(), "the certificate has no value".to_string());
                continue;
            };
            let certificate = match Certificate::parse(value) {
                Ok(certificate) => certificate,
                Err(e) => {
                    skip("(unreadable certificate)".to_string(), format!("the certificate can't be read: {e}"));
                    continue;
                }
            };
            let subject = certificate.display_name();
            let Some(key_id) = object_id(&attrs) else {
                skip(subject, "it has no key identifier (CKA_ID) pairing it with a key".to_string());
                continue;
            };
            let homeless = match session.find_objects(&[Attribute::Class(ObjectClass::PRIVATE_KEY), Attribute::Id(key_id.to_vec())]) {
                Ok(keys) => keys.is_empty(),
                // A search that fails is not proof of absence; say so instead of dropping it.
                Err(e) => {
                    skip(subject, format!("its private key couldn't be looked up: {}", describe(&e)));
                    continue;
                }
            };
            if homeless {
                skip(subject, "it has no private key on the token".to_string());
                continue;
            }
            // P-521, brainpool and Ed25519 certificates parse (to verify with) but aren't signed with.
            if certificate.public_key.store_signing_key().is_none() {
                let what = certificate.public_key.describe();
                skip(subject, format!("its {what} key can't sign through PdfCraft yet (RSA, P-256 and P-384 only)"));
                continue;
            }
            let public = certificate.public_key.clone();
            let key = PrivateKey::external(
                public.clone(),
                std::sync::Arc::new(Pkcs11Key {
                    module: module.clone(),
                    slot,
                    key_id: key_id.to_vec(),
                    public,
                    login_required,
                    pinpad,
                    pin: pin.clone(),
                }),
            );
            ids.push(DigitalId { key, certificate, chain: Vec::new(), friendly_name: Some(subject) });
        }
    }
    Ok(Listing { ids, unusable })
}

/// The `CKA_VALUE` of a certificate object: its DER bytes.
#[cfg(not(target_arch = "wasm32"))]
fn cert_value(attrs: &[cryptoki::object::Attribute]) -> Option<&[u8]> {
    attrs.iter().find_map(|a| match a {
        cryptoki::object::Attribute::Value(v) => Some(v.as_slice()),
        _ => None,
    })
}

/// The `CKA_ID` pairing a certificate with its private key.
#[cfg(not(target_arch = "wasm32"))]
fn object_id(attrs: &[cryptoki::object::Attribute]) -> Option<&[u8]> {
    attrs.iter().find_map(|a| match a {
        cryptoki::object::Attribute::Id(v) => Some(v.as_slice()),
        _ => None,
    })
}

/// A token private key: found by `CKA_ID` at listing, opened for one session at signing.
/// The PIN is asked then and lives only in that call.
#[cfg(not(target_arch = "wasm32"))]
struct Pkcs11Key {
    module: Module,
    slot: cryptoki::slot::Slot,
    key_id: Vec<u8>,
    public: PublicKey,
    login_required: bool,
    pinpad: bool,
    pin: PinSource,
}

#[cfg(not(target_arch = "wasm32"))]
impl ExternalKey for Pkcs11Key {
    fn sign(&self, alg: DigestAlg, msg: &[u8]) -> Result<Vec<u8>, SignError> {
        use cryptoki::mechanism::Mechanism;
        use cryptoki::object::{Attribute, ObjectClass};

        if !matches!(alg, DigestAlg::Sha256 | DigestAlg::Sha384 | DigestAlg::Sha512) {
            return Err(SignError::Unsupported(format!("{} for new signatures", alg.name())));
        }
        let digest = alg.digest(&[msg]);
        // RSA signs the DigestInfo (built here: the token signs exactly these bytes);
        // ECDSA signs the digest itself and returns raw r ‖ s.
        let (mechanism, input) = match &self.public {
            PublicKey::Rsa { .. } => (Mechanism::RsaPkcs, digest_info(alg, &digest)?),
            PublicKey::P256(_) | PublicKey::P384(_) => (Mechanism::Ecdsa, digest),
            _ => return Err(SignError::Unsupported(format!("signing with {} keys", self.public.describe()))),
        };
        let session = self
            .module
            .inner
            .open_ro_session(self.slot)
            .map_err(|e| SignError::Crypto(format!("the token couldn't be opened for signing (was it removed?): {}", describe(&e))))?;
        // The session logs out when it drops; the PIN below lives only in this call.
        self.login(&session)?;
        let mut keys = session
            .find_objects(&[Attribute::Class(ObjectClass::PRIVATE_KEY), Attribute::Id(self.key_id.clone())])
            .map_err(|e| SignError::Crypto(format!("the token key couldn't be found (was the token changed?): {}", describe(&e))))?;
        let Some(key) = keys.pop() else {
            return Err(SignError::Crypto("the token key is gone (was the token changed?)".into()));
        };
        let raw = session.sign(&mechanism, key, &input).map_err(|e| SignError::Crypto(format!("the token didn't sign: {}", describe(&e))))?;
        self.public.store_signature_der(&raw)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Pkcs11Key {
    /// Log the session in: nothing when the token needs none, an empty login on a pinpad,
    /// otherwise the PIN from the caller's source. A wrong PIN is the token's error, with
    /// its remaining-tries flags left for the caller to read from the token info.
    fn login(&self, session: &cryptoki::session::Session) -> Result<(), SignError> {
        use cryptoki::session::UserType;
        if !self.login_required {
            return Ok(());
        }
        if self.pinpad {
            return session
                .login(UserType::User, None)
                .map_err(|e| SignError::Crypto(format!("the token's reader couldn't log in (was the token removed?): {}", describe(&e))));
        }
        let pin: secrecy::SecretString = match &self.pin {
            PinSource::None => return Err(SignError::Crypto("signing with this token needs its PIN".into())),
            PinSource::Env(name) => secrecy::SecretString::new(
                std::env::var(name).map_err(|_| SignError::Crypto(format!("signing needs the token PIN in the {name} environment variable")))?.into(),
            ),
            PinSource::Ask(ask) => ask()?,
        };
        let auth: cryptoki::types::AuthPin = pin;
        session.login(UserType::User, Some(&auth)).map_err(|e| SignError::Crypto(format!("the token couldn't be unlocked: {}", describe(&e))))
    }
}

/// A PKCS #11 failure as a sentence saying what happened, instead of a bare return code.
#[cfg(not(target_arch = "wasm32"))]
fn describe(error: &cryptoki::error::Error) -> String {
    use cryptoki::error::{Error, RvError};
    let rv = match error {
        Error::Pkcs11(rv, ..) => rv,
        _ => return error.to_string(),
    };
    let meaning = match rv {
        RvError::PinIncorrect => "the PIN is incorrect",
        RvError::PinLocked => "the PIN is locked",
        RvError::PinExpired => "the PIN has expired and must be changed",
        RvError::TokenNotPresent => "the token isn't inserted",
        RvError::TokenNotRecognized => "the token isn't recognized",
        RvError::DeviceRemoved => "the device was removed",
        RvError::DeviceError | RvError::DeviceMemory => "the device reported an error",
        RvError::UserAlreadyLoggedIn => "already logged in",
        _ => return error.to_string(),
    };
    format!("{meaning} ({rv:?})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_round_trip_through_the_fingerprint() {
        let fp = parse_reference("pkcs11:b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c").unwrap();
        assert_eq!(
            fp,
            [
                0xb5, 0xbb, 0x9d, 0x80, 0x14, 0xa0, 0xf9, 0xb1, 0xd6, 0x1e, 0x21, 0xe7, 0x96, 0xd7, 0x8d, 0xcc, 0xdf, 0x13, 0x52, 0xf2, 0x3c, 0xd3,
                0x28, 0x12, 0xf4, 0x85, 0x0b, 0x87, 0x8a, 0xe4, 0x94, 0x4c
            ]
        );
        // Upper-case hex reads too (fingerprints are shown upper-case elsewhere).
        assert_eq!(parse_reference("pkcs11:B5BB9D8014A0F9B1D61E21E796D78DCCDF1352F23CD32812F4850B878AE4944C").unwrap(), fp);
    }

    #[test]
    fn references_reject_anything_but_pkcs11_colon_and_64_hex() {
        for bad in [
            "",
            "pkcs11:",
            "windows:b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c",
            "pkcs11:b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944",
            "pkcs11:b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c00",
            "pkcs11:zbbs9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c",
            "pkcs11: b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944",
        ] {
            assert!(parse_reference(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn digest_info_matches_the_rfc_8017_shape() {
        // SHA-256("abc").
        let digest =
            b"\xba\x78\x16\xbf\x8f\x01\xcf\xea\x41\x41\x40\xde\x5d\xae\x22\x23\xb0\x03\x61\xa3\x96\x17\x7a\x9c\xb4\x10\xff\x61\xf2\x00\x15\xad";
        let got = digest_info(DigestAlg::Sha256, digest).unwrap();
        let mut want = vec![0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20];
        want.extend_from_slice(digest);
        assert_eq!(got, want);
    }

    #[test]
    fn digest_info_refuses_anything_it_would_not_sign() {
        assert!(matches!(digest_info(DigestAlg::Sha1, &[0u8; 20]), Err(SignError::Unsupported(_))));
        assert!(digest_info(DigestAlg::Sha256, &[0u8; 31]).is_err());
        assert!(digest_info(DigestAlg::Sha384, &[0u8; 32]).is_err());
        assert!(digest_info(DigestAlg::Sha512, &[0u8; 64]).is_ok());
    }

    /// A missing or garbage module is an error naming the path, never a crash. `file!()` is
    /// this test's own source: it exists but is no DLL.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn broken_modules_are_errors_naming_the_path() {
        for path in ["/nonexistent/pkcs11-test-module.so", file!()] {
            let e = super::load_module(std::path::Path::new(path)).unwrap_err().to_string();
            assert!(e.contains("couldn't be loaded"), "{path}: {e}");
            assert!(e.contains(path), "{e}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn failures_say_what_happened() {
        use cryptoki::error::{Error, RvError};
        // `Function` carries no meaning here; any value does.
        let at = |rv| super::describe(&Error::Pkcs11(rv, cryptoki::context::Function::GetInfo));
        assert!(at(RvError::PinIncorrect).contains("incorrect"));
        assert!(at(RvError::PinLocked).contains("locked"));
        assert!(at(RvError::TokenNotPresent).contains("isn't inserted"));
        assert!(at(RvError::DeviceRemoved).contains("removed"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn attributes_give_the_cert_and_its_key_id() {
        use cryptoki::object::Attribute;
        let attrs = vec![Attribute::Label(b"ignored".to_vec()), Attribute::Id(vec![1, 2]), Attribute::Value(vec![3, 4])];
        assert_eq!(super::object_id(&attrs), Some([1, 2].as_slice()));
        assert_eq!(super::cert_value(&attrs), Some([3, 4].as_slice()));
        let missing: Vec<Attribute> = vec![Attribute::Label(b"x".to_vec())];
        assert_eq!(super::object_id(&missing), None);
        assert_eq!(super::cert_value(&missing), None);
    }

    #[test]
    fn pin_sources_never_print_a_pin() {
        let debug = format!("{:?}", super::PinSource::Env("PDFCRAFT_PIN".to_string()));
        assert!(debug.contains("PDFCRAFT_PIN"));
        let ask = super::PinSource::Ask(std::sync::Arc::new(|| Ok(secrecy::SecretString::new("s3cret".to_string().into()))));
        assert_eq!(format!("{ask:?}"), "Ask(..)");
    }
}
