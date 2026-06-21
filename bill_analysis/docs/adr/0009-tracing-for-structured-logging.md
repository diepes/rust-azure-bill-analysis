# Use `tracing` for structured JSON logging

We replaced `env_logger` with `tracing` + `tracing-subscriber` (JSON layer) as the logging foundation for the MCP server. All existing `log::*` macro calls are bridged automatically via `tracing-log`. The subscriber emits one JSON object per log line, with a local-timezone timestamp in `YYYYMMDD-HHhMM±hh:mm` format, controlled by `RUST_LOG` at startup.

The primary reason is forward compatibility with OpenTelemetry: when tracing is ready to be added, `tracing-opentelemetry` + `opentelemetry-otlp` slot in without touching any callsites. A secondary reason is the per-request audit line — `tracing::info!(upn, tool, bytes, run_msec)` emits structured fields natively, whereas `env_logger` would have required double-encoding JSON into a string message.

## Considered Options

- **`env_logger` with custom format closure** — possible but fragile; structured fields require manual JSON serialisation into the message string, which breaks log aggregators that expect a flat `msg` field.
- **`slog`** — mature structured logging, but no standard OTel bridge; a dead-end for the OTel path.
