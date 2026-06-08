output "tenant_id" {
  description = "Entra tenant ID"
  value       = var.tenant_id
}

output "client_id" {
  description = "Application (client) ID — set as ENTRA_CLIENT_ID"
  value       = azuread_application.mcp.client_id
}

output "client_secret" {
  description = "Client secret value — set as ENTRA_CLIENT_SECRET"
  value       = azuread_application_password.mcp.value
  sensitive   = true
}

output "env_block" {
  description = "Ready-to-use env vars for the MCP server (client_secret shown separately)"
  value       = <<-EOT
    ENTRA_TENANT_ID=${var.tenant_id}
    ENTRA_CLIENT_ID=${azuread_application.mcp.client_id}
    MCP_URL=${var.mcp_public_url}
  EOT
}

output "blob_service_url" {
  description = "Set as AZ_BILLING_BLOB_SERVICE_URL"
  value       = azapi_resource.billing_storage.output.properties.primaryEndpoints.blob
}

output "storage_account_name" {
  description = "Storage account name"
  value       = azapi_resource.billing_storage.name
}

output "container_name" {
  description = "Set as AZ_BILLING_CONTAINER_NAME"
  value       = azapi_resource.billing_container.name
}
