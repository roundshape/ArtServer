# Security policy

English | [日本語](SECURITY.ja.md)

## Supported versions

ArtServer has no releases yet. Only the latest commit of `main` is supported, and fixes are made there.

## Reporting a vulnerability

Please don't report security problems in public issues or pull requests.

Report them privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**
(GitHub's private vulnerability reporting). Only you and the maintainers can see the report.

Please include:

- What the problem is, and what an attacker could do with it
- How to reproduce it: the ArtServer commit, the OS, how ArtServer was started (its options), and the requests or files that trigger it
- A proof of concept, if you have one. Please try it only on a server of your own

Reports are handled on a best-effort basis, without a guaranteed response time. You will be told whether the problem
could be confirmed and what is planned for it.

A published advisory (such as one from RustSec) for one of ArtServer's dependencies is already public, so an ordinary issue is fine for it.

## Scope

ArtServer is meant for a few trusted clients in one organization, running locally or in the organization's own network,
and not exposed to the internet ([design policy, section 1.1](docs/design-policy.md)). Reports on these are especially welcome:

- Getting past the Bearer token, or the Host and Origin checks
- Running engine commands that the allowlist refuses, or reading or writing files on the server
- Fetching another session's exports or uploads, or guessing their URLs
- Getting around the limits on sessions, request size, documents, history and run time

The points in [Known limitations](README.md#known-limitations) (no TLS, one token, all sessions in one process,
files read in the same process, no rate limiting) are known. A report showing that one of them is worse than described is still welcome.

Problems in the [VectorCraft](https://github.com/storytold/vectorcraft) engine that affect ArtServer can be reported here too.
They are passed on to VectorCraft's developers as needed.
