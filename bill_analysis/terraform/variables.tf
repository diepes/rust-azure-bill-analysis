variable "subscription_id" {
  description = "Azure subscription ID to deploy the storage account into"
  type        = string
}

variable "tenant_id" {
  description = "Azure Entra tenant ID"
  type        = string
  default     = "6d49e5e4-e7e3-4c39-a95a-7c74cff1e9a6"
}

variable "app_display_name" {
  description = "Display name for the Entra app registration"
  type        = string
  default     = "bill-analysis-mcp"
}

variable "mcp_public_url" {
  description = "Public base URL of the MCP server (used for the OAuth redirect URI)"
  type        = string
  # e.g. "http://localhost:8091" for local dev
}

variable "location" {
  description = "Azure region for the storage account"
  type        = string
  default     = "australiaeast"
}

variable "resource_group_name" {
  description = "Resource group to deploy the storage account into"
  type        = string
  default     = "rg-azure-billing"
}

variable "billing_viewer_upns" {
  description = "List of user principal names (emails) to assign the BillingViewer role"
  type        = list(string)
  default     = []
}

variable "storage_account_name" {
  description = "Name of the Azure storage account for billing CSVs"
  type        = string
  default     = "eroadstaazurebilling"
}
