<!-- A pull request's description opens with its text, not a heading. -->
<!-- markdownlint-disable-file MD041 -->
<!--
What this changes, and why. Link the issue or discussion it comes from: "Closes: #123" in the commit's footer.
CONTRIBUTING.md says how to build, test and write the commits.
-->

- [ ] Every commit follows
      [the commit-message format](https://github.com/oxidestack/velocity-rs/blob/main/CONTRIBUTING.md#commit-messages)
      and is signed off (`git commit -s`).
- [ ] A `feat`, `fix`, `perf` or breaking commit carries a `release-note` block.
- [ ] A change in behaviour comes with a test whose expected values come from Apache Velocity Engine 2.4.1 and Velocity Tools 3.1 itself, or the test that
      failed before it; never values worked out by hand.
- [ ] `cargo test`, `make lint` and `make tools-test` pass.
