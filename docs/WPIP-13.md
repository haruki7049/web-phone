# WPIP-13: Audio Device Hot-plugging & Dynamic Audio Route Switching

`experimental` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an experimental extension for `wpclient` and `wpapi` implementations to support **Audio Device Hot-plugging & Dynamic Audio Route Switching**.

When physical audio capture (microphone) or playback (speaker) devices are plugged in or unplugged during an active WebRTC audio session, implementations monitor device availability events and seamlessly switch to default or newly arrived audio routes without interrupting active DataChannel connections.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Hot-plug Event Monitoring

`wpclient` and `wpapi` implementations MAY register operating system native audio device event listeners or poll audio host device lists.

- Upon detecting an audio input or output device disconnection during an active call, the client MUST fall back to the system default available input or output audio device.
- Upon detecting a new preferred audio device connection (e.g., Bluetooth headset or USB microphone plugged in), the client MAY switch audio streams dynamically to the newly connected device.

______________________________________________________________________

## 2. Stream Re-initialization

- Audio stream switching MUST NOT drop or interrupt active WebRTC PeerConnection or DataChannel sessions.
- Input (microphone) and output (speaker) CPAL streams MAY be re-created independently without affecting WebRTC packet transmission queues.
