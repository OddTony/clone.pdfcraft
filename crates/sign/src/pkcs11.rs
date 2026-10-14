//! Digital IDs from PKCS #11 tokens (PIV smart cards, HSMs): certificates and keys that sign
//! through a loaded PKCS #11 module (execution plan M9). The private key never leaves the
//! token; PdfCraft sends it only a digest (RSA) or a prehash (ECDSA) to sign.
//!
//! Listing ([`list`]) needs no PIN: PIV certificates and their key-usage bits are public objects.
//! Signing ([`open`]) logs in, finds the matching private key, and returns a [`DigitalId`] whose
//! key asks the token to sign on each call.

use std::path::Path;
use std::sync::Mutex;

use cryptoki::context::{CInitializeArgs, Pkcs11 as Context};
use cryptoki::error::Error as CkError;
use cryptoki::error::RvError;
use cryptoki::mechanism::Mechanism;
use cryptoki::object::{Attribute, AttributeType, CertificateType, ObjectClass, ObjectHandle};
use cryptoki::session::{Session, UserType};
use cryptoki::slot::Slot;
use cryptoki::types::AuthPin;

use crate::der;
use crate::keys::{DigestAlg, ExternalKey, PrivateKey, PublicKey};
use crate::x509::Certificate;
use crate::{DigitalId, SignError};

fn pkcs11_err(e: CkError) -> SignError {
    SignError::Crypto(format!("the token reported an error: {e}"))
}

/// Default module search order: the p11-kit proxy (sees every module registered on the system),
/// then OpenSC directly for installs without p11-kit. These are bare library names, not paths:
/// the platform's own dynamic linker resolves them from its standard search path. An explicit
/// module (Preferences, or `--pkcs11-module`) always wins and skips this list.
fn default_module_names() -> &'static [&'static str] {
    if cfg!(windows) { &["opensc-pkcs11.dll"] } else { &["p11-kit-proxy.so", "opensc-pkcs11.so"] }
}

/// Load and initialize a module: an explicit path if given, otherwise the first default that
/// loads.
fn open_module(module: Option<&Path>) -> Result<Context, SignError> {
    // A path with an embedded NUL can't be a real file name; passing it on reaches `dlopen`
    // only after C-string truncation, and `cryptoki`'s bindings panic rather than error when the
    // (wrong) library that opens has no `C_GetFunctionList` symbol. Reject it before that.
    if module.is_some_and(|p| p.as_os_str().as_encoded_bytes().contains(&0)) {
        return Err(SignError::Crypto("the PKCS #11 module path is not valid".into()));
    }
    let ctx = match module {
        Some(p) => Context::new(p).map_err(|e| SignError::Crypto(format!("{}: {e}", p.display())))?,
        None => {
            let mut last: Option<CkError> = None;
            let mut found = None;
            for name in default_module_names() {
                match Context::new(name) {
                    Ok(c) => {
                        found = Some(c);
                        break;
                    }
                    Err(e) => last = Some(e),
                }
            }
            match found {
                Some(c) => c,
                None => {
                    return Err(SignError::Crypto(match last {
                        Some(e) => format!("no PKCS #11 module found ({e}); set one in Preferences"),
                        None => "no PKCS #11 module configured".into(),
                    }));
                }
            }
        }
    };
    ctx.initialize(CInitializeArgs::OsThreads).map_err(pkcs11_err)?;
    Ok(ctx)
}

