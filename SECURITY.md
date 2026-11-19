# Security policy

Please report a vulnerability privately, so that it can be fixed before it is public: on the repository's **Security**
tab choose **Report a vulnerability**, or use
[the form](https://github.com/oxidestack/velocity-rs/security/advisories/new). Only the maintainers see it. Do not open
a public issue, discussion or pull request for one.

Say which version or commit, the template and variables that trigger it, and what an attacker gets (a read outside the
template, code or reflection reached, memory or CPU exhausted). The fix is released with a GitHub security advisory and
credits you unless you prefer not to be named. Fixes go into the latest release.

In scope: the template engine and the tools it ports. A template is code: rendering a template you do not trust is a
deployment decision, but a template that escapes the sandbox the engine documents, or that runs past its render
deadline, is a bug worth reporting. A difference from Apache Velocity's output is a bug, to report as one.
