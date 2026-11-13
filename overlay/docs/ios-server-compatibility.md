# Server compatibility

Horizon iOS speaks the same protocol as Horizon on desktop (Minecraft: Java Edition 26.3,
protocol 777). A server that accepts a vanilla 26.3 client should accept Horizon; the notes below
cover the one reproduced incompatibility and how to diagnose a failed join.

## GrimAC / PacketEvents kick 26.3 clients

Some servers behind an older **GrimAC** build disconnect 26.3 clients with the generic message
`Invalid packet`. This is a **server-side kick**, not a Horizon codec failure:

- The bundled **PacketEvents** copy inside **GrimAC 2.3.74-ba1a930** does not know every 26.3
  clientbound packet. It fails to map clientbound Play packet `0x8d` (Waypoint) and rejects the
  connection.
- The same kick reproduces with two genuine vanilla 26.3 clients, so Horizon is not the cause.
- Replacing only Grim with **2.3.75-85d931f** removed the failure while keeping ViaVersion and
  Simple Voice Chat installed. A supported Grim release containing that fix is the preferred
  production change.
- A separate PacketEvents jar does not replace the copy shaded inside the old Grim jar.

Horizon needs no workaround for this. If a server still runs an affected Grim build, update it or
connect without that plugin. The original server and its configuration were not modified during
the investigation.

## Controlled compatibility matrix

All servers and clients used fresh temporary directories and worlds, configured for offline test
profiles. No proxy or ProtocolLib was installed in these disposable servers.

| Configuration | Result |
| --- | --- |
| Paper 26.3, no plugins | Two Horizon clients passed: 81 chunks each, chat and `/list` |
| Paper 26.1.2 + ViaVersion 5.12.2 | Two Horizon clients passed: 81 chunks each, chat and `/list` |
| Previous + Simple Voice Chat 2.6.27 | Two Horizon clients passed: 81 chunks each, chat and `/list` |
| Previous + GrimAC 2.3.74-ba1a930 | Server-issued kick, for two Horizon clients, Horizon + vanilla, and two vanilla clients |
| Previous, Grim replaced by 2.3.75-85d931f | Horizon and Horizon + vanilla passed; no PacketEvents exceptions |
| Extended pass on the fixed build | Join/leave/rejoin, visibility across render distance, chat, block break and place |

Real voice transmission, signed online-account chat and production authentication were not
exercised by these offline tests. Online-mode login from iOS remains unvalidated; see
[KNOWN_ISSUES.md](../KNOWN_ISSUES.md).

## Diagnosing a disconnect

Set `HORIZON_PACKET_DIAGNOSTICS=1` before launching. On a local decode/handler failure or a server
disconnect, Horizon logs bounded metadata for the current packet and the previous 64: direction,
state, protocol, packet id/name, payload length, frame length and decoder cursor. It does not
persist packet bodies, chat, cookies, authentication material or profile data, and it redacts
arbitrary server-controlled text.

A server-issued `Invalid packet` is reported as `server-issued play disconnect`, with no preceding
local decoder error. That distinction is covered by a regression test.
