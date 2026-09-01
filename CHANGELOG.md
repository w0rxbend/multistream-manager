# Changelog

Everything worth knowing about between one release and the next, written for
somebody deciding whether to upgrade rather than for somebody reading the diff.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Until 1.0, a minor bump may still change how something behaves; anything that
would break an existing setup is listed under **Changed** with what to do.

## [Unreleased]

### Added

- **A pre-flight check before going live.** The go-live key now shows a
  checklist instead of launching straight in: each platform's credentials and
  login, whether a token expires soon *with no refresh token behind it*,
  whether the login predates a permission a newer feature needs, the stream
  details, the selected OBS scene, free disk — and **which OBS audio inputs
  are muted**. Streaming for forty minutes on a muted microphone is the
  classic solo-streamer disaster, and every one of those facts was already
  known to this program and kept to itself until something failed.
  <kbd>Enter</kbd> then writes the metadata to both platforms *and* starts OBS
  streaming, using an explicit start rather than a toggle so a stream you had
  already begun by hand is left running.
- **A health strip in the header, on every tab.** `TW ● live 142 · YT ● live
  38 · OBS ● 6100 kb/s drop 0.2% · 1:23:04`. The colour is computed, not
  decorative: amber for a failed poll (the numbers are older than they look,
  which is not the same as a dead stream) or output-dropped frames over 1%,
  red for the case nothing else on screen reports — OBS still sending while
  the platform has stopped receiving. Every state has its own glyph too, so it
  survives a monochrome terminal.
- **`chat.close`** (<kbd>&lt;Leader&gt;</kbd> <kbd>c</kbd> <kbd>x</kbd>).
  Joining a chat was one keypress and closing one could not be done at all —
  the code existed and no keystroke could reach it.
- **A real text box for chat.** The composer supported appending a character
  and deleting the last one, and nothing else. It now has arrows, Home/End,
  Delete, Ctrl+W and Ctrl+U, plus <kbd>↑</kbd>/<kbd>↓</kbd> through what you
  have already sent in that chat. One Backspace removes a whole emoji,
  including multi-part ones like flags and families.
- **Config → Chat**, with the switch for chat logging — which previously
  could only be turned on by editing config.toml, while Housekeeping's
  paid-event export read the very logs it produces. The section also reports
  the day's YouTube quota estimate and what the log rotation has produced.
- **A test-notification row** in Config → Notifications, and separate switches
  for polls and predictions.
- **Fine volume control on the OBS tab** (<kbd>]</kbd> / <kbd>[</kbd>, 1%),
  for settling on a level rather than finding one.

### Fixed

- **Turning pop-ups off silenced every error.** `appearance.toasts = false`
  suppressed all notifications, and the activity log is only drawn on the
  Stream Info tab — so a failed go-live while you were reading chat produced
  nothing at all. The setting now means "no *routine* pop-ups"; problems are
  always shown.
- **Twitch bans tombstoned nothing on YouTube.** A YouTube ban emitted the
  banned viewer's display name where the matcher wanted the channel id, so
  banning reported success and left every message they had written on screen.
- **Turning Twitch events off and on again deafened the app** for the rest of
  the session.
- **A dropped command left the interface permanently busy** — every later
  go-live, login and end-stream silently refused with "already working" until
  restart.
- **The statistics polling spent YouTube quota nobody was counting.** At the
  default interval that is roughly 5,700 units a day against a default
  10,000-unit project, invisible to the reserve that exists to keep message
  sending working when reading has to stop.
- **Clicking "4 OBS" or "5 Config" did nothing.** The tab bar drew five labels
  and the mouse hit-testing was built from a separate copy that listed three.
- **The Config tab's footer advertised another tab's keys.**
- **The emoji picker could only ever insert its first match**, though it drew
  a list; and the layout editor's preset key was not a cycle.
- **Twitch tags could be set but never cleared.**
- **Retry ladders that did not climb.** One successful poll wiped YouTube's
  backoff; Twitch reset its reconnect ladder on any accepted connection, so a
  flapping connection retried every two seconds indefinitely; and Twitch
  reconnects had no jitter.
- **A failed save of renewed tokens reported success**, and logging out did an
  unlocked read-modify-write a concurrent refresh could undo.
- **`msm.log` grew without bound**, and a failed logging init was discarded so
  "check msm.log" pointed at a file nothing was writing.
- **A config file that could not be parsed was replaced without a backup.**
  It is now kept as `config.toml.bak`.
- **Diagnostics went stale** — it never retook its snapshot after a login or
  an OBS connection — reported credential *presence* when the documented
  footgun is a question of *source*, and could not be scrolled.

### Changed

- **The command palette is built from the action set** rather than a
  hand-written list of 31 entries. Most of the OBS actions, chat scrolling,
  account cycling and everything on the Config tab were previously in no list
  anywhere, which quietly broke the palette's whole promise. Matching now
  ranks prefix matches above mid-word ones.
