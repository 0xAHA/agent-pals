# Changelog

Each release's notes are taken from its section here by the release workflow.

## [0.3.3]

- **Pals fade in and out at the edges** of the strip instead of being cut off by it. A pal
  standing in its spot is never faded, and a fading pal passing in front of another blends
  over it rather than leaving a see-through gap.

## [0.3.2]

- **Choose where the pals live:** tray **Position** puts the strip at the left, middle or
  right of the taskbar (`position` in `config.json`). Pals line up from that end and come and
  go by its edge. Right, by the tray, stays the default.

## [0.3.1]

- **Idle pals step away.** A pal that has been idle for 2 minutes walks off; it comes back,
  as the same character, with its agent's next event. A pal never leaves while its agent is
  working (however long a step takes) or while its subagents are still on screen. Change the
  delay with `hide_idle_secs` in `config.json` (`0` keeps pals on screen).
- **Codex support.** Codex's hooks now drive pals just like Claude Code's: working, thinking,
  waiting for permission, done, and a pal per subagent. If Codex is installed, the installer
  adds its hooks to `~/.codex/hooks.json`. Codex only runs new hooks once you approve them: run
  `/hooks` in Codex and trust the Agent Pals entries.
- Hook commands are only quoted when the install path contains a space, so they work in any
  shell.

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