/// `pkcs11:<slot id>:<hex CKA_ID>` — content-addressed within one configured module, the way
/// `keychain:<fingerprint>` is for the macOS Keychain.
fn reference(slot: Slot, id: &[u8]) -> String {
    format!("pkcs11:{}:{}", slot.id(), id.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

fn parse_reference(reference: &str) -> Result<(u64, Vec<u8>), SignError> {
    let bad = || SignError::Crypto(format!("{reference}: not a PKCS #11 reference"));
    let rest = reference.strip_prefix("pkcs11:").ok_or_else(bad)?;
    let (slot, id) = rest.split_once(':').ok_or_else(bad)?;
    let slot = slot.parse::<u64>().map_err(|_| bad())?;
    if id.is_empty() || id.len() % 2 != 0 {
        return Err(bad());
    }
    let id = (0..id.len()).step_by(2).map(|i| u8::from_str_radix(&id[i..i + 2], 16).map_err(|_| bad())).collect::<Result<Vec<u8>, _>>()?;
    Ok((slot, id))
}

fn attr_value(session: &Session, handle: ObjectHandle) -> Result<Vec<u8>, SignError> {
    session
        .get_attributes(handle, &[AttributeType::Value])
        .map_err(pkcs11_err)?
        .into_iter()
        .find_map(|a| match a {
            Attribute::Value(v) => Some(v),
            _ => None,
        })
        .ok_or_else(|| SignError::Crypto("the object has no CKA_VALUE".into()))
}

/// A certificate found on a token without logging in: enough to list and let the user choose,
/// not yet to sign with (see [`open`]).
#[derive(Debug, Clone)]
pub struct Pkcs11Identity {
    pub certificate: Certificate,
    /// The token's own label (PIV: usually the cardholder's name), e.g. for "On token: {label}".
    pub token_label: String,
    /// Pass to [`open`] to sign with this identity.
    pub reference: String,
    pub login_required: bool,
    /// The token has its own PIN pad or biometric reader (CKF_PROTECTED_AUTHENTICATION_PATH):
    /// don't ask for a PIN, the token collects it itself during [`open`].
    pub protected_authentication_path: bool,
    pub pin_count_low: bool,
    pub pin_final_try: bool,
    pub pin_locked: bool,
}

/// A token certificate PdfCraft can't sign with, and why ([`Certificate::signing_problem`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unusable {
    pub subject: String,
    pub fingerprint: String,
    pub token_label: String,
    pub reason: String,
}

/// A token's signing identities and the certificates on it that can't sign.
#[derive(Default)]
pub struct Listing {
    pub ids: Vec<Pkcs11Identity>,
    pub unusable: Vec<Unusable>,
}

/// The signing identities on every present token, from `module` (or the default search order).
/// Convenience for callers that don't need [`Listing::unusable`]; see [`list`].
pub fn identities(module: Option<&Path>) -> Result<Vec<Pkcs11Identity>, SignError> {
    Ok(list(module)?.ids)
}

/// List every present token's certificates: the ones that can sign documents
/// ([`Certificate::signing_problem`] — keyUsage, EKU and critical-extension rules, the same
/// check signature validation uses), and the rest with why not (e.g. a PIV card's encryption-only
/// certificate, or PIV Authentication (9A), which has no `nonRepudiation` bit either).
pub fn list(module: Option<&Path>) -> Result<Listing, SignError> {
    let ctx = open_module(module)?;
    let slots = ctx.get_slots_with_token().map_err(pkcs11_err)?;
    let mut out = Listing::default();
    for slot in slots {
        let Ok(info) = ctx.get_token_info(slot) else { continue };
        let Ok(session) = ctx.open_ro_session(slot) else { continue };
        let Ok(certs) = session.find_objects(&[Attribute::Class(ObjectClass::CERTIFICATE), Attribute::CertificateType(CertificateType::X_509)])
        else {
            continue;
        };
        let token_label = info.label().trim().to_string();
        for handle in certs {
            let Ok(attrs) = session.get_attributes(handle, &[AttributeType::Value, AttributeType::Id]) else { continue };
            let (mut der, mut id) = (None, None);
            for a in attrs {
                match a {
                    Attribute::Value(v) => der = Some(v),
                    Attribute::Id(v) => id = Some(v),
                    _ => {}
                }
            }
            let (Some(der), Some(id)) = (der, id) else { continue };
            let Ok(certificate) = Certificate::parse(&der) else { continue };
            if let Some(reason) = certificate.signing_problem() {
                out.unusable.push(Unusable {
                    subject: certificate.display_name(),
                    fingerprint: certificate.fingerprint(),
                    token_label: token_label.clone(),
                    reason: reason.message,
                });
                continue;
            }
            out.ids.push(Pkcs11Identity {
                certificate,
                token_label: token_label.clone(),
                reference: reference(slot, &id),
                login_required: info.login_required(),
                protected_authentication_path: info.protected_authentication_path(),
                pin_count_low: info.user_pin_count_low(),
                pin_final_try: info.user_pin_final_try(),
                pin_locked: info.user_pin_locked(),
            });
        }
    }
    Ok(out)
}

/// A private key that lives on a PKCS #11 token: each `sign` opens a fresh context-specific
/// login first if the key demands one (`CKA_ALWAYS_AUTHENTICATE`), then asks the token to sign.
struct Pkcs11Key {
    session: Mutex<Session>,
    handle: ObjectHandle,
    public: PublicKey,
    always_authenticate: bool,
    /// Kept only to satisfy `CKA_ALWAYS_AUTHENTICATE` re-logins; `None` when the token has its
    /// own PIN pad. Cleared (and its memory zeroed by `SecretString`) when the key is dropped.
    pin: Mutex<Option<AuthPin>>,
}

/// A `CKA_ALWAYS_AUTHENTICATE` login (PKCS #11 §5.6.4) is only valid in the middle of an active
/// sign operation, so it has to land between `C_SignInit` and the data being signed — the
/// all-in-one `Session::sign` has no hook for that. `sign_update`/`sign_final` does: PIV
/// middleware (OpenSC) accepts single-part data through them, even for mechanisms (RSA PKCS#1
/// v1.5, ECDSA) that are conceptually single-part only.
fn sign_raw(
    session: &Session,
    mechanism: &Mechanism,
    handle: ObjectHandle,
    data: &[u8],
    reauth: Option<Option<&AuthPin>>,
) -> Result<Vec<u8>, SignError> {
    let Some(pin) = reauth else { return session.sign(mechanism, handle, data).map_err(pkcs11_err) };
    session.sign_init(mechanism, handle).map_err(pkcs11_err)?;
    // `pin` is `None` when the token has its own PIN pad: it collects the re-authentication
    // itself, so `C_Login` still runs here but with no PIN to supply.
    session.login(UserType::ContextSpecific, pin).map_err(pkcs11_err)?;
    session.sign_update(data).map_err(pkcs11_err)?;
    session.sign_final().map_err(pkcs11_err)
}

impl ExternalKey for Pkcs11Key {
    fn sign(&self, alg: DigestAlg, msg: &[u8]) -> Result<Vec<u8>, SignError> {
        let session = self.session.lock().map_err(|_| SignError::Crypto("the token session lock was poisoned".into()))?;
        let pin = self.pin.lock().map_err(|_| SignError::Crypto("the token PIN lock was poisoned".into()))?;
        let reauth = self.always_authenticate.then(|| pin.as_ref());
        let hash = alg.digest(&[msg]);
        match &self.public {
            PublicKey::Rsa { .. } => {
                // CKM_RSA_PKCS pads and signs exactly the bytes given; it does not hash, so the
                // DigestInfo (hash OID + digest) has to be built here.
                let info = der::seq(&[&der::algorithm(alg.oid(), Some(&der::null())), &der::octets(&hash)]);
                sign_raw(&session, &Mechanism::RsaPkcs, self.handle, &info, reauth)
            }
            PublicKey::P256(_) | PublicKey::P384(_) => {
                let raw = sign_raw(&session, &Mechanism::Ecdsa, self.handle, &hash, reauth)?;
                self.public.store_signature_der(&raw)
            }
            _ => Err(SignError::Unsupported(format!("signing with {} keys", self.public.describe()))),
        }
    }
}

/// Open the identity named by `reference` (from [`list`]), logging in with `pin` (`None` when
/// the token has its own PIN pad), and return a [`DigitalId`] ready to sign with. The chain is
/// built from any other certificates on the token (common on an HSM, rare on a PIV card, which
/// usually carries only the leaf) plus `extra_certs` — a caller-supplied bundle for when the
/// issuing CA's certificate has to come from elsewhere (its AIA "CA Issuers" URL, say: this crate
/// does not fetch one).
///
/// Before calling this, check [`Pkcs11Identity::pin_locked`] / `pin_final_try` / `pin_count_low`
/// from the matching entry and warn the user — a wrong PIN here can permanently lock the token.
pub fn open(module: Option<&Path>, reference: &str, pin: Option<String>, extra_certs: &[Certificate]) -> Result<DigitalId, SignError> {
    // Wrapped in `AuthPin` (a `SecretString`) immediately: the only copy of the plaintext PIN
    // outside this zeroizing wrapper is the caller's, which came from the app's own dialog or a
    // CLI prompt, never argv, env, logs or disk.
    let pin = pin.map(AuthPin::new);
    let (slot_id, id) = parse_reference(reference)?;
    let ctx = open_module(module)?;
    let slot = Slot::try_from(slot_id).map_err(|_| SignError::Crypto(format!("{reference}: no such slot")))?;
    let info = ctx.get_token_info(slot).map_err(pkcs11_err)?;
    let session = ctx.open_ro_session(slot).map_err(pkcs11_err)?;
    if info.protected_authentication_path() {
        session.login(UserType::User, None).map_err(pkcs11_err)?;
    } else {
        let Some(pin) = &pin else { return Err(SignError::Crypto("a PIN is required".into())) };
        session.login(UserType::User, Some(pin)).map_err(|e| match e {
            CkError::Pkcs11(RvError::PinIncorrect, _) => SignError::WrongPassword,
            e => pkcs11_err(e),
        })?;
    }
    let cert_handle = session
        .find_objects(&[Attribute::Class(ObjectClass::CERTIFICATE), Attribute::Id(id.clone())])
        .map_err(pkcs11_err)?
        .into_iter()
        .next()
        .ok_or_else(|| SignError::Crypto("the certificate is no longer on the token".into()))?;
    let certificate = Certificate::parse(&attr_value(&session, cert_handle)?)?;
    if let Some(reason) = certificate.signing_problem() {
        return Err(SignError::Unsupported(format!("{}: {}", certificate.display_name(), reason.message)));
    }
    // Every other certificate on the token (any class, any key usage: an intermediate CA
    // certificate signs nothing itself) plus the caller's bundle, in case the issuer is there.
    let mut pool: Vec<Certificate> = session
        .find_objects(&[Attribute::Class(ObjectClass::CERTIFICATE)])
        .map_err(pkcs11_err)?
        .into_iter()
        .filter(|&h| h != cert_handle)
        .filter_map(|h| attr_value(&session, h).ok().and_then(|v| Certificate::parse(&v).ok()))
        .collect();
    pool.extend(extra_certs.iter().cloned());
    let chain = crate::x509::build_chain(&certificate, &pool, None).into_iter().skip(1).cloned().collect();
    let key_handle = session
        .find_objects(&[Attribute::Class(ObjectClass::PRIVATE_KEY), Attribute::Id(id)])
        .map_err(pkcs11_err)?
        .into_iter()
        .next()
        .ok_or_else(|| SignError::Crypto("the private key is no longer on the token".into()))?;
    let always_authenticate = session
        .get_attributes(key_handle, &[AttributeType::AlwaysAuthenticate])
        .ok()
        .and_then(|attrs| {
            attrs.into_iter().find_map(|a| match a {
                Attribute::AlwaysAuthenticate(b) => Some(b),
                _ => None,
            })
        })
        .unwrap_or(false);
    let key = PrivateKey::external(
        certificate.public_key.clone(),
        std::sync::Arc::new(Pkcs11Key {
            session: Mutex::new(session),
            handle: key_handle,
            public: certificate.public_key.clone(),
            always_authenticate,
            pin: Mutex::new(pin),
        }),
    );
    // Matches the Keychain's "Name (smart card)" convention for identities backed by hardware.
    let friendly_name = Some(format!("{} (smart card)", certificate.display_name()));
    Ok(DigitalId { key, certificate, chain, friendly_name })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A module path with an embedded NUL (a hostile `sign_token_ids`/`sign_document` "module"
    /// argument from a script) must error, not reach `cryptoki`'s FFI bindings, which panic
    /// rather than return an error when the (wrong) library that a truncated path opens has no
    /// `C_GetFunctionList` symbol.
    #[test]
    fn a_module_path_with_an_embedded_nul_is_rejected_cleanly() {
        let path = Path::new("/tmp/pdfcraft-test-\0-module.so");
        assert!(open_module(Some(path)).is_err());
        assert!(list(Some(path)).is_err());
        assert!(open(Some(path), "pkcs11:0:00", None, &[]).is_err());
    }
}
