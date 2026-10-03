`untrusted-local.der` and `untrusted-local-key.der` are a public, test-only self-signed
localhost certificate and PKCS#8 RSA key. The certificate has a localhost DNS SAN and
validity from 2000 to 2100 so trust rejection is independent of the current date.

The runtime TLS test serves these bytes with Rustls and expects the client to reject
this issuer. It never installs either file into a trust store. These files have no
production credentials or signing purpose.
