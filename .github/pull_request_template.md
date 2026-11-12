## What this changes

<!-- What the pull request changes, and why. Link the issue, as "Closes #12". -->

## How it was checked

<!-- The tests that it adds, and the checks by hand that no test can make, with what you saw: a capture of the window, a measure, an input method tried. -->

## Checklist

- [ ] `just ci` passes: formatting, clippy, the tests, `cargo deny`, the dependency rules, the comments and the links.
- [ ] New code has tests, and a fix has the test that would have caught its bug.
- [ ] The comments describe the code, and name no task, phase or ADR.
- [ ] Every `unsafe` block has its `// SAFETY:` comment, in a crate that allows `unsafe`.
- [ ] The documents follow the change: a new ADR for a changed decision, [CONFIGURATION.md](https://github.com/sephiroth74/lokyr/blob/main/docs/CONFIGURATION.md) for a key of the config, and the documents of a changed behavior.
- [ ] Each commit builds on its own, and its message tells what changed and why.
