//! PKCS#11 hardware token and smartcard support (A3 certificates).
//!
//! Connects to hardware cryptographic devices (USB tokens, smart cards) via PKCS#11 modules,
//! reads X.509 certificates and performs hardware-backed digital signing without extracting
//! private keys.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cryptoki::context::{CInitializeArgs, CInitializeFlags, Pkcs11};
use cryptoki::error::{Error as CryptokiError, RvError};
use cryptoki::mechanism::Mechanism;
use cryptoki::object::{Attribute, AttributeType, CertificateType, ObjectClass};
use cryptoki::session::UserType;
use cryptoki::slot::Slot;
use cryptoki::types::AuthPin;

use crate::keys::{DigestAlg, ExternalKey, PrivateKey, PublicKey};
use crate::{Certificate, DigitalId, SignError, x509};

/// Default library paths for common PKCS#11 drivers (OpenSC, SafeNet eToken, SafeSign, etc.).
pub const DEFAULT_MODULE_PATHS: &[&str] = &[
    "/usr/lib/libcastle.so",
    "/usr/lib/libcastle.so.1.0.0",
    "/usr/lib/x86_64-linux-gnu/libcastle.so",
    "/usr/lib/opensc-pkcs11.so",
    "/usr/lib/x86_64-linux-gnu/opensc-pkcs11.so",
    "/usr/lib64/opensc-pkcs11.so",
    "/usr/lib/pkcs11/opensc-pkcs11.so",
    "/usr/lib/onepin-opensc-pkcs11.so",
    "/usr/lib/pkcs11/onepin-opensc-pkcs11.so",
    "/usr/local/lib/opensc-pkcs11.so",
    "/usr/lib/libeToken.so",
    "/usr/lib/libeTPkcs11.so",
    "/usr/lib/x86_64-linux-gnu/libeToken.so",
    "/usr/lib/x86_64-linux-gnu/libeTPkcs11.so",
    "/usr/lib/libaetpkss.so",
    "/usr/lib/x86_64-linux-gnu/libaetpkss.so",
    "/usr/lib/libgmpkcs11.so",
    "eTPKCS11.dll",
    "C:\\Windows\\System32\\eTPKCS11.dll",
    "aetpkss1.dll",
    "C:\\Windows\\System32\\aetpkss1.dll",
    "acospkcs11.dll",
    "C:\\Windows\\System32\\acospkcs11.dll",
    "OcsCryptolib_pkcs11.dll",
    "C:\\Windows\\System32\\OcsCryptolib_pkcs11.dll",
    "opensc-pkcs11.dll",
    "C:\\Program Files\\OpenSC Project\\OpenSC\\pkcs11\\opensc-pkcs11.dll",
    "C:\\Windows\\System32\\opensc-pkcs11.dll",
    "/Library/OpenSC/lib/opensc-pkcs11.so",
    "/Library/Frameworks/eToken.framework/Versions/Current/libeToken.dylib",
    "/usr/local/lib/libaetpkss.dylib",
];

/// Returns the existing candidate PKCS#11 module libraries on this system, deduplicated.
pub fn available_modules(custom: Option<&[&str]>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut add = |p: &Path| {
        if let Ok(canon) = std::fs::canonicalize(p) {
            if !paths.contains(&canon) {
                paths.push(canon);
            }
        } else if p.exists() {
            let pb = p.to_path_buf();
            if !paths.contains(&pb) {
                paths.push(pb);
            }
        }
    };
    if let Some(custom_paths) = custom {
        for &p in custom_paths {
            add(Path::new(p));
        }
    }
    for &p in DEFAULT_MODULE_PATHS {
        add(Path::new(p));
    }
    paths
}

/// A PKCS#11 hardware token private key.
pub struct Pkcs11Key {
    module_path: PathBuf,
    slot_id: u64,
    key_id: Vec<u8>,
    pin: Mutex<Option<String>>,
    public: PublicKey,
}

impl Pkcs11Key {
    pub fn new(module_path: PathBuf, slot_id: u64, key_id: Vec<u8>, pin: Option<String>, public: PublicKey) -> Self {
        Self { module_path, slot_id, key_id, pin: Mutex::new(pin), public }
    }

