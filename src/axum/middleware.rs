use axum::{
    extract::Query,
    http::{Request, Uri},
    response::{IntoResponse, Redirect, Response},
};
use futures_util::{future::BoxFuture, FutureExt};
use headers::{authorization::Bearer, Authorization, HeaderMapExt};
use serde::Deserialize;
use std::{
    sync::Arc,
    task::{Context, Poll},
};
use tower::{Layer, Service};
use tower_sessions::Session;

use crate::{idp::admin::IdpAdmin, oidc::keycloak::KeycloakOidcProvider, vault::VaultService, AiclIdentity};

use super::error::AppErrorHandler;

pub struct AuthenticateService<S> {
    identifier: Arc<KeycloakOidcProvider>,
    idp: Arc<IdpAdmin>,
    inner: S,
}

impl<S: Clone> Clone for AuthenticateService<S> {
    fn clone(&self) -> Self {
        Self {
            identifier: self.identifier.clone(),
            idp: self.idp.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<B, S> Service<Request<B>> for AuthenticateService<S>
where
    S: Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Response: IntoResponse + Send,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let error_handler = request.extensions().get::<AppErrorHandler>().expect("Error handler not found").clone();
        let inner = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, inner);
        let provider = self.identifier.clone();
        let idp = self.idp.clone();

        // Get the session from the request extensions
        let session = match request.extensions().get::<Session>() {
            Some(session) => session.clone(),
            None => panic!("Session not found in request extensions, layer this correctly"),
        };

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();
            match provider.authenticate(&mut parts, &session, &idp).await {
                Ok(()) => {},
                Err(error) => {
                    tracing::error!(%error, "Authentication failed");
                    return Ok(error_handler.handle_error(error));
                },
            }
            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

pub struct AuthenticateLayer {
    pub identifier: Arc<KeycloakOidcProvider>,
    pub idp: Arc<IdpAdmin>,
}

impl Clone for AuthenticateLayer {
    fn clone(&self) -> Self {
        Self {
            identifier: self.identifier.clone(),
            idp: self.idp.clone(),
        }
    }
}

impl<S: Clone> Layer<S> for AuthenticateLayer {
    type Service = AuthenticateService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthenticateService {
            identifier: self.identifier.clone(),
            idp: self.idp.clone(),
            inner,
        }
    }
}

/// Layer that applies the login enforcer middleware
pub struct LoginEnforcerLayer {
    pub identifier: Arc<KeycloakOidcProvider>,
}

impl Clone for LoginEnforcerLayer {
    fn clone(&self) -> Self {
        Self {
            identifier: self.identifier.clone(),
        }
    }
}

impl<S> tower::Layer<S> for LoginEnforcerLayer {
    type Service = LoginEnforcerMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LoginEnforcerMiddleware {
            identifier: self.identifier.clone(),
            inner,
        }
    }
}

/// The middleware service that enforces login
pub struct LoginEnforcerMiddleware<S> {
    identifier: Arc<KeycloakOidcProvider>,
    inner: S,
}

impl<S: Clone> Clone for LoginEnforcerMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            identifier: self.identifier.clone(),
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OidcQuery {
    code: String,
    state: String,
    #[allow(dead_code)]
    session_state: Option<String>,
}

impl<S, B> tower::Service<Request<B>> for LoginEnforcerMiddleware<S>
where
    S: tower::Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let error_handler = req.extensions().get::<AppErrorHandler>().expect("Error handler not found").clone();
        // Clone the inner service
        let mut inner = self.inner.clone();

        // Check if the user is identified, and return if they are.
        if let Some(_) = req.extensions().get::<AiclIdentity>() {
            return async move { inner.call(req).await }.boxed();
        }
        // They aren't identified. Get the session and create a redirect URI to send them to the oidc login page.
        let identifier = self.identifier.clone();

        // Get the session from the request extensions
        let session = match req.extensions().get::<Session>() {
            Some(session) => session.clone(),
            None => panic!("Session not found in request extensions, layer this correctly"),
        };

        // Extract the URI for potential redirection
        let uri = req.uri().clone();
        let redirect = strip_oidc_params(&uri);

        if let Some(Query(query)) = Query::<OidcQuery>::try_from_uri(&uri).ok() {
            return Box::pin(async move {
                match identifier
                    .handle_callback(&query.code, &query.state, &session, &redirect)
                    .await
                {
                    Ok(()) => Ok(Redirect::to(&redirect.to_string()).into_response()),
                    Err(e) => {
                        tracing::error!("Failed to start authentication: {}", e);
                        Ok(error_handler.handle_error(e))
                    }
                }
            });
        }

        Box::pin(async move {
            match identifier.start_auth(&session, &redirect).await {
                Ok(auth_uri) => Ok(Redirect::to(&auth_uri.to_string()).into_response()),
                Err(e) => {
                    tracing::error!("Failed to start authentication: {}", e);
                    Ok(error_handler.handle_error(e))
                }
            }
        })
    }
}

