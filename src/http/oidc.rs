//! OpenID Connect against authentik. The client is discovered once at
//! startup; the login flow keeps its state in a private cookie so the server
//! stays stateless.

use anyhow::{Context, Result};
use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreAuthenticationFlow, CoreErrorResponseType,
    CoreGenderClaim, CoreJsonWebKey, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
    CoreProviderMetadata, CoreRevocableToken, CoreRevocationErrorResponse,
    CoreTokenIntrospectionResponse, CoreTokenType,
};
use openidconnect::{
    AdditionalClaims, AuthorizationCode, Client, ClientId, ClientSecret, CsrfToken,
    EmptyExtraTokenFields, EndpointMaybeSet, EndpointNotSet, EndpointSet, IdTokenFields, IssuerUrl,
    Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, StandardErrorResponse,
    StandardTokenResponse, TokenResponse,
};
use serde::{Deserialize, Serialize};

use crate::config::OidcConfig;

/// authentik puts group names in the `groups` claim of the ID token.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupsClaims {
    #[serde(default)]
    pub groups: Vec<String>,
}

impl AdditionalClaims for GroupsClaims {}

pub type GroupsTokenResponse = StandardTokenResponse<
    IdTokenFields<
        GroupsClaims,
        EmptyExtraTokenFields,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm,
    >,
    CoreTokenType,
>;

pub type GroupsClient<
    HasAuthUrl = EndpointSet,
    HasDeviceAuthUrl = EndpointNotSet,
    HasIntrospectionUrl = EndpointNotSet,
    HasRevocationUrl = EndpointNotSet,
    HasTokenUrl = EndpointMaybeSet,
    HasUserInfoUrl = EndpointMaybeSet,
> = Client<
    GroupsClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    GroupsTokenResponse,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    HasAuthUrl,
    HasDeviceAuthUrl,
    HasIntrospectionUrl,
    HasRevocationUrl,
    HasTokenUrl,
    HasUserInfoUrl,
>;

pub struct OidcClient {
    client: GroupsClient,
    http: reqwest::Client,
    pub group: Option<String>,
}

/// What the login handler stores in the private cookie between redirect and
/// callback.
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginState {
    pub csrf: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

pub struct Identity {
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub groups: Vec<String>,
}

pub fn http_client() -> Result<reqwest::Client> {
    // Following redirects opens the client up to SSRF; the issuer never needs them.
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()?)
}

pub async fn discover(cfg: &OidcConfig, public_url: &url::Url) -> Result<OidcClient> {
    let http = http_client()?;
    let issuer = IssuerUrl::new(cfg.issuer.clone()).context("KORYTO_OIDC_ISSUER")?;
    let metadata = CoreProviderMetadata::discover_async(issuer, &http)
        .await
        .with_context(|| format!("OIDC discovery at {}", cfg.issuer))?;
    let redirect = public_url
        .join("/api/auth/callback")
        .context("redirect URI")?;
    let client = GroupsClient::from_provider_metadata(
        metadata,
        ClientId::new(cfg.client_id.clone()),
        Some(ClientSecret::new(cfg.client_secret.clone())),
    )
    .set_redirect_uri(RedirectUrl::from_url(redirect));
    Ok(OidcClient {
        client,
        http,
        group: cfg.group.clone(),
    })
}

impl OidcClient {
    /// The URL to send the browser to, plus what to remember for the callback.
    pub fn authorize(&self) -> (url::Url, LoginState) {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, csrf, nonce) = self
            .client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new("email".into()))
            .add_scope(Scope::new("profile".into()))
            .set_pkce_challenge(challenge)
            .url();
        let state = LoginState {
            csrf: csrf.secret().clone(),
            nonce: nonce.secret().clone(),
            pkce_verifier: verifier.secret().clone(),
        };
        (url, state)
    }

    pub async fn exchange(&self, code: &str, state: &LoginState) -> Result<Identity> {
        let response = self
            .client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .context("token endpoint not discovered")?
            .set_pkce_verifier(PkceCodeVerifier::new(state.pkce_verifier.clone()))
            .request_async(&self.http)
            .await
            .context("exchanging the authorization code")?;
        let id_token = response
            .id_token()
            .context("no ID token in the token response")?;
        let nonce = Nonce::new(state.nonce.clone());
        let claims = id_token
            .claims(&self.client.id_token_verifier(), &nonce)
            .context("verifying the ID token")?;
        Ok(Identity {
            subject: claims.subject().to_string(),
            email: claims.email().map(|e| e.to_string()),
            name: claims
                .name()
                .and_then(|n| n.get(None))
                .map(|n| n.to_string())
                .or_else(|| claims.preferred_username().map(|u| u.to_string())),
            groups: claims.additional_claims().groups.clone(),
        })
    }
}
