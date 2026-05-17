# ADR-0008: Entra OAuth 2.0 Auth Code + PKCE for MCP Authentication

The MCP server needs authentication so that only authorised users can query billing data. We chose the OAuth 2.0 Authorization Code + PKCE flow against Microsoft Entra ID, with the MCP server acting as an OAuth proxy that passes Entra JWTs through to callers.

## Decision

### Flow

The MCP server acts as a thin OAuth 2.0 proxy:

1. Exposes `GET /.well-known/oauth-authorization-server` so MCP clients (VS Code Copilot) can discover the OAuth endpoints automatically.
2. `GET /authorize` generates a PKCE `code_verifier`/`code_challenge`, stores short-lived `PkceState` in memory keyed by `state`, and redirects the browser to Entra.
3. Entra redirects back to `GET /callback` with the authorization code. The server exchanges the code with Entra, gets an Entra access token, and redirects the MCP client to its own `redirect_uri` with the token.
4. All subsequent `POST /mcp` calls must carry `Authorization: Bearer <entra_access_token>`. The server validates the JWT and checks the `roles` claim.

### Token validation on every `POST /mcp` call

- Signature verified against Entra JWKS (`https://login.microsoftonline.com/{tenant_id}/discovery/v2.0/keys`); JWKS is cached in memory and refreshed on key-not-found.
- `iss` must equal `https://login.microsoftonline.com/{tenant_id}/v2.0`.
- `aud` must equal the app registration `client_id`.
- `exp` must be in the future.
- `roles` claim must contain `BillingViewer`.

All validation failures produce a distinct `[auth] FAIL <reason>` log line including `oid` and `upn` where available (e.g. `missing_role`, `token_expired`, `invalid_signature`).

### Authorization: one App Role — `BillingViewer`

A single `BillingViewer` App Role is declared on the app registration and gates all three MCP tools. Finer-grained roles can be added later without changing the validation middleware shape.

App Roles are preferred over Entra group membership: role names are meaningful in code (`BillingViewer` vs. an opaque group GUID), scoped to this application, and not subject to the 200-group JWT overflow problem.

### Configuration

Loaded from `.env` file (via `dotenvy`) then real environment variables:

| Variable | Purpose |
|---|---|
| `ENTRA_TENANT_ID` | Entra tenant ID |
| `ENTRA_CLIENT_ID` | App registration client ID (also used as JWT `aud`) |
| `ENTRA_CLIENT_SECRET` | App registration client secret |
| `MCP_PUBLIC_URL` | Base URL advertised in OAuth metadata (e.g. `http://localhost:3000`); must match a redirect URI registered in Entra |

`client_secret` is never accepted as a CLI arg to avoid leaking into the process list.

If any of the four variables are absent, the server refuses to start **unless** `--no-auth` is passed explicitly. This prevents silent misconfiguration in production while keeping local dev easy.

### Unauthenticated endpoints

`GET /.well-known/oauth-authorization-server`, `GET /authorize`, `GET /callback`, and `GET /mcp` (health check) are public. Only `POST /mcp` requires a valid JWT.

### Startup validation

On startup (when `--no-auth` is not set), the server probes Entra before accepting requests:

1. **Fetch OIDC discovery document** — `GET https://login.microsoftonline.com/{tenant_id}/v2.0/.well-known/openid-configuration`. Confirms the tenant exists and pre-warms the JWKS cache. Exits with a clear error if the tenant ID is wrong.
2. **Client credentials probe** — `POST https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token` with `grant_type=client_credentials`. Confirms the app registration exists and the `client_id`/`client_secret` are valid. Exits with the Entra error code if not (e.g. `AADSTS7000215`).
3. **Redirect URI advisory log** — prints the exact callback URL that will be sent to Entra (`{MCP_PUBLIC_URL}/callback`) with a reminder to ensure it is registered in the app registration. Redirect URI registration cannot be validated without `Application.Read.All` Graph API permission, so this is a human-readable prompt rather than a hard check.

```
[startup] Entra tenant OK: contoso.onmicrosoft.com
[startup] App registration credentials OK: client_id=abc...
[startup] OAuth callback will use: http://localhost:3000/callback
[startup]   ↳ Ensure this is registered in your app registration redirect URIs
```

### No server-side token storage

The server does not store or refresh tokens. When Copilot receives a `401`, it re-triggers the browser auth flow. This keeps the server stateless with respect to credentials.

## Alternatives considered

- **Client Credentials flow:** rejected — no user identity, no `roles` claim per user; suitable for service-to-service only.
- **Server-issued tokens (AS proxy):** rejected — requires a token store; Entra JWT pass-through gives identity and role claims for free via standard JWKS validation.
- **Entra group membership instead of App Roles:** rejected — group GUIDs are environment-specific, invisible in code, and subject to 200-group JWT overflow.
- **`--no-auth` as default (warn and continue):** rejected — silent misconfiguration in production is worse than requiring an explicit opt-out.
