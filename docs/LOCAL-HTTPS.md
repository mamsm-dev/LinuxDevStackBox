# Local HTTPS manual recovery (advanced)

The app manages local HTTPS end to end: it generates a private per-user CA with the system `openssl`, signs site certificates, and installs the CA into the distribution trust store via a polkit prompt. Use this page only when you need to inspect or repair the state by hand.

## Files

All HTTPS state lives under the user config directory (override with `LINUXDEVSTACKBOX_DATA_DIR`):

| File | Purpose |
| ---- | ------- |
| `~/.config/linuxdevstackbox/https/ca/rootCA.pem` | The local CA certificate |
| `~/.config/linuxdevstackbox/https/ca/rootCA-key.pem` | The local CA private key (mode 0600, never leaves the machine) |
| `~/.config/linuxdevstackbox/https/<hash>.pem` / `<hash>-key.pem` | Per-site leaf certificate and key |
| `~/.config/linuxdevstackbox/https-port` | The HTTPS listen port (default 443) |
| `~/.config/linuxdevstackbox/https-enabled` | Marker file: HTTPS is enabled when present |
| `~/.config/linuxdevstackbox/ssl.conf` | Generated Apache TLS configuration |

## Inspect the CA

```sh
openssl x509 -noout -subject -dates \
  -in ~/.config/linuxdevstackbox/https/ca/rootCA.pem
```

## Check whether the system trusts the CA

```sh
openssl verify -CAfile /etc/ssl/certs/ca-certificates.crt \
  ~/.config/linuxdevstackbox/https/ca/rootCA.pem
```

(Fedora uses `/etc/pki/tls/certs/ca-bundle.crt`.) A result of `OK` means browsers using the system store will trust sites signed by the CA.

## Remove the CA from the system trust store

```sh
# Arch
sudo rm /etc/ca-certificates/trust-source/anchors/linuxdevstackbox-rootCA.pem
sudo update-ca-trust

# Debian/Ubuntu
sudo rm /usr/local/share/ca-certificates/linuxdevstackbox-rootCA.crt
sudo update-ca-certificates

# Fedora
sudo rm /etc/pki/ca-trust/source/anchors/linuxdevstackbox-rootCA.pem
sudo update-ca-trust
```

The app performs the same removal through a polkit prompt when you disable and repair HTTPS.

## Full reset

1. Stop Apache in the app.
2. Remove the HTTPS state: `rm -rf ~/.config/linuxdevstackbox/https ~/.config/linuxdevstackbox/ssl.conf ~/.config/linuxdevstackbox/https-enabled`.
3. Remove the CA anchor as shown above.
4. Enable HTTPS again from the app (Services → Apache → HTTPS); it regenerates the CA, certificates and trust.

Some Firefox builds use their own certificate store; import `rootCA.pem` via Settings → Privacy & Security → Certificates if needed.
