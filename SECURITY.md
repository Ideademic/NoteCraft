# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub:
**[Report a vulnerability](https://github.com/Ideademic/NoteCraft/security/advisories/new)**
(Security tab › Advisories › Report a vulnerability).

Don't open a public issue or pull request for a vulnerability until a fix is released.

Useful to include:
- the NoteCraft version or commit, and the platform (macOS, Windows, Linux, FreeBSD, web);
- what an attacker can do, and what they need (a crafted `.one`, `.onetoc2` or `.onepkg` file, a
  picture or PDF, a command-line argument, a local account…);
- steps or a sample file that reproduce it. If the sample contains private notes, say so and we'll
  agree on a way to share it.

We'll acknowledge the report, keep you updated while we work on a fix, and credit you in the
advisory unless you'd rather not be named.

## Supported versions

Fixes go into `main` and ship in the next release. Older releases don't get backported fixes.

## Scope

In scope: anything that lets a notebook file, package, picture, PDF, command or command-line
argument read or write files it shouldn't, run code, leak data, use unbounded memory or time, or
crash NoteCraft and lose the user's notes. NoteCraft treats every file as hostile input; a crash
from a crafted file counts, because every panic is a bug (see "Never crash" in
[AGENTS.md](AGENTS.md)).

Out of scope: problems that need an attacker who already controls the user's account or
machine, and bugs in Microsoft OneNote or in other software that opens files NoteCraft writes.
