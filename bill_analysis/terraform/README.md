# Terraform — Entra App Registration

Deploys the Azure Entra app registration, client secret, `BillingViewer` app role, and role assignments for the `bill_analysis_mcp` server.

## Prerequisites

```bash
brew install terraform
# login to developer tenant       # or your OS equivalent
az login --tenant 6d49e5e4-e7e3-4c39-a95a-7c74cff1e9a6
```

The `az login` session is used by the `azuread` Terraform provider automatically.

## Quick start

```bash
cd terraform
cp terraform.tfvars.example terraform.tfvars
# edit terraform.tfvars — set mcp_public_url and billing_viewer_upns

terraform init
terraform apply
```

After apply, grab the outputs:

```bash
terraform output env_block
terraform output -raw client_secret   # sensitive — don't log this
```

## Starting the MCP server

```bash
export ENTRA_TENANT_ID=6d49e5e4-e7e3-4c39-a95a-7c74cff1e9a6
export ENTRA_CLIENT_ID=$(terraform output -raw client_id)
export ENTRA_CLIENT_SECRET=$(terraform output -raw client_secret)
export MCP_URL=http://localhost:8091

cargo run --bin bill_analysis_mcp -- \
  --data-dir /path/to/billing/csv \
  --bind 0.0.0.0:8091
```

## Variables

| Variable | Default | Description |
|---|---|---|
| `tenant_id` | `6d49e5e4-...` | Your test Entra tenant |
| `app_display_name` | `bill-analysis-mcp` | App registration name |
| `mcp_public_url` | *(required)* | Base URL for OAuth redirect |
| `billing_viewer_upns` | `[]` | Users to assign BillingViewer role |

## Role

The `BillingViewer` app role (value: `BillingViewer`) maps directly to the role checked in `oauth_proxy.rs`. Assign it to any user who should be allowed to query billing data.
