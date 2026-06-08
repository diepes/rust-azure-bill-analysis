terraform {
  required_providers {
    azuread = {
      source  = "registry.terraform.io/hashicorp/azuread"
      version = "~> 3.0"
    }
    azapi = {
      source  = "registry.terraform.io/azure/azapi"
      version = "~> 2.0"
    }
  }
}

provider "azuread" {
  tenant_id = var.tenant_id
}

provider "azapi" {
  subscription_id = var.subscription_id
  tenant_id       = var.tenant_id
}
