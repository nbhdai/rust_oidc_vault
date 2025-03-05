pub mod axum;
pub mod errors;
pub mod idp;
pub mod oidc;
pub mod vault;

use std::sync::Arc;

use anyhow::Context;
use axum::middleware::IdentifierLayer;
pub use axum::{
    error::{AppErrorHandler, ErrorHandlerExtensionLayer},
    extractors::OptionalIdentity,
    middleware::{ApiTokenAuthLayer, AuthenticateLayer, LoginEnforcerLayer},
};
use idp::admin::IdpAdmin;
use oidc::{
    keycloak::{KeycloakOidcBuilder, KeycloakOidcProvider},
    logout::LogoutService,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vault::VaultService;

/// Represents a team identity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TeamIdentity {
    pub id: Uuid,
    pub name: String,
}

/// Represents a team identity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct InstitutionIdentity {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub enum Role {
    Root,
    Advisor,
    Captain,
    Student,
    Spectator,
}

impl Role {
    pub fn parse(s: &str) -> Self {
        match s {
            "ROOT" => Self::Root,
            "ADVISOR" => Self::Advisor,
            "CAPTAIN" => Self::Captain,
            "STUDENT" => Self::Student,
            "SPECTATOR" => Self::Spectator,
            _ => panic!("Role not found: {}", s),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Root => "ROOT",
            Self::Advisor => "ADVISOR",
            Self::Captain => "CAPTAIN",
            Self::Student => "STUDENT",
            Self::Spectator => "SPECTATOR",
        }
    }

    pub fn is_admin(&self) -> bool {
        match self {
            Self::Root => true,
            _ => false,
        }
    }
}

/// Represents a user's identity in the AICL system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AiclIdentity {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub team: Option<TeamIdentity>,
    pub institution: Option<InstitutionIdentity>,
    pub role: Role,
}

#[derive(Clone)]
pub struct AiclIdentifier {
    pub vault: Arc<VaultService>,
    pub oidc: Arc<KeycloakOidcProvider>,
    pub idp: Arc<IdpAdmin>,
}

impl AiclIdentifier {
    pub async fn from_env() -> anyhow::Result<Self> {
        let vault = Arc::new(
            VaultService::from_env()
                .await
                .with_context(|| "Vault service initialization failed")?,
        );
        let idp_config = vault
            .get_idp_config_from_vault()
            .await
            .with_context(|| "Failed to get IDP config from Vault")?;
        let client_secret = idp_config.client_secret.clone();
        let idp = IdpAdmin::new(idp_config)
            .await
            .with_context(|| "IDP admin initialization failed")?;
        let oidc = Arc::new(
            KeycloakOidcBuilder::new(
                "http://localhost:4040".to_string(), // Application base URL
                "http://keycloak:8080/realms/app-realm".to_string(), // Issuer
                "rust-app".to_string(),              // Client ID
            )
            .with_client_secret(client_secret)
            .with_scopes(vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ])
            .build()
            .await
            .with_context(|| "Failed to build KeycloakOidcProvider")?,
        );

        Ok(Self { oidc, vault, idp })
    }

    pub fn identifier_layer(&self) -> IdentifierLayer {
        IdentifierLayer {
            identifier: self.clone(),
        }
    }

    pub fn authenticate_layer(&self) -> AuthenticateLayer {
        AuthenticateLayer {}
    }

    pub fn login_layer(&self) -> LoginEnforcerLayer {
        LoginEnforcerLayer {}
    }

    pub fn logout_service(&self) -> LogoutService {
        LogoutService {}
    }

    pub fn api_token_layer(&self) -> ApiTokenAuthLayer {
        ApiTokenAuthLayer {}
    }
}
