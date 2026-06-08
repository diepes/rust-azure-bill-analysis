# ── Storage account (Azure billing CSVs) ─────────────────────────────────────

resource "azapi_resource" "billing_rg" {
  type      = "Microsoft.Resources/resourceGroups@2024-03-01"
  name      = var.resource_group_name
  location  = var.location
  parent_id = "/subscriptions/${var.subscription_id}"
}

resource "azapi_resource" "billing_storage" {
  type      = "Microsoft.Storage/storageAccounts@2023-05-01"
  name      = var.storage_account_name
  location  = var.location
  parent_id = azapi_resource.billing_rg.id

  body = {
    kind = "StorageV2"
    sku  = { name = "Standard_LRS" }
    properties = {
      minimumTlsVersion        = "TLS1_2"
      allowBlobPublicAccess    = false
      supportsHttpsTrafficOnly = true
      encryption = {
        services  = { blob = { enabled = true, keyType = "Account" } }
        keySource = "Microsoft.Storage"
      }
    }
  }

  response_export_values = ["properties.primaryEndpoints.blob"]
}

resource "azapi_resource" "billing_container" {
  type      = "Microsoft.Storage/storageAccounts/blobServices/containers@2023-05-01"
  name      = "billing"
  parent_id = "${azapi_resource.billing_storage.id}/blobServices/default"

  body = {
    properties = { publicAccess = "None" }
  }
}

# ── Grant the MCP app service principal read access to blobs ─────────────────
# Storage Blob Data Reader = 2a2b9908-6ea1-4ae2-8e65-a410df84e7d1

resource "azapi_resource" "billing_storage_reader" {
  type      = "Microsoft.Authorization/roleAssignments@2022-04-01"
  name      = "10000000-0000-0000-0001-000000000001"
  parent_id = azapi_resource.billing_storage.id

  body = {
    properties = {
      roleDefinitionId = "/providers/Microsoft.Authorization/roleDefinitions/2a2b9908-6ea1-4ae2-8e65-a410df84e7d1"
      principalId      = azuread_service_principal.mcp.object_id
      principalType    = "ServicePrincipal"
    }
  }
}
