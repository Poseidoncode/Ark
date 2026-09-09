//! GitHub OAuth Device Flow implementation.
//!
//! This module implements the GitHub Device Flow (RFC 8628) for authenticating
//! users without embedding a client secret. The flow:
//!
//! 1. POST to `https://github.com/login/device/code` with `client_id` and `scope`
//!    → returns `device_code`, `user_code`, `verification_uri`, `interval`, `expires_in`
//! 2. User opens `verification_uri` in a browser and enters the `user_code`
//! 3. App polls `https://github.com/login/oauth/access_token` with
//!    `client_id` and `device_code` at the specified `interval`
//! 4. Once approved, the endpoint returns `access_token`
//!
//! The token is stored securely in the OS keychain via `CredentialStore`.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The default OAuth App client ID registered with GitHub.
/// Device Flow does NOT require a client secret.
///
/// This is bundled into the app so users don't need to configure anything.
/// Forks or contributors can override it at build time with the
/// `ARK_GITHUB_CLIENT_ID` environment variable.
const DEFAULT_CLIENT_ID: &str = "Ov23liJ1Di4lxURPBID6";

/// The OAuth App client ID used at runtime.
/// Falls back to `DEFAULT_CLIENT_ID` if no env var is set at build time.
fn github_client_id() -> &'static str {
    option_env!("ARK_GITHUB_CLIENT_ID").unwrap_or(DEFAULT_CLIENT_ID)
}

/// Whether GitHub OAuth is enabled (client ID was configured).
pub fn oauth_enabled() -> bool {
    !github_client_id().is_empty()
}

/// Scopes requested for the token. `repo` covers public + private repos.
/// `read:user` lets us fetch the authenticated user's name/email for settings.
const GITHUB_SCOPES: &str = "repo read:user";

/// Keychain entry name for the OAuth token.
pub const TOKEN_KEYCHAIN_ID: &str = "ark:github:oauth-token";

// ── Response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    #[allow(dead_code)]
    interval: Option<u64>,
}

/// Information returned to the frontend when starting the device flow.
/// The frontend uses this to display the user code and open the browser.
#[derive(Debug, Serialize, Clone)]
pub struct DeviceFlowInfo {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub interval: u64,
    pub expires_in: u64,
}