- **Category autocomplete spends one search per word, not one per keystroke.**
  Typing "Baldur's Gate 3" was fifteen Helix calls, fourteen discarded.
- **Chat paging follows the height of the pane** rather than a fixed ten
  lines.
- **An OBS shortcut that collides with a binding is reported.** Such a
  shortcut never fires — the keymap is resolved first, deliberately — and
  nothing said so. docs/configuration.md described the precedence backwards.

## [0.2.0] — 2026-08-18

### Added

- **Desktop notifications for stream events.** Raids, subscriptions, gifted
  subs, cheers, Super Chats and memberships now reach your desktop's own
  notification service, not just the Chat tab — because during a stream the
  terminal is usually behind OBS. Raids are sent as *critical*, which most
  desktops show even under do-not-disturb. Needs nothing installed in the
  common case: `notify-send`, then `gdbus`, then `kdialog`, then the terminal
  bell. Everything is switchable in **Config → Notifications** or the new
  `[notifications]` section.
- **Twitch events that never touch chat** — new followers, channel-point
  redemptions with the viewer's text, hype trains, polls and predictions —
  over a second connection (EventSub). A follow does not appear in any chat
  window, so until now this program could not see one at all.
- **Finish the broadcast** (<kbd>Space</kbd> <kbd>s</kbd> <kbd>x</kbd>). Going
  live created a YouTube broadcast and nothing could close one, so the only way
  to end a session cleanly was YouTube Studio. Asks twice, because a completed
  broadcast cannot be reopened, and deliberately does not stop OBS.
- **Twitch moderation from the chat pane.** <kbd>d</kbd> delete, <kbd>b</kbd>
  ban and <kbd>t</kbd> time out worked on YouTube and refused on Twitch; they
  now work on both, via Twitch's Helix endpoints.
- **`/raid <channel>` and `/unraid`**, the usual way a Twitch stream ends.
- **YouTube thumbnails and scheduling.** A `Thumbnail (YouTube)` field takes a
  JPEG or PNG up to 2MB, and `Start time (YouTube)` accepts `20:00`,
  `2026-08-20 20:00`, or `+2h` — scheduling ahead creates the watch page
  immediately so it can be shared and viewers can set a reminder.
- **Credentials from the environment**, for every one of them, each with its
  own `*_env` key so it can be pointed at whatever your password manager
  already uses. The OBS host and port can now come from the environment too,
  which is what makes one dotfiles repository work across machines.
- **`--version` and `--help`** as real options.
- **Advisory and licence checking in CI** (`cargo-deny`, weekly as well as on
  every push), and this changelog.

### Changed

- **Slash commands are refused rather than posted.** Typing `/ban someviewer`
  into the composer used to post those words to everybody watching: Twitch
  removed chat commands from IRC in 2023 and YouTube never had them. Anything
  beginning with a slash that this program does not recognise is now refused,
  with a note saying what to use instead. `//text` posts a leading slash on
  purpose.
- **Chat notifications no longer require the chat to be off-screen.** The old
  rule assumed you were reading chat in this program. Set
  `[notifications] only_when_hidden = true` for the previous behaviour.
- **A burst of events is paced, not dropped.** The old notifier discarded
  anything arriving within two seconds of the last one, so a raid landing in
  the middle of a gift drop was lost. They queue and release one per gap.
- **Config → Appearance's "Notifications" row is now "In-app pop-ups"**, to
  tell it apart from the new Notifications section next door.
- **Logging in again is needed for the new Twitch features.** Moderation,
  raids and events each need permissions Twitch only grants at authorisation,
  so a saved login cannot acquire them: **Config → Accounts**, log out and back
  in. Everything else keeps working meanwhile, and anything that cannot work
  says which permission it is missing.

### Fixed

- **Config → Diagnostics no longer runs its checks on every frame.** It looked
  for clipboard helpers by starting them, up to six process launches per
  redraw — twice a second at rest and ten times a second while animating, on
  the thread that has to answer keystrokes. It is a snapshot now, taken when
  the section opens, with `r` to take a fresh one.
- Running without a terminal (piped, or under a service manager) explains
  itself instead of failing with `No such device or address (os error 6)`.
- An unrecognised command-line argument exits 2 rather than 0.

## [0.1.0] — 2026-08-08

First release: configure and go live on Twitch and YouTube from one terminal,
read and answer both chats side by side, and drive OBS — scenes, audio,
streaming and recording — from a fourth tab. Fifty-seven themes, a configurable
keymap with AstroNvim-shaped defaults, an arrangeable combined view for a second
monitor, and no command line at all.

[Unreleased]: https://github.com/worxbend/multistream-manager/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/worxbend/multistream-manager/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/worxbend/multistream-manager/releases/tag/v0.1.0
