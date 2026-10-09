# Cloud saves

SlattyLauncher synchronises save folders with GOG's cloud storage. Losing a save is the worst thing a
launcher can do, so the design favours refusing over guessing. This page describes the rules and
their known limits.

## What is synchronised

- For each game, GOG's remote configuration lists the save locations of the Windows build. Each
  location has a name and a path template such as
  `<?SAVED_GAMES?>/id Software/DOOM/base`.
- Templates are resolved inside the game's Wine prefix (`drive_c/users/steamuser/…`), or inside the
  game folder for `<?INSTALL?>`. Path components are matched case-insensitively, like on Windows.
- If GOG enables cloud saves but lists no location, the Galaxy SDK storage folder is used
  (`<?APPLICATION_DATA_LOCAL?>/GOG.com/Galaxy/Applications/<client id>/Storage/Shared/Files`).

Files saved through the Galaxy SDK storage API that live outside these folders are not
synchronised. Native Linux builds have no documented cloud locations and are not synchronised
either.

## How a change is detected

Every synchronised file has a history entry for its account, game and location, recorded at the last
successful sync:

- the SHA-256 of the local content;
- the hash GOG listed for the cloud copy.

On the next sync, each side is compared with that entry:

- the local file has changed if its SHA-256 differs;
- the cloud file has changed if its listed hash differs.

Modification dates are never used to decide.

| Local | Cloud | Action |
|---|---|---|
| unchanged | unchanged | nothing |
| changed | unchanged | upload |
| unchanged | changed | download |
| changed | changed | **conflict** |
| deleted | unchanged | delete in the cloud (needs permission, see below) |
| unchanged | deleted | delete locally (needs permission) |
| deleted | changed | **conflict** |
| changed | deleted | **conflict** |
| exists | exists, no history | contents compared: identical files are adopted, different ones are a **conflict** |
| exists | missing, no history | upload |
| missing | exists, no history | download |

## Safety rules

- **Conflicts never resolve themselves.** Nothing is written, and the launch is stopped. You choose
  with `--prefer local` or `--prefer remote` (or the buttons in the interface). The version you do
  not keep is saved in the backups folder.
- **A local file is never overwritten without a copy.** It is first copied to
  `~/.local/share/slatty/backups/<user>/<game>/<time>/<location>/local/`. The new content is
  written to a temporary file, then renamed into place.
- **Deletions need permission.** They are applied only with `--allow-deletions`, file by file.
- **A suspicious folder blocks deletions.** In these cases deletions are refused, even with
  permission:
  - the local folder is missing or empty while history says it had saves (wrong prefix, game never
    run, folder moved);
  - the cloud is empty while history says it had saves (wrong account, service problem);
  - the save folder path changed since the last sync. Its history is then ignored, and files that
    differ become conflicts.
- **History is per account.** Another account never inherits it.
- **One sync per game at a time.** A lock refuses a second sync on the same game.
- **Uploads re-check the cloud.** Just before uploading, the cloud is listed again; a file that
  changed in the meantime is not overwritten and becomes a conflict at the next sync.
- **Network failures leave local files untouched**, and history only advances for files that were
  actually transferred.
- **Cloud paths cannot escape the save folder.** A cloud file name containing `..` or an absolute
  path is rejected.
- **Case-only duplicates are refused.** `Save.dat` and `save.dat` are one file on Windows, so a
  folder holding both is reported instead of being synchronised.
- **Binary saves are never merged.**

## Around a game session

1. **Before launch:** sync. Downloads and uploads that do not conflict are applied. A conflict or an
   error stops the launch. If the cloud cannot be reached, the game starts with local saves.
2. **After the session ends** (every game process has exited): sync again, which uploads what the
   game wrote.
3. **If the end of the session is uncertain** (the launcher lost track of the game), nothing is
   uploaded. The next launch reports the interrupted session.

## Known limits

- **Overwrite window.** GOG's storage API is not known to support conditional uploads. A change made
  by another machine *after* the last check and *before* the upload finishes can be overwritten. The
  window lasts as long as one upload. A test (`residual_window_after_fresh_listing_is_not_detected`)
  pins this behaviour so it is not forgotten.
- **Ignored marker.** Cloud entries whose hash is `aadd86936a80ee8a369579c3926f1b3c` are ignored, as
  heroic-gogdl does. Their meaning is not known: it is neither an empty file nor an empty gzip
  stream.
- **Transfer format.** Uploads are gzip-compressed, with the same metadata headers as heroic-gogdl;
  whether GOG requires those headers is not verified. Downloads were verified to arrive
  decompressed.