/// Information about the authenticated GitHub user.
#[derive(Debug, Serialize, Clone)]
pub struct GitHubUser {
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Errors specific to the OAuth flow.
#[derive(Debug)]
pub enum OAuthError {
    Network(String),
    /// GitHub returned an authentication error (401/403) — the token is invalid.
    Auth,
    /// The device code expired before the user approved.
    Expired,
    /// The user denied the authorization request.
    Denied,
    /// The polling was cancelled by the caller.
    Cancelled,
    /// GitHub returned an unexpected response.
    Parse(String),
    Other(String),
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OAuthError::Network(msg) => write!(f, "Network error: {}", msg),
            OAuthError::Auth => write!(f, "Authentication failed — token is invalid or expired."),
            OAuthError::Expired => write!(f, "Device code expired. Please try again."),
            OAuthError::Denied => write!(f, "Authorization request was denied."),
            OAuthError::Cancelled => write!(f, "Authentication cancelled."),
            OAuthError::Parse(msg) => write!(f, "Failed to parse response: {}", msg),
            OAuthError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<OAuthError> for String {
    fn from(err: OAuthError) -> String {
        err.to_string()
    }
}

// ── HTTP helpers (using std + ureq-free approach via subprocess curl) ──
//
// We avoid adding a heavy HTTP client dependency by using `curl` which is
// available on macOS, most Linux distros, and can be bundled on Windows.
// All requests use `-sS` (silent + show errors) and JSON content type.

/// Execute a curl POST request with form-urlencoded body and return the response body.
fn curl_post_form(url: &str, body: &[(&str, &str)]) -> Result<String, OAuthError> {
    use std::process::Command;

    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-f", "-X", "POST", url]);
    cmd.args(["-H", "Accept: application/json"]);
    cmd.args(["-H", "Content-Type: application/x-www-form-urlencoded"]);

    for (key, value) in body {
        cmd.args(["--data-urlencode", &format!("{}={}", key, value)]);
    }

    // Set a reasonable timeout
    cmd.args(["--connect-timeout", "10", "--max-time", "30"]);

    let output = cmd
        .output()
        .map_err(|e| OAuthError::Network(format!("Failed to execute curl: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OAuthError::Network(format!(
            "curl failed: {}",
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a command with piped stdin and capture both output streams.
fn run_command_with_input(
    mut cmd: std::process::Command,
    input: &[u8],
) -> Result<std::process::Output, OAuthError> {
    use std::io::Write;

    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| OAuthError::Network(format!("Failed to execute curl: {}", e)))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input)
            .map_err(|e| OAuthError::Network(format!("Failed to write curl config: {}", e)))?;
    }

    child
        .wait_with_output()
        .map_err(|e| OAuthError::Network(format!("Failed to wait for curl: {}", e)))
}

/// Execute a curl GET request with an Authorization header.
/// The header is passed via stdin (`--config -`) to keep the token out of
/// the process argument list (`ps`).
fn curl_get_auth(url: &str, token: &str) -> Result<String, OAuthError> {
    use std::process::Command;

    let config = format!(
        r#"-H "Accept: application/vnd.github+json"
-H "Authorization: Bearer {}"
"#,
        token
    );

    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-f", "--config", "-", url]);
    cmd.args(["--connect-timeout", "10", "--max-time", "30"]);
    let output = run_command_with_input(cmd, config.as_bytes())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_trim = stderr.trim();
        // curl with -f exits non-zero on 4xx/5xx. Distinguish 401/403
        // (auth errors) from other HTTP failures so callers can react.
        if stderr_trim.contains("401") || stderr_trim.contains("403") {
            return Err(OAuthError::Auth);
        }
        return Err(OAuthError::Network(format!("curl failed: {}", stderr_trim)));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ── Public API ──────────────────────────────────────────────────

/// Step 1: Request a device code from GitHub.
///
/// Returns the device code, user code, and verification URI to display
/// to the user. The frontend should then open the verification URI in
/// a browser and start polling for the token.
pub fn request_device_code() -> Result<DeviceFlowInfo, OAuthError> {
    if !oauth_enabled() {
        return Err(OAuthError::Other(
            "GitHub OAuth is not configured. The app developer must set ARK_GITHUB_CLIENT_ID at build time.".to_string()
        ));
    }

    let body = curl_post_form(
        "https://github.com/login/device/code",
        &[("client_id", github_client_id()), ("scope", GITHUB_SCOPES)],
    )?;

    let resp: DeviceCodeResponse = serde_json::from_str(&body)
        .map_err(|e| OAuthError::Parse(format!("Failed to parse device code response: {}", e)))?;

    Ok(DeviceFlowInfo {
        user_code: resp.user_code,
        verification_uri: resp.verification_uri,
        device_code: resp.device_code,
        interval: resp.interval,
        expires_in: resp.expires_in,
    })
}

/// Step 2: Poll GitHub for an access token.
///
/// This function blocks and polls at the specified interval until:
/// - The token is granted (returns `Ok(token)`)
/// - The device code expires (returns `Err(Expired)`)
/// - The user denies the request (returns `Err(Denied)`)
/// - A network error occurs (returns `Err(Network)`)
///
/// The `should_cancel` callback is checked between polls; if it returns
/// `true`, polling stops with `Err(Cancelled)`.
pub fn poll_for_token(
    device_code: &str,
    interval: u64,
    expires_in: u64,
    should_cancel: &dyn Fn() -> bool,
) -> Result<String, OAuthError> {
    if device_code.trim().is_empty() || device_code.len() > 1024 {
        return Err(OAuthError::Other("Invalid device code".to_string()));
    }
    let poll_interval = Duration::from_secs(interval.clamp(1, 10));
    let deadline = std::time::Instant::now() + Duration::from_secs(expires_in.clamp(60, 1800));
    let mut current_interval = poll_interval;

    loop {
        if should_cancel() {
            return Err(OAuthError::Cancelled);
        }

        if std::time::Instant::now() > deadline {
            return Err(OAuthError::Expired);
        }

        let body = curl_post_form(
            "https://github.com/login/oauth/access_token",
            &[
                ("client_id", github_client_id()),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )?;

        let resp: TokenResponse = serde_json::from_str(&body)
            .map_err(|e| OAuthError::Parse(format!("Failed to parse token response: {}", e)))?;

        if let Some(token) = resp.access_token {
            return Ok(token);
        }

        match resp.error.as_deref() {
            Some("authorization_pending") => {
                // User hasn't approved yet — keep polling
                std::thread::sleep(current_interval);
            }
            Some("slow_down") => {
                current_interval = (current_interval + Duration::from_secs(5)).min(Duration::from_secs(30));
                std::thread::sleep(current_interval);
            }
            Some("expired_token") => return Err(OAuthError::Expired),
            Some("access_denied") => return Err(OAuthError::Denied),
            Some(err) => {
                return Err(OAuthError::Other(format!(
                    "GitHub returned error: {} — {}",
                    err,
                    resp.error_description.unwrap_or_default()
                )));
            }
            None => {
                return Err(OAuthError::Other(
                    "GitHub returned no token and no error".to_string(),
                ));
            }
        }
    }
}

/// Fetch the authenticated user's profile from GitHub API.
/// Returns `OAuthError::Auth` if the token is invalid (401/403).
pub fn fetch_github_user(token: &str) -> Result<GitHubUser, OAuthError> {
    #[derive(Deserialize)]
    struct ApiUser {
        login: String,
        name: Option<String>,
        email: Option<String>,
    }

    let body = curl_get_auth("https://api.github.com/user", token)?;

    // Guard against an empty response body before parsing. Without this,
    // serde_json reports "EOF while parsing a value at line 1 column 0",
    // which is opaque and unrelated to the actual cause.
    if body.trim().is_empty() {
        return Err(OAuthError::Network(
            "GitHub returned an empty response body".to_string(),
        ));
    }

    // Check for GitHub auth error in the response body.
    // With -f, curl returns non-zero on 4xx/5xx, so the body is empty
    // and curl_get_auth already returned an error. But if -f is not
    // supported (older curl), the body may contain the error JSON.
    if body.contains("Bad credentials") || body.contains("Requires authentication") {
        return Err(OAuthError::Auth);
    }

    let user: ApiUser = serde_json::from_str(&body)
        .map_err(|e| OAuthError::Parse(format!("Failed to parse user response: {}", e)))?;

    Ok(GitHubUser {
        login: user.login,
        name: user.name,
        email: user.email,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_error_display() {
        assert!(OAuthError::Expired.to_string().contains("expired"));
        assert!(OAuthError::Denied.to_string().contains("denied"));
        assert!(OAuthError::Cancelled.to_string().contains("cancelled"));
    }

    #[test]
    fn test_device_flow_info_serialization() {
        let info = DeviceFlowInfo {
            user_code: "ABCD-1234".to_string(),
            verification_uri: "https://github.com/login/device".to_string(),
            device_code: "dev123".to_string(),
            interval: 5,
            expires_in: 900,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("ABCD-1234"));
        assert!(json.contains("dev123"));
    }

    #[test]
    fn test_spawned_command_output_is_captured() {
        use std::process::Command;

        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.arg("--list");
        let output = run_command_with_input(cmd, b"").unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();

        assert!(output.status.success());
        assert!(stdout.contains("test_spawned_command_output_is_captured"));
    }
}
