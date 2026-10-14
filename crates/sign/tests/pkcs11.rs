//! Signing with PKCS #11 tokens, against a throwaway SoftHSM2 token (never the system's).
//! Skipped cleanly when `softhsm2-util` or `pkcs11-tool` aren't installed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use cryptoki::context::{CInitializeArgs, Pkcs11};
use cryptoki::mechanism::Mechanism;
use cryptoki::object::{Attribute, AttributeType, ObjectClass, ObjectHandle};
use cryptoki::session::{Session, UserType};
use cryptoki::types::AuthPin;
use pdfcraft_cos::Document;
use pdfcraft_sign::keys::{DigestAlg, ExternalKey, PrivateKey, PublicKey};
use pdfcraft_sign::x509::{Certificate, Name};
use pdfcraft_sign::{SignError, SignOptions, Status, Time, TrustStore, der, pkcs11, signatures};

/// Like `Certificate::self_signed`, but the certified key (`subject_public`) and the signing
/// key (`signer`) can differ — a CA-issued certificate rather than a true self-signed one. Only
/// used to provision the always-authenticate test key (see [`provision_certificate`]).
fn issue_certificate(
    dn: &Name,
    subject_public: &pdfcraft_sign::keys::PublicKey,
    signer: &PrivateKey,
    from: Time,
    years: u32,
    serial: &[u8],
) -> Certificate {
    use pdfcraft_sign::der::tag;
    use pdfcraft_sign::keys::DigestAlg;
    let alg = signer.preferred_digest();
    let sig_alg = signer.signature_algorithm(alg);
    let until = Time { year: from.year + years, ..from };
    let spki = subject_public.spki();
    let ski = DigestAlg::Sha1.digest(&[&spki]);
    let ext = |o: &str, critical: bool, value: &[u8]| {
        if critical { der::seq(&[&der::oid(o), &der::boolean(true), &der::octets(value)]) } else { der::seq(&[&der::oid(o), &der::octets(value)]) }
    };
    // Bit 0 (digitalSignature) and bit 1 (nonRepudiation) set: eligible under
    // `Certificate::signing_problem`.
    let key_usage = der::tlv(tag::BIT_STRING, &[6, 0b1100_0000]);
    let extensions = der::seq(&[
        &ext("2.5.29.15", true, &key_usage),
        &ext("2.5.29.14", false, &der::octets(&ski)),
        &ext("2.5.29.37", false, &der::seq(&[&der::oid("1.3.6.1.5.5.7.3.4"), &der::oid("1.2.840.113583.1.1.5")])),
    ]);
    let tbs = der::seq(&[
        &der::explicit(0, &der::int(2)),
        &der::uint(serial),
        &sig_alg,
        &dn.raw,
        &der::seq(&[&from.encode(), &until.encode()]),
        &dn.raw,
        &spki,
        &der::explicit(3, &extensions),
    ]);
    let signature = signer.sign(alg, &tbs).unwrap();
    let cert = der::seq(&[&tbs, &sig_alg, &der::bit_string(&signature)]);
    Certificate::parse(&cert).unwrap()
}

const SOFTHSM_LIB: &str = "/usr/lib/softhsm/libsofthsm2.so";
const PIN: &str = "123456";
const SO_PIN: &str = "12345678";

fn tool_available(name: &str) -> bool {
    Command::new(name).arg("--help").output().is_ok()
}

