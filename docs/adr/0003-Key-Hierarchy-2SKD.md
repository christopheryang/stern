# ADR-0003: Key hierarchy — Argon2id + 2SKD, per-entry DEKs wrapped by a KEK

- **Status:** Accepted (recorded 2026-11-21)
- **Date:** 2026-11-21

## Context

The vault file is copyable, so its encryption *is* the security
boundary. A password-only KDF lets anyone who copies the file run an
offline dictionary attack at Argon2id speed. Encrypting every entry
directly under one password-derived key also makes password changes
expensive (re-encrypt everything) and gives a single key total blast
radius. 1Password's 2SKD (two-secret key derivation) shows the value of
binding unlock to the device as well as to the user's memory.

## Decision

- **Two secrets:** the master password plus a 16-byte device Secret Key
  generated at vault creation and stored in the OS keychain.
- **Derivation:** HKDF-SHA256 preprocess over both secrets → Argon2id
  (256 MiB, 3 iterations, parallelism 4) → master key → HKDF-SHA256 →
  KEK.
- **Per-entry keys:** each entry gets a fresh random DEK per write;
  contents are encrypted with XChaCha20-Poly1305 with context-binding
  AAD; the DEK is wrapped under the KEK with AES-KW (RFC 3394).
- **Verification:** a hash of the derived material in the vault metadata
  is compared in constant time before any entry is decrypted.
- Key material lives only in `SecretMem` (mlock + zeroize) inside the
  unlocked session.

## Consequences

- A copied vault file alone cannot be attacked offline at full speed:
  the attacker also needs the Secret Key from the device keychain.
- Password change is cheap: re-wrap DEKs, entries untouched.
- Compromise of one DEK exposes one entry version, not the vault.
- **Deliberate downside:** there is no recovery. Lose the password or
  the device (and its keychain) and the vault is gone. Export/import
  plus the Secret Key are the migration path; support cannot help.
- Argon2id's parameters make unlock take noticeable time and memory on
  low-end hardware — accepted as the cost of the offline-attack margin,
  and recorded in `kdf_params.rs` / `crypto_constants.rs` so any change
  is a visible, reviewable event.
