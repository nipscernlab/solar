# Reporting a vulnerability in SOLAR

Write to **chrysthofer.afonso@cern.ch**. Say what you found, what it lets someone do, and
how to reproduce it. A proof of concept, however rough, is worth more than a careful
description.

Please do not open a public issue for something exploitable. There is no embargo policy
to negotiate and no bounty to claim: this is a laboratory, the fix will be written as soon
as it is understood, and you will be credited in `CHANGELOG.md` unless you would rather
not be.

You should get an answer within five working days. If you do not, write again to
chrysthofer.afonso@cern.ch and copy the group: Chrysthofer Arthur Amaro Afonso is the
technical coordinator of NIPS-CERN, and Prof. Luciano Manhães de Andrade Filho heads the
group.

## What is in scope

- The `solar` binary and the crates of this repository.
- The protocol itself, where a message can make SOLAR do something the contract does not
  allow: escape the boundary, read or write something it never declared, or hang a
  session that the timeouts should have ended.
- Anything that lets a caller reach past SOLAR into the machine it runs on.

## What is not, yet

- SAPHO, AURORA, CGV and the other projects of the laboratory. They have their own
  repositories, and the same address reaches the same person.
- Denial of service through sheer volume. `solar/1` is a program on standard input and
  standard output with no network listener; whoever can send it a million requests can
  already run programs on that machine.

## What SOLAR promises, so that you know what counts as broken

Section 1 of [the contract](docs/CONTRACT.md) makes three promises: every call gets a
response, a response says as much as it can, and every API follows the same template.
A way to make SOLAR answer nothing, hang forever, or answer something it never declared
is a bug worth reporting even if you cannot see how to exploit it.

## Versions

SOLAR is `0.2.0`, tagged but not published as an artefact. When there are releases, this section will
say which ones get fixes. Until then, the fix goes on `main`.
