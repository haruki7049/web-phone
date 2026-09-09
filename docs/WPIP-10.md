# WPIP-10: STUN/TURN Dynamic HMAC Token Authentication

`optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an optional extension for `wpdaemon` and `wpclient` implementations to support **Ephemeral HMAC-SHA1 Token Authentication** for embedded STUN/TURN services (WPIP-06).

By issuing time-limited, cryptographically signed TURN credentials during HTTP signaling (`POST /sdp`), nodes prevent unauthorized third parties from abusing embedded TURN relays as open bandwidth proxies.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Ephemeral Credential Generation

When a client successfully authenticates via Ed25519 signature during `POST /sdp`:

- `wpdaemon` MUST compute an ephemeral TURN username: `<expiration_timestamp>:<user_address_hex>`.
- `wpdaemon` MUST compute an ephemeral TURN password using HMAC-SHA1 over the username using a server-side secret key: `HMAC-SHA1(ServerSecret, Username)`.
- The `200 OK` SDP Answer response JSON MAY include an `ice_servers` block containing the generated ephemeral TURN credentials.

```json
{
  "type": "answer",
  "sdp": "...",
  "ice_servers": [
    {
      "urls": ["turn:node.web-phone.dev:3478?transport=udp"],
      "username": "1757278800:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "credential": "base64_hmac_sha1_signature"
    }
  ]
}
```

______________________________________________________________________

## 2. TURN Server Verification

The embedded TURN service in `wpdaemon` MUST verify incoming `Allocate` requests:

1. Parse the expiration timestamp from the TURN username. If `current_time > expiration_timestamp`, reject allocation with `401 Unauthorized`.
1. Compute `HMAC-SHA1(ServerSecret, Username)` and verify that it matches the presented `credential`.
