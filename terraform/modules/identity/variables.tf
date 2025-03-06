variable "realm_id" {
  description = "The ID of the Keycloak realm"
  type        = string
}

variable "app_client_id" {
  description = "The ID of the Keycloak OIDC client for the application"
  type        = string
}

variable "vault_identity_oidc_key_name" {
  type = string
}

variable "external_accessor" {}

variable "institutions" {
  description = "List of team definitions"
  type = list(object({
    name        = string
    description = string
  }))
  default = []
}

variable "roles" {
  description = "List of role definitions"
  type = list(object({
    name        = string
    description = string
    rules = set(object({
      path = string
      capabilities = list(string)
    }))
  }))
}