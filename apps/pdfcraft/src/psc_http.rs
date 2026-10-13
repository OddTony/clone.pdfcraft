//! HTTPS for ICP-Brasil cloud certificates (`pdfcraft_sign::psc::Transport`): ureq with the
//! system's trusted certificates, as the update check uses. Error statuses are read like any other
//! answer, since providers explain them in the JSON body.

use std::time::Duration;

use pdfcraft_engine::sign::psc::{KNOWN, Provider, Transport};

/// The redirect URI registered with every provider: PdfCraft listens there for the code.
const REDIRECT_URI: &str = "http://127.0.0.1:43821/callback";

/// Client credentials set at build time, by provider name: `PDFCRAFT_PSC_<NAME>_CLIENT_ID` and
/// `PDFCRAFT_PSC_<NAME>_CLIENT_SECRET`. A provider without a client ID isn't offered.
const CREDENTIALS: &[(&str, Option<&str>, Option<&str>)] =
    &[("Certisign", option_env!("PDFCRAFT_PSC_CERTISIGN_CLIENT_ID"), option_env!("PDFCRAFT_PSC_CERTISIGN_CLIENT_SECRET"))];

/// The known providers this build is registered with.
pub fn registered() -> Vec<Provider> {
    KNOWN
        .iter()
        .filter_map(|(name, base)| {
            let (_, id, secret) = CREDENTIALS.iter().find(|(n, _, _)| n == name)?;
            Some(Provider {
                name: (*name).to_string(),
                base: (*base).to_string(),
                client_id: (*id)?.to_string(),
                client_secret: secret.unwrap_or_default().to_string(),
                redirect_uri: REDIRECT_URI.to_string(),
            })
        })
        .collect()
}

pub struct Https {
    agent: ureq::Agent,
}

impl Https {
    pub fn new() -> Result<Self, String> {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .tls_config(ureq::tls::TlsConfig::builder().root_certs(crate::updates::os_roots()?).build())
            .build()
            .new_agent();
        Ok(Self { agent })
    }
}

const USER_AGENT: &str = concat!("PdfCraft/", env!("CARGO_PKG_VERSION"));

impl Transport for Https {
    fn get(&self, url: &str, bearer: &str) -> Result<String, String> {
        let mut response = self
            .agent
            .get(url)
            .header("Authorization", &format!("Bearer {bearer}"))
            .header("Accept", "application/json")
            .header("User-Agent", USER_AGENT)
            .call()
            .map_err(|e| e.to_string())?;
        response.body_mut().read_to_string().map_err(|e| e.to_string())
    }

    fn post_form(&self, url: &str, form: &str) -> Result<String, String> {
        let mut response = self
            .agent
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .header("User-Agent", USER_AGENT)
            .send(form)
            .map_err(|e| e.to_string())?;
        response.body_mut().read_to_string().map_err(|e| e.to_string())
    }

    fn post_json(&self, url: &str, bearer: &str, json: &str) -> Result<String, String> {
        let mut response = self
            .agent
            .post(url)
            .header("Authorization", &format!("Bearer {bearer}"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("User-Agent", USER_AGENT)
            .send(json)
            .map_err(|e| e.to_string())?;
        response.body_mut().read_to_string().map_err(|e| e.to_string())
    }
}
