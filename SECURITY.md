# Security Policy

## Reporting a Vulnerability

We take security seriously. If you discover a security vulnerability, please:

1. **DO NOT** open a public issue
2. Email us at: **security@z.fun**
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

We will respond within 48 hours and work with you to address the issue.

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| main    | :white_check_mark: |

## Security Considerations

### Client-Side Security
- All wallet operations happen in your browser
- Private keys never leave your device
- Proof generation is done locally

### Server-Side Security
- Server never sees your private keys
- Server only provides Merkle tree data
- All proofs are publicly verifiable

### What to Audit
If you're reviewing the code for security:

1. **crates/lib/** - Core verification logic
2. **crates/wasm/** - Browser WASM interface
3. **crates/server/** - API endpoints and data handling

## Known Limitations

- This is experimental software
- No formal security audit has been completed yet
- Use at your own risk with funds you can afford to lose
