---
cairn: log
id: per-protocol-servers
date: 2026-08-29
---

# One account, one server per protocol

Sirup's account stopped being a session and became a mailbox. `server`, `sock-file`, `tls`, `starttls`, `alpn` and `sasl` moved out of `[accounts.<name>]` into an `imap` or an `smtp` block, which is the shape himalaya, himalaya-tui, neverest and carillon already read. One provider is now one account where it used to be two, and `-a fastmail` names the mailbox rather than half of it.

The flat form is gone rather than kept as a shorthand. Accepting both would have meant either `#[serde(flatten)]`, which silently disables the `deny_unknown_fields` this schema leans on, or an untagged enum, whose failure is "data did not match any variant" instead of the field that was wrong. Both trade away the error quality cli-002 exists to protect.

`start` takes the protocols to serve as a positional list, defaulting to every block the account declares, which is pimalaya-cli's own shape for `completion` and `manual`. A bare `sirup start` serves the whole account, `sirup start imap` serves the one block a per-protocol service unit wants, and a protocol asked for but not declared is refused naming the ones that are. `repl` takes exactly one, required only beyond a single block, a standard input being unable to drive two sessions.

Opening and serving split. Every upstream is opened and authenticated first, one at a time so the spinners do not interleave and so a provider refusing one leaves no socket bound; only then is a socket bound per session and a thread given to each accept loop. Once serving, the first failure clears a shared flag the other loops poll, so the process exits rather than leaving a supervisor reading the unit as healthy while half of it is dead.

The socket path carries the protocol, `<socks-dir>/sirup/<account>-<protocol>.sock`, `sock-file` overriding it per block. The scheme became optional, a block knowing its own protocol, so `imap.server = "imap.fastmail.com"` takes `imaps://` and a scheme the protocol does not speak is refused naming the ones it does. A new `Protocol` type owns that table along with the ALPN and port defaults, which is also what makes adding the queued ManageSieve block a third variant rather than a third code path.

The regression this would otherwise have shipped is closed: an account resolves every secret through one `SecretResolver`, so the `pass` entry both blocks of the reference account name is spawned once. Verified against the live Fastmail account with a counting shim on the `PATH`: one spawn for a two-block start, both sessions opened, both sockets bound, `LIST` answering over the IMAP socket and `NOOP` over the SMTP one. That is alignment item G2, which did not apply while an account held a single secret.

The wizard stopped asking which protocol to keep. Discovery returning both endpoints generates both blocks around one set of credentials, one prompt fewer for a strictly better result.

Two smaller things rode along. The `repl` now names the socket it could not attach to, an absent one usually meaning `start` is not running rather than anything about the path, and its IMAP half stops on end of input instead of spinning on `BAD Null command` under a pipe.

Capabilities moved: configuration, socket-proxy, wizard.