fn fixture() -> Vec<u8> {
    let objs: Vec<&[u8]> = vec![
        b"<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [6 0 R] >> >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> /Annots [6 0 R] >>",
        b"<< /Length 44 >>\nstream\nBT /F1 14 Tf 20 250 Td (Contract text) Tj ET\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        b"<< /Type /Annot /Subtype /Widget /FT /Sig /T (Approval) /Rect [150 20 280 70] /P 3 0 R /F 4 >>",
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn opts() -> SignOptions {
    SignOptions { page: 0, rect: Some([20.0, 100.0, 220.0, 150.0]), date: "D:20261002120000+01'00'".into(), ..SignOptions::default() }
}

/// A signer used only to bootstrap certificates (see [`provision_certificate`]): always a key
/// without `CKA_ALWAYS_AUTHENTICATE`, so a plain single-part `Session::sign` suffices — no need
/// for the Init/ContextSpecific-login/Update/Final sequence the production `Pkcs11Key` uses.
struct BootstrapKey {
    session: Mutex<Session>,
    handle: ObjectHandle,
    public: PublicKey,
}

impl ExternalKey for BootstrapKey {
    fn sign(&self, alg: DigestAlg, msg: &[u8]) -> Result<Vec<u8>, SignError> {
        let session = self.session.lock().map_err(|_| SignError::Crypto("lock poisoned".into()))?;
        let hash = alg.digest(&[msg]);
        match &self.public {
            PublicKey::Rsa { .. } => {
                let info = der::seq(&[&der::algorithm(alg.oid(), Some(&der::null())), &der::octets(&hash)]);
                session.sign(&Mechanism::RsaPkcs, self.handle, &info).map_err(|e| SignError::Crypto(e.to_string()))
            }
            PublicKey::P256(_) => {
                let raw = session.sign(&Mechanism::Ecdsa, self.handle, &hash).map_err(|e| SignError::Crypto(e.to_string()))?;
                let sig = p256::ecdsa::Signature::from_slice(&raw).map_err(|e| SignError::Crypto(e.to_string()))?;
                Ok(sig.to_der().as_bytes().to_vec())
            }
            PublicKey::P384(_) => Err(SignError::Unsupported("P-384 unused in this test".into())),
            other => Err(SignError::Unsupported(format!("{} unused in this test", other.describe()))),
        }
    }
}

/// `CKA_EC_POINT` on SoftHSM2 is the DER `OCTET STRING` wrapping the SEC1 point, not the raw
/// point PdfCraft expects from an SPKI; unwrap it defensively (a bare point is accepted too, in
/// case another token gives one).
fn ec_point_from_attribute(raw: &[u8]) -> Vec<u8> {
    if raw.len() >= 2 && raw[0] == 0x04 && raw[1] as usize == raw.len() - 2 { raw[2..].to_vec() } else { raw.to_vec() }
}

fn public_key_of(session: &Session, handle: ObjectHandle) -> PublicKey {
    let attrs = session.get_attributes(handle, &[AttributeType::Modulus, AttributeType::PublicExponent, AttributeType::EcPoint]).unwrap();
    let (mut n, mut e, mut ec) = (None, None, None);
    for a in attrs {
        match a {
            Attribute::Modulus(v) => n = Some(v),
            Attribute::PublicExponent(v) => e = Some(v),
            Attribute::EcPoint(v) => ec = Some(v),
            _ => {}
        }
    }
    match (n, e, ec) {
        (Some(n), Some(e), _) => PublicKey::Rsa { n, e },
        (_, _, Some(ec)) => PublicKey::P256(ec_point_from_attribute(&ec)),
        _ => panic!("object has neither RSA nor EC public attributes"),
    }
}

fn token_key(session: &Session, id: &str) -> (ObjectHandle, ObjectHandle, PublicKey, bool) {
    let id_bytes = (0..id.len()).step_by(2).map(|i| u8::from_str_radix(&id[i..i + 2], 16).unwrap()).collect::<Vec<u8>>();
    let pub_handle =
        session.find_objects(&[Attribute::Class(ObjectClass::PUBLIC_KEY), Attribute::Id(id_bytes.clone())]).unwrap().into_iter().next().unwrap();
    let priv_handle =
        session.find_objects(&[Attribute::Class(ObjectClass::PRIVATE_KEY), Attribute::Id(id_bytes)]).unwrap().into_iter().next().unwrap();
    let public = public_key_of(session, pub_handle);
    let always_authenticate = session
        .get_attributes(priv_handle, &[AttributeType::AlwaysAuthenticate])
        .unwrap()
        .into_iter()
        .find_map(|a| match a {
            Attribute::AlwaysAuthenticate(b) => Some(b),
            _ => None,
        })
        .unwrap_or(false);
    (pub_handle, priv_handle, public, always_authenticate)
}

/// Builds a certificate for the key pair `subject_id`'s public key, signed by `signer_id`'s
/// private key (the same id for a true self-signed certificate), and writes it onto the token
/// as a `CKA_CERTIFICATE` object with `subject_id`, so [`pkcs11::list`] and [`pkcs11::open`] can
/// find it the same way they would a PIV card's factory-issued one.
///
/// Signing with a *different* key than the subject's is only used to provision the
/// always-authenticate key below: that key's own signature can't be used to bootstrap its own
/// certificate on SoftHSM2 (see [`roundtrip_body`]), so a non-reauthenticating key signs on its
/// behalf instead. The result isn't a real self-signed certificate (issuer != subject key), but
/// listing and opening the identity don't check that — only an attempted signing operation would.
fn provision_certificate(dir: &Path, token: &str, subject_id: &str, signer_id: &str, cn: &str) -> Certificate {
    let pkcs11tool = |args: &[&str]| {
        let out = Command::new("pkcs11-tool").args(["--module", SOFTHSM_LIB, "--token-label", token]).args(args).output().unwrap();
        assert!(out.status.success(), "pkcs11-tool {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };

    let ctx = Pkcs11::new(SOFTHSM_LIB).unwrap();
    ctx.initialize(CInitializeArgs::OsThreads).unwrap();
    let slot = ctx.get_slots_with_token().unwrap().into_iter().find(|s| ctx.get_token_info(*s).is_ok_and(|i| i.label() == token)).unwrap();
    let session = ctx.open_rw_session(slot).unwrap();
    session.login(UserType::User, Some(&AuthPin::new(PIN.into()))).unwrap();

    let (_, _, subject_public, _) = token_key(&session, subject_id);
    let (_, signer_priv, signer_public, signer_always_auth) = token_key(&session, signer_id);
    assert!(!signer_always_auth, "{signer_id}: the bootstrap signer must not itself need re-authentication");
    let signer_key = PrivateKey::external(
        signer_public.clone(),
        Arc::new(BootstrapKey { session: Mutex::new(session), handle: signer_priv, public: signer_public }),
    );

    let dn = Name::build(cn, "", "PdfCraft tests", "", "");
    let serial = DigestAlg::Sha256.digest(&[cn.as_bytes()])[..8].to_vec();
    let cert = issue_certificate(&dn, &subject_public, &signer_key, Time::from_unix(1_760_000_000), 5, &serial);

    let cert_path = dir.join(format!("{subject_id}.der"));
    std::fs::write(&cert_path, &cert.raw).unwrap();
    pkcs11tool(&["--login", "--pin", PIN, "--write-object", cert_path.to_str().unwrap(), "--type", "cert", "--id", subject_id, "--label", cn]);
    cert
}

struct Token {
    dir: PathBuf,
    label: String,
}

impl Token {
    /// Assumes `SOFTHSM2_CONF` is already set in this process's environment (by the parent, in
    /// [`pkcs11_signing_roundtrip`], before it re-exec'd into the child that runs this).
    fn init(label: &str, dir: PathBuf) -> Token {
        let out =
            Command::new("softhsm2-util").args(["--init-token", "--free", "--label", label, "--so-pin", SO_PIN, "--pin", PIN]).output().unwrap();
        assert!(out.status.success(), "softhsm2-util --init-token: {}", String::from_utf8_lossy(&out.stderr));
        Token { dir, label: label.to_string() }
    }

    fn keypairgen(&self, key_type: &str, id: &str, label: &str, always_auth: bool) {
        let mut args = vec![
            "--module",
            SOFTHSM_LIB,
            "--token-label",
            &self.label,
            "--login",
            "--pin",
            PIN,
            "--keypairgen",
            "--key-type",
            key_type,
            "--id",
            id,
            "--label",
            label,
            "--usage-sign",
        ];
        if always_auth {
            args.push("--always-auth");
        }
        let out = Command::new("pkcs11-tool").args(&args).output().unwrap();
        assert!(out.status.success(), "keypairgen {key_type}: {}", String::from_utf8_lossy(&out.stderr));
    }
}

fn open_doc(b: &[u8]) -> Document {
    Document::open(Arc::new(b.to_vec())).unwrap()
}

/// `SOFTHSM2_CONF` has to be set before anything in this process (including SoftHSM2 itself, via
/// `Pkcs11::new`/`initialize`) reads it; mutating it afterwards needs `std::env::set_var`, which
/// is `unsafe` and forbidden in this workspace. So the test with no marker set re-execs itself as
/// a child with the variable set on `Command` (safe: that only touches the child's environment,
/// inherited at process start, never mutated concurrently), and the child — marked by the env var
/// below — runs the real body.
const CHILD_MARKER: &str = "PDFCRAFT_PKCS11_TEST_CHILD";

#[test]
fn pkcs11_signing_roundtrip() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        return roundtrip_body();
    }
    if !tool_available("softhsm2-util") || !tool_available("pkcs11-tool") || !Path::new(SOFTHSM_LIB).exists() {
        eprintln!("skipping: softhsm2-util, pkcs11-tool or {SOFTHSM_LIB} not available");
        return;
    }
    let dir = std::env::temp_dir().join(format!("pdfcraft-softhsm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tokens")).unwrap();
    let conf = dir.join("softhsm2.conf");
    std::fs::write(&conf, format!("directories.tokendir = {}\nobjectstore.backend = file\nlog.level = ERROR\n", dir.join("tokens").display()))
        .unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "pkcs11_signing_roundtrip"])
        .env("SOFTHSM2_CONF", &conf)
        .env(CHILD_MARKER, "1")
        .status()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.success(), "child process failed");
}

