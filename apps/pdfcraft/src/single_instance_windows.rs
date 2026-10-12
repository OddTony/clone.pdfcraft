//! Safe bindings from packages already used by the desktop app. No reusable key files.
use super::{MAX_FRAME, MAX_REQUEST, Nonce, check_deadline, invalid};
use std::io;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use windows::Security::Cryptography::{CryptographicBuffer, DataProtection::DataProtectionProvider};
use windows::Storage::Streams::IBuffer;

pub(super) struct Protection {
    encrypt: DataProtectionProvider,
    decrypt: DataProtectionProvider,
    // Keep the COM apartment alive through the provider calls; workers use their own guards.
    _apartment: winsafe::guard::CoUninitializeGuard,
}

impl Protection {
    pub(super) fn new() -> io::Result<Self> {
        #[cfg(test)]
        eprintln!("protection: before COM initialization on {:?}", std::thread::current().id());
        let apartment = winsafe::CoInitializeEx(winsafe::co::COINIT::MULTITHREADED).map_err(io::Error::other)?;
        #[cfg(test)]
        eprintln!("protection: COM result {:?}; before provider activation", apartment.hr());
        Ok(Self {
            encrypt: DataProtectionProvider::CreateOverloadExplicit(&"LOCAL=user".into()).map_err(io::Error::other)?,
            decrypt: DataProtectionProvider::new().map_err(io::Error::other)?,
            _apartment: apartment,
        })
    }

    pub(super) fn nonce(&self) -> io::Result<Nonce> {
        let bytes = copy(&CryptographicBuffer::GenerateRandom(32).map_err(io::Error::other)?, 32)?;
        bytes.try_into().map_err(|_| invalid("invalid random challenge"))
    }

    pub(super) fn protect(&self, bytes: &[u8], deadline: Instant, stop: &AtomicBool) -> io::Result<Vec<u8>> {
        if bytes.len() > MAX_REQUEST {
            return Err(invalid("open plaintext is too large"));
        }
        check_deadline(deadline, stop)?;
        let buffer = CryptographicBuffer::CreateFromByteArray(bytes).map_err(io::Error::other)?;
        let operation = self.encrypt.ProtectAsync(&buffer).map_err(io::Error::other)?;
        // Poll the async status instead of joining indefinitely. Cancel on total deadline/stop.
        while operation.Status().map_err(io::Error::other)?.0 == 0 {
            if let Err(e) = check_deadline(deadline, stop) {
                let _ = operation.Cancel();
                return Err(e);
            }
            std::thread::sleep(super::POLL);
        }
        check_deadline(deadline, stop)?;
        copy(&operation.GetResults().map_err(io::Error::other)?, MAX_FRAME)
    }

    pub(super) fn unprotect(&self, bytes: &[u8], deadline: Instant, stop: &AtomicBool) -> io::Result<Vec<u8>> {
        if bytes.len() > MAX_FRAME {
            return Err(invalid("open ciphertext is too large"));
        }
        check_deadline(deadline, stop)?;
        let buffer = CryptographicBuffer::CreateFromByteArray(bytes).map_err(io::Error::other)?;
        let operation = self.decrypt.UnprotectAsync(&buffer).map_err(io::Error::other)?;
        while operation.Status().map_err(io::Error::other)?.0 == 0 {
            if let Err(e) = check_deadline(deadline, stop) {
                let _ = operation.Cancel();
                return Err(e);
            }
            std::thread::sleep(super::POLL);
        }
        check_deadline(deadline, stop)?;
        copy(&operation.GetResults().map_err(io::Error::other)?, MAX_REQUEST)
    }
}

fn copy(buffer: &IBuffer, cap: usize) -> io::Result<Vec<u8>> {
    if buffer.Length().map_err(io::Error::other)? as usize > cap {
        return Err(invalid("open buffer is too large"));
    }
    let mut bytes = windows::core::Array::new();
    CryptographicBuffer::CopyToByteArray(buffer, &mut bytes).map_err(io::Error::other)?;
    Ok(bytes.to_vec())
}
