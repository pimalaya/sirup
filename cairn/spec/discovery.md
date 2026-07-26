---
cairn: spec
capability: discovery
status: current
---

# Discovery chain

The wizard resolves an account from a single email, URL or domain input by probing service-discovery sources in a fixed order and taking the first non-empty result. Sirup routes only the two SASL-mediated mail protocols, so any JMAP endpoint a probe surfaces is dropped at the source. The individual PACC, Autoconfig and SRV bricks are consumed directly from `io-pim-discovery` rather than through its combined compose client, to drive a per-source spinner and first-hit-wins UX.

### Requirement: Fixed probe order, first hit wins
Given an email or domain, Sirup SHALL probe sources in this order and take the first non-empty result: PACC, then Thunderbird Autoconfig (ISP main URL, ISP fallback URL, then the ISPDB), then RFC 6186 SRV records.

#### Scenario: PACC resolves first
- GIVEN an email whose domain answers a PACC lookup
- WHEN the wizard resolves the account
- THEN Sirup uses the PACC result and does not consult Autoconfig or SRV

### Requirement: Direct URL skips discovery
A direct `imap` or `smtp` URL input SHALL skip discovery entirely.

#### Scenario: URL input
- GIVEN an `imap://` or `smtp://` server URL
- WHEN the wizard resolves the account
- THEN Sirup uses the URL directly and runs no discovery probe

### Requirement: Drop JMAP endpoints
Any JMAP endpoint surfaced by a probe SHALL be dropped, since Sirup routes only IMAP and SMTP.

#### Scenario: Provider advertises JMAP
- GIVEN a discovery source that returns both a JMAP and an IMAP endpoint
- WHEN Sirup consumes the result
- THEN the JMAP endpoint is discarded and the IMAP endpoint is kept

### Requirement: Configurable resolver
The DNS resolver SHALL be selected from `SIRUP_DNS_RESOLVER`, then the system resolver, before falling back to a default, so a domain is not leaked to a hardcoded public resolver.

#### Scenario: No resolver configured
- GIVEN `SIRUP_DNS_RESOLVER` is unset
- WHEN the wizard needs to resolve SRV records
- THEN Sirup uses the system resolver before any public default