    pub fn set_pin(&self, pin: Option<String>) {
        if let Ok(mut lock) = self.pin.lock() {
            *lock = pin;
        }
    }
}

/// Load or retrieve an already-initialized PKCS#11 module context.
/// PKCS#11 driver libraries are held in memory for the process lifetime
/// to avoid driver thread crashes from premature dlclose on vendor drivers.
pub fn get_or_load_module(path: &Path) -> Result<&'static Pkcs11, SignError> {
    static CACHE: Mutex<Option<Vec<(PathBuf, &'static Pkcs11)>>> = Mutex::new(None);
    let mut lock = CACHE.lock().map_err(|_| SignError::Crypto("module cache mutex poisoned".into()))?;
    let list = lock.get_or_insert_with(Vec::new);
    if let Some((_, pk)) = list.iter().find(|(p, _)| p == path) {
        return Ok(*pk);
    }
    let pk = Pkcs11::new(path).map_err(|e| SignError::Crypto(format!("failed to load PKCS#11 module {}: {e}", path.display())))?;
    let _ = pk.initialize(CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK));
    let leaked: &'static Pkcs11 = Box::leak(Box::new(pk));
    list.push((path.to_path_buf(), leaked));
    Ok(leaked)
}

impl ExternalKey for Pkcs11Key {
    fn sign(&self, alg: DigestAlg, msg: &[u8]) -> Result<Vec<u8>, SignError> {
        log::info!("PKCS#11 external signing invoked with algorithm: {alg:?}");
        let pk = get_or_load_module(&self.module_path)?;

        let slot = Slot::try_from(self.slot_id).map_err(|e| SignError::Crypto(format!("invalid PKCS#11 slot {}: {e}", self.slot_id)))?;

        // Most crypto tokens require a read-write session to authenticate with user PIN
        let session = pk
            .open_rw_session(slot)
            .or_else(|_| pk.open_ro_session(slot))
            .map_err(|e| SignError::Crypto(format!("failed to open PKCS#11 session on slot {}: {e}", self.slot_id)))?;

        let pin_val = match self.pin.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => return Err(SignError::Crypto("mutex lock poisoned".into())),
        };

        if let Some(pin_str) = pin_val {
            log::debug!("Authenticating user with provided PIN...");
            let auth_pin = AuthPin::from(pin_str);
            if let Err(e) = session.login(UserType::User, Some(&auth_pin)) {
                match e {
                    CryptokiError::Pkcs11(RvError::UserAlreadyLoggedIn, _) => {
                        log::debug!("User already logged in to PKCS#11 session.");
                    }
                    CryptokiError::Pkcs11(RvError::PinIncorrect, _) => {
                        log::warn!("PKCS#11 token login failed: incorrect PIN.");
                        return Err(SignError::WrongPassword);
                    }
                    CryptokiError::Pkcs11(RvError::PinLocked, _) => {
                        log::error!("PKCS#11 token login failed: PIN is locked.");
                        return Err(SignError::Crypto("The token PIN is locked (PUK required).".into()));
                    }
                    e => {
                        log::error!("PKCS#11 token login failed: {e}");
                        return Err(SignError::Crypto(format!("PKCS#11 token login failed: {e}")));
                    }
                }
            } else {
                log::debug!("PIN authentication successful.");
            }
        } else {
            log::warn!("No PIN provided for hardware token signing attempt.");
        }

        // Find private key matching key_id
        log::debug!("Locating private key on token (ID: {:?})...", self.key_id);
        let template = vec![Attribute::Class(ObjectClass::PRIVATE_KEY), Attribute::Id(self.key_id.clone())];
        let mut objects = session.find_objects(&template).unwrap_or_default();

        // Fallback: search for any signing private key on the token
        if objects.is_empty() {
            log::debug!("Exact key ID not found; falling back to first object with Class=PRIVATE_KEY...");
            let fallback_template = vec![Attribute::Class(ObjectClass::PRIVATE_KEY)];
            objects = session.find_objects(&fallback_template).map_err(|e| SignError::Crypto(format!("failed to query private key: {e}")))?;
        }

