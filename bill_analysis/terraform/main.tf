
# ── App registration ─────────────────────────────────────────────────────────

resource "azuread_application" "mcp" {
  display_name     = var.app_display_name
  sign_in_audience = "AzureADMyOrg"

  web {
    redirect_uris = ["${var.mcp_public_url}/callback"]
  }

  # Expose the app so tokens carry the appid in the audience.
  # identifier_uris defaults to api://<client_id> — matches the Rust probe scope.
  api {
    requested_access_token_version = 2
  }

  app_role {
    id                   = "10000000-0000-0000-0000-000000000001"
    allowed_member_types = ["User"]
    description          = "Can view Azure billing data via the MCP server"
    display_name         = "BillingViewer"
    value                = "BillingViewer"
    enabled              = true
  }
}

resource "azuread_service_principal" "mcp" {
  client_id = azuread_application.mcp.client_id
}

# ── Client secret ─────────────────────────────────────────────────────────────

resource "azuread_application_password" "mcp" {
  application_id = azuread_application.mcp.id
  display_name   = "mcp-server-secret"
  end_date       = "2027-12-31T00:00:00Z"
}

# ── Role assignments ──────────────────────────────────────────────────────────

data "azuread_user" "billing_viewers" {
  for_each            = toset(var.billing_viewer_upns)
  user_principal_name = each.value
}

resource "azuread_app_role_assignment" "billing_viewer" {
  for_each = data.azuread_user.billing_viewers

  app_role_id         = "10000000-0000-0000-0000-000000000001"
  principal_object_id = each.value.object_id
  resource_object_id  = azuread_service_principal.mcp.object_id
}
