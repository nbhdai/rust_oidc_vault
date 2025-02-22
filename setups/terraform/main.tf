# Configure the Vault provider
provider "vault" {
  address = "http://vault:8300"
  token   = "root"  # This should be changed in production
}

# Configure OIDC auth method
resource "vault_jwt_auth_backend" "oidc" {
  path               = "oidc"
  type               = "oidc"
  oidc_discovery_url = "http://keycloak:8080/realms/test"
  oidc_client_id     = "vault-client"
  oidc_client_secret = "your-client-secret"
  default_role       = "default"
  
  tune {
    default_lease_ttl = "1h"
    max_lease_ttl     = "1h"
  }
}

# Create a named key for signing tokens
resource "vault_identity_oidc_key" "key" {
  name             = "named-key"
  algorithm        = "RS256"
  rotation_period  = 86400  # 24 hours in seconds
}

# Create a scope
resource "vault_identity_oidc_scope" "user_scope" {
  name        = "user"
  template    = <<EOF
{
  "username": {{identity.entity.name}},
  "contact": {
    "email": {{identity.entity.metadata.email}}
  }
}
EOF
  description = "Basic user information"
}

# Create the provider
resource "vault_identity_oidc_provider" "test" {
  name = "my-provider"
  https_enabled = false
  issuer_host = "0.0.0.0:8200"
  allowed_client_ids = [
    vault_identity_oidc_client.test.client_id
  ]
}

# Create client
resource "vault_identity_oidc_client" "test" {
  name          = "test-aicl"
  redirect_uris = [
    "http://localhost:8080/realms/test/protocol/openid-connect/auth",
  ]
  id_token_ttl     = 2400
  access_token_ttl = 7200
}

# Create role
resource "vault_identity_oidc_role" "role" {
  name      = "third-party-app-role"
  key       = vault_identity_oidc_key.key.name
  client_id = vault_identity_oidc_client.test.client_id
}

# Policy for OIDC operations
resource "vault_policy" "oidc" {
  name = "oidc-policy"

  policy = <<EOT
path "identity/oidc/provider/+/authorize" {
  capabilities = ["read", "update"]
}

path "identity/oidc/provider/+/token" {
  capabilities = ["read", "update"]
}

path "identity/oidc/provider/+/userinfo" {
  capabilities = ["read"]
}

path "identity/oidc/provider/+/.well-known/*" {
  capabilities = ["read"]
}
EOT
}