# Roadmap

Where velocity-rs is and where it is going. velocity-rs ports to Rust the parts of Apache Velocity that
[mock-server](https://github.com/oxidestack/mock-server) reaches, with the Java library's own output as the measure of
every behaviour ported. The stages below run in order; a finished stage carries the month it was finished.

To suggest something for the roadmap, open an issue. [GOVERNANCE.md](GOVERNANCE.md) says how a larger change is
decided.

## Done

### The port — 2026

The behaviour mock-server relies on, ported and tested against output recorded from the Java library.

## Now: publication

- The repository public on GitHub under Apache-2.0, with the documentation under CC-BY-4.0 and the API documentation on
  docs.rs.
- GitHub Actions on every push and pull request: the build, the tests, clippy, cargo-deny, the publication gate and
  the commit-message gate.
- Repository security: code scanning, OpenSSF Scorecard, dependency review, Renovate, branch protection that requires
  every check, and a sign-off (DCO) check on every pull request.

## Next: the first release

- `v0.1.0` on crates.io, published from its tag through trusted publishing and owned by the oxidestack organisation,
  with its documentation on docs.rs and a GitHub release carrying its notes.

## Later

- The parts of Apache Velocity mock-server does not reach, where users ask for them.
- An OpenSSF Best Practices badge.
- More maintainers, from more than one organisation ([GOVERNANCE.md](GOVERNANCE.md)).
