---
cairn: change
id: per-protocol-servers
status: landed
created: 2026-08-29
---

# One account, one server per protocol

## Why

Sirup is the only Pimalaya binary whose account holds a single server. Everywhere else an account is a person's mailbox and the protocols it speaks are sub-tables of it: himalaya, himalaya-tui, neverest and carillon all read `imap.server` and `smtp.server` under one `[accounts.<name>]`, each block carrying its own TLS, STARTTLS, ALPN and SASL. Sirup instead flattens one protocol into the account and makes the URL scheme mandatory to tell which.

The cost lands on the user, who has to write the same person twice:

```toml
[accounts.fastmail-imap]
server = "imaps://imap.fastmail.com"
sasl.plain.authcid = "pimalaya@fastmail.org"
sasl.plain.passwd.command = ["pass", "show", "pimalaya/fastmail-imap-pop-smtp"]

[accounts.fastmail-smtp]
server = "smtps://smtp.fastmail.com"
sasl.plain.authcid = "pimalaya@fastmail.org"
sasl.plain.passwd.command = ["pass", "show", "pimalaya/fastmail-imap-pop-smtp"]
```

Two accounts for one mailbox, two systemd units, and `-a fastmail` naming neither of them. The same configuration in himalaya is one account, so a user keeping both configurations in step has to translate between two account vocabularies.

It also blocks the queued managesieve-session change, whose own proposal records the constraint as a limitation: "Sirup's scheme is mandatory rather than defaulted, one account serving one protocol, so the bare form is out". That change wants a third protocol beside the other two, which is a third block here and a third account today.

## What

Move the server, the TLS profile, the STARTTLS switch, the ALPN override and the SASL block out of the account and into a per-protocol table, so an account declares as many endpoints as it speaks:

```toml
[accounts.fastmail]
default = true
imap.server = "imap.fastmail.com"
imap.sasl.plain.authcid = "pimalaya@fastmail.org"
imap.sasl.plain.passwd.command = ["pass", "show", "pimalaya/fastmail-imap-pop-smtp"]
smtp.server = "smtp.fastmail.com"
smtp.sasl.plain.authcid = "pimalaya@fastmail.org"
smtp.sasl.plain.passwd.command = ["pass", "show", "pimalaya/fastmail-imap-pop-smtp"]
```

`start` takes the protocols to serve as a positional list, defaulting to every block the account declares. `sirup start` serves both, `sirup start imap` serves one, which is what a per-protocol systemd unit wants. The shape is pimalaya-cli's own, `CompletionCommand` and `ManualCommand` already taking a positional list that defaults to everything.

Five decisions come with it.

**The flat form goes away rather than staying as a shorthand.** Accepting both means either `#[serde(flatten)]`, which silently disables the `deny_unknown_fields` this schema relies on everywhere, or an untagged enum, whose failure is "data did not match any variant" rather than the field that was wrong. Both trade away the error quality cli-002 exists to protect, and the migration is one word per line on a configuration a user writes once. It is a **BREAKING** change, and the right release to make it in is this one, whose `[Unreleased]` already carries two.

**The socket path keys on the protocol.** `<socks-dir>/sirup/<account>.sock` becomes `<socks-dir>/sirup/<account>-<protocol>.sock`, and the `sock-file` override moves into the protocol block. This is the half that reaches outside the repo: himalaya's MIGRATION.md documents `unix:///run/sirup/example.sock` verbatim, so that line moves too.

**Connecting is sequential, serving is parallel.** Every upstream is opened and authenticated before any socket is bound, one spinner at a time, and only then does each protocol get the thread running the accept loop it has today. Spinners from concurrent connects would interleave into noise, and a failure during the connect phase leaves no half-bound daemon behind.

**A partial failure kills the run.** An upstream that cannot open aborts the whole `start`, exactly as it does today. A daemon that ran degraded would leave its unit reading active while half the service is dead, with nothing to retry it.

**The account resolves its secrets through one `SecretResolver`.** This is the regression the change would otherwise ship: the example above names one `pass` entry twice, and resolved block by block that is two key unlocks per start where there is one today. pimalaya-config 0.2's resolver memoizes on `CommandConfig` equality, and the two entries are identical, so it collapses back to one. This is alignment item G2, which did not apply to sirup while an account held a single secret.

Three things fall out for free. The scheme stops being mandatory, a block knowing its own protocol, so `imap.server = "posteo.de"` takes `imaps://` exactly as himalaya's does. The wizard stops asking which protocol to keep when discovery returns both, and generates both blocks instead, which is one prompt fewer. And `repl` takes the protocol as a positional, required only when the account declares more than one: a single standard input cannot drive two sessions, so "all" has no meaning there.