        let key_handle = objects.into_iter().next().ok_or_else(|| {
            log::error!("No private key found on token for this certificate.");
            SignError::Crypto("No private key found on token for this certificate.".into())
        })?;

        let digest = alg.digest(&[msg]);
        let raw_sig = match self.public {
            PublicKey::Rsa { .. } => {
                log::debug!("Computing RSA signature on token chip...");
                let digest_info = crate::rsa_pad::digest_info(alg, &digest, true);
                let sign_res = session.sign(&Mechanism::RsaPkcs, key_handle, &digest_info);
                match sign_res {
                    Ok(s) => s,
                    Err(e) => {
                        log::debug!("Mechanism::RsaPkcs with DigestInfo failed ({e}); retrying with raw digest...");
                        let sign_res2 = session.sign(&Mechanism::RsaPkcs, key_handle, &digest);
                        match sign_res2 {
                            Ok(s) => s,
                            Err(e2) => {
                                log::debug!("Mechanism::RsaPkcs simple failed ({e2}); trying Sha256RsaPkcs...");
                                session
                                    .sign(&Mechanism::Sha256RsaPkcs, key_handle, msg)
                                    .map_err(|e3| SignError::Crypto(format!("PKCS#11 RSA signature failed (RsaPkcs: {e}, Sha256: {e3})")))?
                            }
                        }
                    }
                }
            }
            PublicKey::P256(_) | PublicKey::P384(_) => {
                log::debug!("Computing ECDSA signature on token chip...");
                session.sign(&Mechanism::Ecdsa, key_handle, &digest).map_err(|e| SignError::Crypto(format!("PKCS#11 ECDSA signature failed: {e}")))?
            }
            _ => return Err(SignError::Unsupported(format!("PKCS#11 signing with {}", self.public.describe()))),
        };

        log::info!("Token computed cryptographic signature successfully (size: {} bytes)", raw_sig.len());
        let _ = session.logout();
        self.public.store_signature_der(&raw_sig)
    }
}

/// A token certificate that cannot be used for signing, with the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unusable {
    pub subject: String,
    pub fingerprint: String,
    pub reason: String,
    pub token_label: String,
}

/// The PKCS#11 tokens' signing identities and any unusable certificates.
#[derive(Default)]
pub struct Listing {
    pub ids: Vec<DigitalId>,
    pub unusable: Vec<Unusable>,
}

