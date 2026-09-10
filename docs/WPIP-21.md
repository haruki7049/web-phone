# WPIP-21: Decentralized Repository-Local Specification Standard

`draft` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines the repository-local documentation standard for **WPIPs (web-phone Implementation Possibilities)** across the `web-phone` ecosystem.

Unlike centralized protocol specification models, WPIP specifications MUST be maintained, versioned, and documented directly within each `web-phone` related codebase repository. Repositories MAY place WPIP specifications in documentation locations other than a `docs/` directory (such as `WPIP/`, `spec/`, or project root).

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Motivation & Design Rationale

1. **Elimination of Central Gatekeepers**:

   - Centralized protocol repositories introduce single-point-of-failure governance bottlenecks, review latency, and political friction for protocol evolution.
   - In alignment with `web-phone`'s decentralized, self-sovereign architecture (WPIP-01), protocol specifications SHOULD evolve alongside the software that implements them.

1. **Atomic Code & Specification Synchronization**:

   - Keeping WPIPs directly inside the codebase repository guarantees that git commits, tags, and release artifacts contain the exact specification state corresponding to the executable software.
   - Developers and AI agents can inspect, test, and verify code against authoritative specifications within the same repository context without external network dependencies.

1. **Flexible Repository Layout**:

   - Repositories differ in project structure and language conventions. Allowing WPIPs to reside in any repository-local location (e.g., `docs/`, `WPIP/`, `spec/`, or root directory) ensures maximum flexibility across various frameworks while preserving repository-local versioning.

______________________________________________________________________

## 2. Specification Rules & Structure

1. **Repository-Local Management**:

   - Every `web-phone` related codebase repository MUST store its implemented and proposed WPIP specifications locally within the repository itself.
   - Repositories MAY store WPIP specifications in directories other than `docs/` (such as `WPIP/`, `spec/`, or project root).
   - Individual proposals MUST follow a clear naming convention (RECOMMENDED: `WPIP-XX.md` or `WPIP-XX-<title>.md`).
   - An index document SHOULD be provided within the repository listing all supported, optional, and experimental WPIPs with their titles, statuses, and summaries.

1. **Atomic Modification**:

   - Any pull request or commit that modifies protocol packet formats (`ProtocolPacket`), signaling endpoints, or cryptographic behaviors MUST include corresponding updates to the repository's local WPIP specification files.

1. **Cross-Repository Referencing**:

   - When referencing WPIPs across different ecosystem repositories, implementations SHOULD specify the target repository URL, relative file path, and git commit hash or release tag (e.g., `web-phone/docs/WPIP-04.md@v0.1.0`).
