resource "vault_jwt_auth_backend" "keycloak" {
  path               = "oidc"
  type               = "oidc"
  default_role       = "default"
  oidc_discovery_url = "http://keycloak:8080/realms/app-realm"
  oidc_client_id     = "vault"
  oidc_client_secret = keycloak_openid_client.vault_client.client_secret

  tune {
    default_lease_ttl = "1h"
    max_lease_ttl     = "24h"
    token_type        = "default-service"
  }
}

# Default role mapping for users
resource "vault_jwt_auth_backend_role" "default" {
  backend        = vault_jwt_auth_backend.keycloak.path
  role_name      = "default"
  token_ttl      = 3600
  token_max_ttl  = 86400
  
  bound_audiences = [keycloak_openid_client.vault_client.client_id]
  user_claim      = "sub"
  claim_mappings = {
    preferred_username = "username"
    email              = "email"
  }
  
  # Map Keycloak groups/roles to Vault policies based on the 'resource_access' claim
  groups_claim = "resource_access.vault.roles"
  
  # Set the token policies based on the mapped roles
  token_bound_cidrs = []
  token_policies = ["default"]
  
  allowed_redirect_uris = [
    "http://localhost:8200/ui/vault/auth/oidc/oidc/callback",
    "http://vault:8200/ui/vault/auth/oidc/oidc/callback",
    "http://localhost:4040/api/auth/callback",
    "http://app:4040/api/auth/callback"
  ]
}
