# wasix-sendmail

A POSIX-compliant sendmail implementation

## Features

- Sendmail-compatible command-line interface
- Multiple backends: SMTP and file output
- Reads email from stdin (standard sendmail behavior)
- Supports reading recipients from email headers (`-t` flag)

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

The backend is configured via environment variables:

### SMTP Backend

Set `SENDMAIL_BACKEND=smtp` and configure:

- `SMTP_HOST` - SMTP server hostname (default: `localhost`)
- `SMTP_PORT` - SMTP server port (default: `587`)
- `SMTP_USERNAME` - Optional username for authentication
- `SMTP_PASSWORD` - Optional password for authentication

### File Backend (default)

Set `SENDMAIL_BACKEND=file` and optionally:

- `SENDMAIL_FILE_PATH` - Output file path (default: `/tmp/sendmail_output.txt`)