/// Returns the `pkcs11:<SHA-256 fingerprint>` reference string for an identity.
pub fn reference(certificate: &Certificate) -> String {
    let digest = DigestAlg::Sha256.digest(&[&certificate.raw]);
    format!("pkcs11:{}", digest.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// Internal token certificate entry with its location and metadata.
struct TokenCertEntry {
    module_path: PathBuf,
    slot_id: u64,
    token_label: String,
    key_id: Vec<u8>,
    cert: Certificate,
    chain: Vec<Certificate>,
}

/// Enumerate certificates across given or default modules.
fn enumerate_token_certificates(custom_modules: Option<&[&str]>) -> (Vec<TokenCertEntry>, Vec<Unusable>) {
    let mut entries = Vec::new();
    let mut unusable = Vec::new();

    let modules = available_modules(custom_modules);
    for module_path in modules {
        let pk = match get_or_load_module(&module_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let slots = match pk.get_slots_with_token() {
            Ok(s) => s,
            Err(_) => continue,
        };

        if slots.is_empty() {
            if let Ok(all) = pk.get_all_slots() {
                for slot in all {
                    if let Ok(info) = pk.get_slot_info(slot) {
                        let desc = info.slot_description().trim().to_string();
                        if !desc.is_empty() && !desc.contains("UNAVAILABLE") && !unusable.iter().any(|u: &Unusable| u.fingerprint == format!("slot-{}", slot.id())) {
                            unusable.push(Unusable {
                                subject: desc.clone(),
                                fingerprint: format!("slot-{}", slot.id()),
                                reason: "Leitor detectado, mas a comunicação falhou (pcscd travado ou porta USB). Reconecte o token.".to_string(),
                                token_label: desc,
                            });
                        }
                    }
                }
            }
            continue;
        }

        for slot in slots {
            let slot_id = slot.id();
            let token_info = match pk.get_token_info(slot) {
                Ok(info) => info,
                Err(_) => continue,
            };
            let token_label = token_info.label().trim().to_string();

            let session = match pk.open_ro_session(slot) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Query X.509 certificate objects
            let template = vec![Attribute::Class(ObjectClass::CERTIFICATE), Attribute::CertificateType(CertificateType::X_509)];

            let cert_handles = match session.find_objects(&template) {
                Ok(h) => h,
                Err(_) => continue,
            };

            let mut slot_certs = Vec::new();
            for handle in cert_handles {
                let attrs = match session.get_attributes(handle, &[AttributeType::Value, AttributeType::Id, AttributeType::Label]) {
                    Ok(a) => a,
                    Err(_) => continue,
                };

                let mut val = None;
                let mut id = Vec::new();
                for attr in attrs {
                    match attr {
                        Attribute::Value(v) => val = Some(v),
                        Attribute::Id(i) => id = i,
                        _ => {}
                    }
                }

                let Some(cert_bytes) = val else { continue };
                let cert = match Certificate::parse(&cert_bytes) {
                    Ok(c) => c,
                    Err(e) => {
                        let subject = x509::subject_of(&cert_bytes).map(|n| n.display_name()).unwrap_or_else(|| "(unreadable)".into());
                        let fp = DigestAlg::Sha256.digest(&[&cert_bytes]).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
                        unusable.push(Unusable {
                            subject,
                            fingerprint: fp,
                            reason: format!("the certificate cannot be parsed: {e}"),
                            token_label: token_label.clone(),
                        });
                        continue;
                    }
                };

                slot_certs.push((id, cert));
            }

            for (id, cert) in &slot_certs {
                // Intermediate or root CA certificates belong to the chain, not user signing identities
                if cert.may_issue() {
                    continue;
                }

                let fingerprint = cert.fingerprint();
                let is_signing_allowed = cert.key_usage.is_none_or(|u| u & 0b11 != 0);

                if !is_signing_allowed {
                    unusable.push(Unusable {
                        subject: cert.display_name(),
                        fingerprint,
                        reason: "its key usage does not allow digital signatures".into(),
                        token_label: token_label.clone(),
                    });
                    continue;
                }

                if cert.public_key.store_signing_key().is_none() {
                    let what = cert.public_key.describe();
                    unusable.push(Unusable {
                        subject: cert.display_name(),
                        fingerprint,
                        reason: format!("its {what} key is not supported for signing (RSA and ECDSA P-256/P-384 only)"),
                        token_label: token_label.clone(),
                    });
                    continue;
                }

                // Collect other certificates on the token as potential chain
                let chain: Vec<Certificate> = slot_certs.iter().filter(|(_, c)| c.raw != cert.raw && c.may_issue()).map(|(_, c)| c.clone()).collect();

                entries.push(TokenCertEntry {
                    module_path: module_path.clone(),
                    slot_id,
                    token_label: token_label.clone(),
                    key_id: id.clone(),
                    cert: cert.clone(),
                    chain,
                });
            }
        }
    }

    // If working token identities were found, don't clutter the dialog with reader error notices
    if !entries.is_empty() {
        unusable.retain(|u| !u.reason.contains("Leitor detectado"));
    }

    (entries, unusable)
}

/// List usable PKCS#11 token signing identities and unusable certificates.
pub fn list(custom_modules: Option<&[&str]>) -> Result<Listing, SignError> {
    let (entries, unusable) = enumerate_token_certificates(custom_modules);
    let mut ids = Vec::new();

    for entry in entries {
        let public = entry.cert.public_key.clone();
        let key = PrivateKey::external(public.clone(), Arc::new(Pkcs11Key::new(entry.module_path, entry.slot_id, entry.key_id, None, public)));
        let friendly_name = Some(format!("{} (Token A3 · {})", entry.cert.display_name(), entry.token_label));
        ids.push(DigitalId { key, certificate: entry.cert, chain: entry.chain, friendly_name });
    }

    Ok(Listing { ids, unusable })
}

/// List all usable PKCS#11 token identities.
pub fn identities(custom_modules: Option<&[&str]>) -> Result<Vec<DigitalId>, SignError> {
    Ok(list(custom_modules)?.ids)
}

/// Find a PKCS#11 identity by its `pkcs11:...` reference or certificate common name.
pub fn find(reference_or_name: &str, pin: Option<&str>, custom_modules: Option<&[&str]>) -> Result<DigitalId, SignError> {
    let (entries, _) = enumerate_token_certificates(custom_modules);
    let found = entries.into_iter().find(|e| {
        let ref_str = reference(&e.cert);
        ref_str == reference_or_name || e.cert.subject.common_name() == Some(reference_or_name.strip_prefix("pkcs11:").unwrap_or(reference_or_name))
    });

    let entry = found.ok_or_else(|| SignError::Crypto(format!("no PKCS#11 token identity found matching {reference_or_name}")))?;

    let public = entry.cert.public_key.clone();
    let key = PrivateKey::external(
        public.clone(),
        Arc::new(Pkcs11Key::new(entry.module_path, entry.slot_id, entry.key_id, pin.map(str::to_string), public)),
    );
    let friendly_name = Some(format!("{} (Token A3 · {})", entry.cert.display_name(), entry.token_label));

    Ok(DigitalId { key, certificate: entry.cert, chain: entry.chain, friendly_name })
}

/// Verify that the user PIN is valid for the specified token identity,
/// returning Ok(()) if correct, or an appropriate error (WrongPassword, PinLocked, etc.).
pub fn verify_pin(reference_or_name: &str, pin: &str, custom_modules: Option<&[&str]>) -> Result<(), SignError> {
    if pin.trim().is_empty() {
        return Err(SignError::Crypto("PIN não fornecido".into()));
    }
    let (entries, _) = enumerate_token_certificates(custom_modules);
    let found = entries.into_iter().find(|e| {
        let ref_str = reference(&e.cert);
        ref_str == reference_or_name || e.cert.subject.common_name() == Some(reference_or_name.strip_prefix("pkcs11:").unwrap_or(reference_or_name))
    });
    let entry = found.ok_or_else(|| SignError::Crypto(format!("no PKCS#11 token identity found matching {reference_or_name}")))?;

    let pk = get_or_load_module(&entry.module_path)?;
    let slot = Slot::try_from(entry.slot_id).map_err(|e| SignError::Crypto(format!("invalid PKCS#11 slot {}: {e}", entry.slot_id)))?;
    let session = pk
        .open_rw_session(slot)
        .or_else(|_| pk.open_ro_session(slot))
        .map_err(|e| SignError::Crypto(format!("failed to open PKCS#11 session on slot {}: {e}", entry.slot_id)))?;

    let auth_pin = AuthPin::from(pin.to_string());
    if let Err(e) = session.login(UserType::User, Some(&auth_pin)) {
        match e {
            CryptokiError::Pkcs11(RvError::UserAlreadyLoggedIn, _) => {
                log::debug!("User already logged in to PKCS#11 session during PIN verification.");
            }
            CryptokiError::Pkcs11(RvError::PinIncorrect, _) => {
                log::warn!("PKCS#11 PIN verification failed: incorrect PIN.");
                return Err(SignError::WrongPassword);
            }
            CryptokiError::Pkcs11(RvError::PinLocked, _) => {
                log::error!("PKCS#11 PIN verification failed: PIN is locked.");
                return Err(SignError::Crypto("The token PIN is locked (PUK required).".into()));
            }
            e => {
                log::error!("PKCS#11 PIN verification error: {e}");
                return Err(SignError::Crypto(format!("Token login failure: {e}")));
            }
        }
    }
    log::info!("PIN verified successfully with PKCS#11 token.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_pkcs11() {
        let listing = list(None).expect("listing should not fail even without tokens");
        println!("Found {} identities, {} unusable", listing.ids.len(), listing.unusable.len());
        for id in &listing.ids {
            println!("  IDENTITY: {:?}", id.friendly_name);
        }
        for u in &listing.unusable {
            println!("  UNUSABLE: {:?} - {:?}", u.subject, u.reason);
        }
    }
}
