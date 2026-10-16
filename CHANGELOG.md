# Changelog

Each release's notes are taken from its section here by the release workflow.

## [0.3.0]

First public release.

- **Installer:** per-user MSI, no admin needed. Sets up the Claude Code hooks for you (your
  original `settings.json` is kept as `settings.json.agent-pals.bak`) and removes them again on
  uninstall.
- **Updates:** checks GitHub Releases, offers the update in the tray (or installs it
  automatically when no agent is busy, if you choose). Downloads are checked against the
  published SHA-256 before they run.
- **One pal per subagent**, driven by that subagent's own activity. Agents that message each
  other walk over and chat; a subagent reports back to its parent before leaving.
- **Six character families** at double detail (heroes, space-knights, starship-crew, undersea,
  rescue-pups, paper-co), plus the original office and construction crews. **Random** mixes them
  and never repeats a family until all are on screen.
- **Livelier pals:** several work and idle animations per family, their own timing, and they
  turn around now and then.
- **Tray:** Start with Windows, update settings, theme and size.
