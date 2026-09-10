# Verify a release artifact

Every binary archive attached to a panproto release carries two independent claims: a checksum, which says the bytes are the ones that were built, and an attestation, which says which workflow built them and from which source revision. The first catches a truncated or corrupted download. Only the second distinguishes an archive panproto built from one someone else assembled and named the same thing.

## Checksums

Each release attaches a `SHA256SUMS` file covering every `.tar.gz` and `.zip`, in the format `sha256sum -c` reads:

```sh
gh release download v0.72.1 --pattern 'panproto-c-*' --pattern 'SHA256SUMS'
sha256sum -c SHA256SUMS
```

On macOS, `shasum -a 256 -c SHA256SUMS`.

The Swift XCFramework is verified separately and automatically. `Package.swift` pins `releaseXCFrameworkChecksum`, and SwiftPM refuses to resolve if the downloaded artifact does not match, so a consumer adding the package as a dependency gets this check without asking for it.

## Attestations

A checksum only says the file matches a hash published beside it. Both come from the same release, so an attacker able to replace one can replace the other. An attestation is signed by the workflow's own OIDC identity at build time and records the repository, the workflow file, and the commit:

```sh
gh attestation verify panproto-c-aarch64-apple-darwin.tar.gz --repo panproto/panproto
```

This succeeds only for an archive built by this repository's workflow. It needs no key on your side and no key is stored on ours: signing is keyless, tied to the job's identity rather than to a secret that could leak.

## What is in an archive

Each release also carries `panproto-c.cdx.json`, a [CycloneDX](https://cyclonedx.org/) software bill of materials listing every crate the C library was built from, at the versions `Cargo.lock` pinned. It is itself attested, so the inventory is as verifiable as the binary it describes.

To answer "does this release contain some vulnerable dependency", read the SBOM rather than rebuilding:

```sh
gh release download v0.72.1 --pattern 'panproto-c.cdx.json'
jq -r '.components[] | "\(.name) \(.version)"' panproto-c.cdx.json | sort
```

## Crates, wheels and npm packages

These do not need the steps above. `crates.io`, PyPI and npm are all published through OIDC Trusted Publishing, so the registry itself records which workflow published each version, and npm additionally carries provenance that `npm audit signatures` checks. No long-lived publication token exists for any of them.

## See also

- [Install the CLI](./cli.md) for the installer scripts, which verify checksums themselves.
- [Install the Swift SDK](./swift.md) for the XCFramework pin.