fn roundtrip_body() {
    let dir = PathBuf::from(std::env::var("SOFTHSM2_CONF").unwrap()).parent().unwrap().to_path_buf();
    let token = Token::init("pdfcraft-test", dir);
    token.keypairgen("rsa:2048", "01", "rsa-sig", false);
    token.keypairgen("EC:prime256v1", "02", "ec-sig", false);
    token.keypairgen("EC:prime256v1", "03", "ec-sig-reauth", true);
    let rsa_cert = provision_certificate(&token.dir, &token.label, "01", "01", "PdfCraft RSA test");
    let ec_cert = provision_certificate(&token.dir, &token.label, "02", "02", "PdfCraft EC test");
    // Key 03 needs a re-authenticating C_Sign to make its own certificate, which SoftHSM2 can't
    // do (see the comment on `provision_certificate`); key 01 signs on its behalf instead.
    let reauth_cert = provision_certificate(&token.dir, &token.label, "03", "01", "PdfCraft EC reauth test");

    let module = Some(Path::new(SOFTHSM_LIB));
    let listing = pkcs11::list(module).unwrap();
    assert_eq!(listing.ids.len(), 3, "{:?}", listing.ids);
    assert!(listing.unusable.is_empty(), "{:?}", listing.unusable);
    let ids = listing.ids;

    // RSA and plain EC: a full sign-and-validate round trip through the real PKCS #11 path.
    for serial in [&rsa_cert.serial, &ec_cert.serial] {
        let entry = ids.iter().find(|i| &i.certificate.serial == serial).unwrap();
        assert!(!entry.protected_authentication_path);
        let id = pkcs11::open(module, &entry.reference, Some(PIN.to_string()), &[]).unwrap();
        assert!(id.key.is_external());
        let signed = pdfcraft_sign::sign(&open_doc(&fixture()), &id, &opts()).unwrap();
        let trusted = signatures(&open_doc(&signed), &signed, &TrustStore { certs: vec![id.certificate.clone()] });
        let s = trusted.iter().find(|s| s.signed).unwrap();
        assert_eq!(s.status, Status::Valid, "{:?}", s.details);
    }

    // The always-authenticate key: open() (plain C_Login, no signing yet) must still succeed,
    // proving CKA_ALWAYS_AUTHENTICATE was read and the identity is otherwise usable. The actual
    // sign then runs PdfCraft's real Init/ContextSpecific-login/Update/Final sequence (PKCS #11
    // §5.6.4) — SoftHSM2 doesn't support multi-part RSA/ECDSA signing at all (confirmed
    // separately), so it can't carry that through, but the result must be a clean error, never a
    // panic. The sequence itself is exercised for real against the user's PIV card.
    let entry = ids.iter().find(|i| i.certificate.serial == reauth_cert.serial).unwrap();
    let id = pkcs11::open(module, &entry.reference, Some(PIN.to_string()), &[]).unwrap();
    assert!(id.key.is_external());
    let result = pdfcraft_sign::sign(&open_doc(&fixture()), &id, &opts());
    assert!(result.is_err(), "expected SoftHSM2's multi-part limitation to surface as an error, got Ok");
    // Drop the token session this `DigitalId` is holding open before starting another one below:
    // SoftHSM2 (like most PKCS #11 modules) refuses a second `C_Initialize` while an earlier one
    // in this process hasn't been finalized yet, which only happens once every live session from
    // it is dropped.
    drop(id);

    let wrong_pin = pkcs11::open(module, &ids[0].reference, Some("000000".to_string()), &[]);
    assert!(matches!(wrong_pin, Err(SignError::WrongPassword)), "{wrong_pin:?}");
}
