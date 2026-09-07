# Minimal Troubleshooting Guide

Use this guide to quickly locate the most common startup, relay, and model-refresh problems.

## What to check first

1. Read the [Runtime and Deployment Guide](runtime-and-deployment-guide.md).
2. Then read the [FAQ and Account Routing Rules](faq-and-account-routing-rules.md).

## Recommended troubleshooting order

### 1. Confirm the service is running

- Check whether the desktop app or service process is actually running.
- Confirm that the listen address, port, and proxy settings are correct.
- Review logs for database connection failures, port conflicts, or permission errors.

### 2. Confirm the request reaches the gateway

- Make sure the client is really sending requests to the current gateway address.
- Check the request log for the matching path and status code.
- For third-party clients, verify that both the API key and base URL are correct.

### 3. Confirm the model and account are available

- Check whether the account is disabled, expired, or quota-limited.
- Refresh usage and retry.
- If the account is banned, go through the account cleanup flow first.

### 4. Check streaming and tool-call behavior

- Verify that `tool_calls`, SSE end events, and restored fields are not being lost.

## If it still fails

- Follow the documentation links in README.
- Preserve the failed request, response, and request-log entries before continuing the investigation.