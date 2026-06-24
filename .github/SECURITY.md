# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 0.1.x   | ✅        |

Only the latest release receives security fixes. Older tags are unsupported.

## Scope

Min-Mozhi is a **compiler and simulator** — its security surface is different
from a web service. In scope:

- **Malicious input that crashes the compiler** (mimz check, mimz compile,
  mimz sim) — a compiler should never panic or produce undefined behavior on
  any source file, no matter how malformed.
- **Incorrect Verilog output** that silently violates a safety rule the spec
  guarantees (e.g. emitting multiple drivers, latches, or unsigned/signed
  confusion without raising an `E`-code).
- **Supply-chain issues** — malicious dependency, compromised GitHub Action.

Out of scope for this release:

- Compile-time information-flow checks (`secret` taint, `system_fault` network)
  — those are a roadmap item (Phase 2 / spec G5), not yet implemented.
- Synthesized hardware security — Min-Mozhi emits Verilog; hardware correctness
  is the downstream toolchain''s responsibility.

## Reporting a Vulnerability

**Please do NOT file a public GitHub issue for security bugs.**

Use one of:

1. **GitHub private vulnerability reporting** — go to the Security tab on the
   repository page and click ''Report a vulnerability'' (recommended).
2. **Email** — contact the maintainer directly via the email on
   [Naveen R''s GitHub profile](https://github.com/Naveen2070).

Include:

- `mimz --version` output
- Operating system and Rust version
- A minimal reproducer (`.mimz` source file or CLI invocation)
- What you expected vs. what happened

## Response Timeline

| Step                                  | Target                              |
| ------------------------------------- | ----------------------------------- |
| Acknowledgment                        | Within 48 h                         |
| Initial assessment                    | Within 7 days                       |
| Fix or mitigation shipped             | Within 30 days (severity-dependent) |
| Public disclosure (CVE if applicable) | After fix is released               |

## Disclosure Policy

We follow **coordinated disclosure**: you report privately, we fix and release,
then you (and we) disclose publicly. We will credit you in the release notes
unless you prefer to remain anonymous.
