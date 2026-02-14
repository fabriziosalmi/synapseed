# Install SYNAPSEED

Install the SYNAPSEED binary from the repository root:

```bash
cargo install --path bin/synapseed --force
```

Verify the installation:

```bash
synapseed --version
```

The binary must be in your `PATH` for the extension to find it, or configure `synapseed.binaryPath` in settings.
