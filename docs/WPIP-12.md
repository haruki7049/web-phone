# WPIP-12: Dynamic Codec Negotiation & Capability Handshake

`experimental` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an experimental extension for `wpdaemon` and `wpclient` implementations to support **Dynamic Audio Codec Capability Negotiation**.

During signed HTTP signaling (`POST /sdp`), clients declare supported audio Codec IDs (defined in WPIP-04), allowing nodes to negotiate optimal codec selections (e.g., Opus vs PCM) dynamically.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Codec Capability Declaration

Clients issuing `POST /sdp` Requests MAY include a `supported_codecs` array in the JSON request body:

```json
{
  "type": "offer",
  "sdp": "...",
  "supported_codecs": [1, 0, 2]
}
```

Where Codec IDs correspond to WPIP-04 definitions (`1` = OPUS, `0` = PCM_F32LE, `2` = PCM_S16LE).

`wpdaemon` MUST select the highest priority mutually supported codec and return it in the `200 OK` SDP Answer response JSON.