/// Strips OIDC-related parameters from a URI
///
/// Removes 'code', 'state', 'session_state', and other OIDC-related query parameters
fn strip_oidc_params(uri: &Uri) -> Uri {
    // If there's no query string, just return the original URI
    let query = match uri.query() {
        Some(q) => q,
        None => return uri.clone(),
    };

    // List of OIDC-related parameters to filter out
    let oidc_params = [
        "code",
        "state",
        "session_state",
        "iss",
        "id_token_hint",
        "post_logout_redirect_uri",
    ];

    // Parse and filter the query parameters
    let filtered_params: Vec<(&str, &str)> = query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.split('=');
            let key = parts.next()?;
            let value = parts.next().unwrap_or("");
            if oidc_params.contains(&key) {
                None
            } else {
                Some((key, value))
            }
        })
        .collect();

    // If no parameters remain after filtering, return the path-only URI
    if filtered_params.is_empty() {
        let path = uri.path();
        return path.parse().unwrap_or_else(|_| uri.clone());
    }

    // Reconstruct the URI with the filtered parameters
    let new_query = filtered_params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<String>>()
        .join("&");

    let mut parts = uri.to_string();
    if let Some(query_start) = parts.find('?') {
        parts.truncate(query_start);
        parts.push_str("?");
        parts.push_str(&new_query);
    }

    parts.parse().unwrap_or_else(|_| uri.clone())
}


pub struct ApiTokenAuthLayer {
    pub token_service: Arc<VaultService>,
    pub idp_admin: Arc<IdpAdmin>,
}

impl Clone for ApiTokenAuthLayer {
    fn clone(&self) -> Self {
        Self {
            token_service: self.token_service.clone(),
            idp_admin: self.idp_admin.clone(),
        }
    }
}

impl<S> tower::Layer<S> for ApiTokenAuthLayer {
    type Service = ApiTokenAuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ApiTokenAuthMiddleware {
            token_service: self.token_service.clone(),
            idp_admin: self.idp_admin.clone(),
            inner,
        }
    }
}

// The middleware service that authenticates API tokens
pub struct ApiTokenAuthMiddleware<S> {
    token_service: Arc<VaultService>,
    idp_admin: Arc<IdpAdmin>,
    inner: S,
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

fn extract_token<B>(req: &Request<B>) -> Option<String> {    
    let headers = req.headers();
    
    if let Some(bearer) = headers.typed_get::<Authorization::<Bearer>>() {
        return Some(bearer.token().to_string());
    }
    
    // Try to get token from query parameter
    if let Ok(Query(params)) = Query::<TokenQuery>::try_from_uri(req.uri()) {
        return params.token;
    }
    
    None
}

impl<S, B> tower::Service<Request<B>> for ApiTokenAuthMiddleware<S>
where
    S: tower::Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        // Get the error handler from request extensions
        let error_handler = req.extensions().get::<AppErrorHandler>()
            .expect("Error handler not found")
            .clone();
            
        // Get token service and setup clones for async block
        let token_service = self.token_service.clone();
        let idp_admin = self.idp_admin.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract the token from either header or query parameter
            let token = extract_token(&req);
            
            let token = match token {
                Some(token) => token,
                None => {
                    // No token provided, continue with the request
                    // The route handler will determine if authentication is required
                    return inner.call(req).await;
                }
            };
            
            // Verify the token and get the user ID
            match token_service.verify_token(&token).await {
                Ok(user_id) => {
                    // Get the user identity from IdpAdmin
                    match idp_admin.get_domain_user(user_id).await {
                        Ok(identity) => {
                            // Add identity to request extensions
                            let (mut parts, body) = req.into_parts();
                            parts.extensions.insert(identity);
                            
                            // Reconstruct the request and continue
                            let req = Request::from_parts(parts, body);
                            inner.call(req).await
                        }
                        Err(e) => {
                            // User ID is valid but user not found in identity provider
                            Ok(error_handler.handle_error(e))
                        }
                    }
                }
                Err(token_error) => {
                    Ok(error_handler.handle_error(token_error))
                }
            }
        })
    }
}