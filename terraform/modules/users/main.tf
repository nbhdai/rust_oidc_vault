# User Module - User creation and assignment

terraform {
  required_providers {
    keycloak = {
      source  = "mrparkers/keycloak"
      version = ">= 3.0.0"
    }
  }
}

# Create users
resource "keycloak_user" "users" {
  for_each  = { for user in var.users : user.username => user }
  
  realm_id   = var.realm_id
  username   = each.key
  enabled    = true

  email      = each.value.email
  first_name = each.value.first_name
  last_name  = each.value.last_name

  attributes = {
    "id" = each.value.id  # Store UUID as an attribute
  }

  initial_password {
    value     = each.value.password
    temporary = false
  }
}

# Assign roles to users
resource "keycloak_user_roles" "user_roles" {
  for_each = { for user in var.users : user.username => user }
  
  realm_id = var.realm_id
  user_id  = keycloak_user.users[each.key].id

  role_ids = [
    for role_name in each.value.roles : 
    lookup(var.roles, role_name, "")
    if lookup(var.roles, role_name, "") != ""
  ]
}

# Assign users to their teams (groups)
resource "keycloak_user_groups" "user_groups" {
  for_each = { for user in var.users : user.username => user }
  
  realm_id = var.realm_id
  user_id  = keycloak_user.users[each.key].id
  
  group_ids = [
    lookup(var.groups, each.value.team, "")
  ]
}