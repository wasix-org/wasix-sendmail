# wasix-sendmail

A sendmail-compatible email sender with multiple backends.

## Features

- Supports all common sendmail flags
- Multiple backends: SMTP relay, REST API, and file output

## Building for WASIX

Build the WASM module:

```bash
cargo build --release --target=wasm32-wasmer-wasi
```

This compiles the project to `target/wasm32-wasmer-wasi/release/sendmail.wasm`. You can then run that binary either directly or via the supplied `wasmer.toml` (`wasmer run .`)

## Usage

Read email from stdin and send:

```bash
echo "Subject: Test\n\nBody" | sendmail recipient@example.com
```

Read recipients from email headers:

```bash
echo "To: user@example.com\nSubject: Test\n\nBody" | sendmail -t
```

Set envelope sender:

```bash
echo "Subject: Test\n\nBody" | sendmail -f sender@example.com recipient@example.com
```

## Configuration

The backend is selected automatically based on which environment variables are set.

### 1. File Backend (highest priority)

For debugging and testing:

- `SENDMAIL_FILE_PATH` - Path to output file where emails will be written

### 2. SMTP Relay Backend (second highest priority)

For sending via an SMTP relay:

- `SENDMAIL_RELAY_HOST` - SMTP relay hostname (required)
- `SENDMAIL_RELAY_PORT` - SMTP relay port (default: `587`)
- `SENDMAIL_RELAY_PROTO` - Protocol (e.g., `tls`, `starttls`, `plain`, `opportunistic`) (default: opportunistic)
- `SENDMAIL_RELAY_USER` - Username for authentication (optional)
- `SENDMAIL_RELAY_PASS` - Password for authentication (optional)

If a username or password is specified, you also need to specify the other one.

### 3. REST API Backend (lowest priority)

For sending via a custom REST API:

- `SENDMAIL_API_URL` - URL of the mail endpoint (required)
- `SENDMAIL_API_SENDER` - Default sender address (required)
- `SENDMAIL_API_TOKEN` - Authentication token (required)

**Note:** When deploying to [wasmer edge](https://wasmer.io/products/edge) the environment variables for the REST API will be automatically populated.

**Note:** If no backend is configured, sendmail will exit with an error.

All three API variables must be set for the REST API backend to be used.