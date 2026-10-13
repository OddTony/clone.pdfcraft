//! PKCS #11 tokens and smart cards (plan: `sign.pkcs11`).
//!
//! The first step is deliberately small and has no hardware in it: the reference a token
//! identity is kept under (`pkcs11:<SHA-256 of the certificate DER>`, like the
//! `windows:` and `keychain:` references) and the `DigestInfo` a `CKM_RSA_PKCS` signature
//! is computed over. Module loading (explicit user-chosen paths only), slot listing and
//! signing through `cryptoki` come later; nothing here opens a DLL.

use crate::keys::DigestAlg;
use crate::{Certificate, SignError};

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
}
