# WPIP-15: Optional Security Mechanisms & DoS Protections

`draft` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This document specifies optional security mechanisms and Denial of Service (DoS) protection specifications for `wpdaemon`, `wpapi`, and `wpclient` implementations within the `web-phone` ecosystem.

While core WPIP specifications (WPIP-01 through WPIP-14) define protocol formats and signaling mechanics, this proposal establishes standardized, optional Rate Limiting (Token Bucket), Ed25519 Request Authorization with Time-Drift Replay Protection, Call State Authorization, and Resource Limits to defend against traffic floods, identity spoofing, and resource exhaustion attacks.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Rate Limiting Specifications (Token Bucket)

To prevent traffic flood attacks across different protocol interfaces, compliant `wpdaemon` implementations MAY enforce Token Bucket rate limiters.

### 1.1 HTTP Signaling Endpoint Rate Limiting

- Implementations SHOULD restrict incoming HTTP SDP offer/answer requests per client IP address.
- Recommended configuration:
  - Refill Rate: `5.0` tokens/second
  - Bucket Capacity (Max Burst): `10.0` tokens
- Implementations MUST inspect `X-Forwarded-For` and `X-Real-IP` HTTP headers ONLY when the immediate connecting peer IP matches a configured list of trusted reverse proxies (`trusted_proxies`). If the direct connection is untrusted, `wpdaemon` MUST ignore client-supplied forwarding headers to prevent rate-limit bypass via IP spoofing.
- When an IP exceeds the token bucket capacity, the server MUST respond with `HTTP 429 Too Many Requests`.

### 1.2 STUN UDP Endpoint Rate Limiting

- Implementations SHOULD restrict incoming STUN Binding Requests per UDP source IP address to mitigate STUN amplification and UDP packet flood attacks.
- Recommended configuration:
  - Refill Rate: `20.0` tokens/second
  - Bucket Capacity (Max Burst): `50.0` tokens
- Packets exceeding the threshold MUST be dropped immediately prior to parsing or building response frames.

### 1.3 WebRTC DataChannel Packet Rate Limiting

- Implementations SHOULD enforce per-client packet rate limits on established WebRTC DataChannels to prevent audio packet spamming and control packet floods.
- Recommended configuration:
  - Refill Rate: `100.0` packets/second
  - Bucket Capacity (Max Burst): `200.0` packets
- Data packets exceeding the token limit MUST be discarded.

______________________________________________________________________

## 2. Ed25519 Request Authorization & Anti-Replay Protection

To guarantee client identity authenticity and prevent unauthorized SDP handshakes, implementations MAY require signed HTTP headers.

### 2.1 HTTP Authorization Header Format

Compliant clients MUST send an `Authorization` header formatted as:
`WP-Ed25519 <PubKeyHex>:<Timestamp>:<SignatureHex>`

- `PubKeyHex`: Hex-encoded 32-byte Ed25519 public key (`UserAddress`).
- `Timestamp`: UNIX epoch timestamp in seconds.
- `SignatureHex`: Hex-encoded Ed25519 signature over the payload `${timestamp}:${sdp_offer}`.

### 2.2 Time-Drift Verification

- Daemons MUST verify the signature using the provided public key.
- Daemons MUST check the time difference between server current time and request `Timestamp`.
- If `|server_time - request_timestamp| > 300` seconds, the daemon MUST reject the request to prevent replay attacks.

______________________________________________________________________

## 3. Call State Authorization & Unsolicited Media Filtering

To prevent unauthorized audio injection and unsolicited call interception:

- Implementations MUST enforce explicit call approval checks (`CallAcceptResponse`) before forwarding 1-to-1 audio streams between client endpoints.
- Unsolicited audio frames targeted at clients who have not explicitly accepted the call session MUST be filtered out by `wpdaemon`.
- Group call rooms MAY enforce participant capacity limits to avoid forwarding overload.

______________________________________________________________________

## 4. Resource Allocation & Timeout Policies

- **Maximum Packet Payload Size**: Implementations MUST reject or truncate individual protocol packets exceeding `1,048,576` bytes (1 MB) to prevent OOM memory exhaustion.
- **Keep-Alive Pruning (WPIP-09 Extension)**: Daemons MUST periodically send `Ping` control frames (recommended: every 10 seconds). If a client fails to return a `Pong` response within 30 seconds, the daemon MUST terminate the PeerConnection and free associated registry memory.
