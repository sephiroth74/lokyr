# Release notes

The notes of each release, which Sparkle's window shows with the new version: one file a release, `<X.Y.Z>.md`, in English, in Markdown. A candidate, `X.Y.Z-rc.N`, and a build of a date, `X.Y.Z-YYYYMMDD`, take their release's notes.

They tell the people who use Lokyr what changed: what is new, what works better, what was fixed, and what to know before updating, as a key of the config that changed. They name no task, phase or ADR.

[CHANGELOG.md](../../CHANGELOG.md), at the repository's root, of the format of [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), holds every release's changes, of which the notes tell what matters most: each change that someone using Lokyr can notice adds its line under `## [Unreleased]` as it is made, and a release moves them under `## [X.Y.Z] - YYYY-MM-DD`, of the release's day, and adds its link at the end; `cargo xtask release` refuses a version that the changelog has no section of, and `cargo xtask check-changelog`, in `just ci`, checks the format.
