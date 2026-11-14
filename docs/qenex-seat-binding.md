# QENEX seat binding

When Zed runs inside a QENEX seat, it keeps its editor state inside that
seat's workspace. One bot never reads or overwrites another bot's settings,
keymaps, database or logs.

This document describes how to run Zed in a seat, where its files go, and how
to opt out.

## Requirements

Set two environment variables before starting Zed:

- `QENEX_SEAT` - the seat identifier, for example `coder`. Leave it unset
  or empty to use the global locations.
- `QENEX_SEAT_WORKSPACE` - the absolute path of the seat workspace. It is
  the only writable location while the process is seat-bound.

```sh
export QENEX_SEAT=coder
export QENEX_SEAT_WORKSPACE=/var/lib/qenex/seats/coder/workspace
zed
```

## Where Zed stores its state

While seat-bound, Zed resolves its writable roots inside the seat workspace:

- Configuration: `$QENEX_SEAT_WORKSPACE/.config/zed`
- Data (database, extensions, language servers):
  `$QENEX_SEAT_WORKSPACE/.data/zed`
- State: `$QENEX_SEAT_WORKSPACE/.state/zed`
- Logs: `$QENEX_SEAT_WORKSPACE/.data/zed/logs`
- Temporary files: `$QENEX_SEAT_WORKSPACE/.tmp/zed`

Everything else stays where it was. Reads from system and toolchain locations
are unaffected, and project settings work as usual.

Without `QENEX_SEAT`, Zed keeps its usual platform locations, for example
`$XDG_CONFIG_HOME/zed` and `$XDG_DATA_HOME/zed` on Linux.

## Checking the seat

Zed prints its seat identity in its system specs:

```sh
zed --system-specs
```

When the process is seat-bound, the output contains a line like:

```
seat=coder workspace=/var/lib/qenex/seats/coder/workspace
```

Without a seat, no seat line is printed.

## Writes outside the workspace

While seat-bound, Zed refuses to write outside the seat workspace. For
example, pointing `--user-data-dir` at a global directory fails with an error
that names the offending path:

```
refusing to write outside the QENEX seat workspace: /home/user/.local/share/zed (seat workspace: /var/lib/qenex/seats/coder/workspace); set QENEX_SEAT_ALLOW_GLOBAL=1 to allow writes outside the seat workspace
```

## Opting out

Set `QENEX_SEAT_ALLOW_GLOBAL=1` to allow writes outside the seat workspace
again. Use this only for maintenance, for example when a seat needs to read a
shared extension directory.

## What's next

The seat runtime service that exposes a seat's execution surface ships with
the QENEX rust-sdk repository.
