# Cloud saves

Slatty synchronises save folders with GOG's cloud storage, and an Epic game's with Epic's (see
[Epic games](#epic-games)). Losing a save is the worst thing
a launcher can do, so the design favours refusing over guessing. This page describes the rules and
their known limits.

## What is synchronised

- For each game, GOG's remote configuration lists the save locations of its Windows build. Each has
  a name and a path template such as `<?SAVED_GAMES?>/id Software/DOOM/base`.
- Templates are resolved inside the game's Wine prefix (`drive_c/users/steamuser/…`, or your user
  name for a game run through Wine alone), or inside the game folder for `<?INSTALL?>`. Path parts
  are matched case-insensitively, as on Windows.
- If GOG turns cloud saves on for a game but lists no location, the Galaxy SDK's storage folder is
  used (`<?APPLICATION_DATA_LOCAL?>/GOG.com/Galaxy/Applications/<client id>/Storage/Shared/Files`).
- Before a game's first launch its prefix does not exist yet. A check still shows what will be
  downloaded, but nothing is written: Proton must create the prefix first, so the saves arrive at
  the first launch, just before the game starts.

Not synchronised: files saved through the Galaxy SDK's storage outside these folders; native Linux
builds (GOG lists no save locations for them); Steam's games (the Steam client syncs them) and
games added by hand (GOG does not know them); links and names that are not UTF-8 below a save
folder, which are reported.

## How a change is detected

Every synchronised file has a history entry for its account, game and location, recorded at the
last successful sync: the SHA-256 of the local content, and the hash GOG listed for the cloud copy.
On the next sync, each side is compared with that entry: the local file changed if its SHA-256
differs, the cloud file changed if its listed hash differs. Modification dates are never used.

"Deleted" below means a file with history that is gone; "missing", a file without history that is
not there.

| Local | Cloud | Action |
|---|---|---|
| unchanged | unchanged | nothing |
| changed | unchanged | upload |
| unchanged | changed | download |
| changed | changed | contents compared: identical files are adopted, different ones are a **conflict** |
| deleted | unchanged | delete in the cloud (needs permission, see below) |
| unchanged | deleted | delete locally (needs permission) |
| deleted | deleted | history forgotten |
| deleted | changed | **conflict** |
| changed | deleted | **conflict** |
| exists | exists, no history | contents compared: identical files are adopted, different ones are a **conflict** |
| exists | missing | upload |
| missing | exists | download |

## What is protected

- **Conflicts never settle themselves.** A file in conflict is left alone on both sides; the other
  files of the sync still go up and down. You choose with `--prefer local` or `--prefer remote`
  (or the buttons of the Cloud saves drawer), which settles every conflict of the game at once, in
  all its locations. The version you do not keep is saved in the backups.
- **A local file is never overwritten without a copy.** It is first copied to
  `~/.local/share/slatty/backups/<user>/<game>/<time>/<location>/local/`. The new content is
  written under a temporary name, then renamed into place.
- **Nor is a cloud file.** Before an upload replaces it (a game that crashed while saving would
  otherwise put a damaged save over a good one) or a deletion removes it, the cloud version is
  downloaded to `…/<location>/remote/` beside the local copies. Backups are flushed to the disk
  and never deleted by Slatty. Each sync that backs up files gets a folder of its own, even
  two syncs in the same second.
- **Deletions need permission.** They are applied only with `slatty cloud sync <id>
  --allow-deletions`, which covers that whole sync; the interface never passes it on its own. Two
  conflicts are the exception: the cloud deleted a file you changed and you keep the cloud's side,
  or you deleted a file the cloud changed and you keep yours. That choice is the permission; the
  copy that goes is backed up first.
- **A save is checked again just before it is replaced or deleted.** If the game, or anything
  else, changed or created it while the sync waited on the network, it is left alone and reported.
  A deletion held for later is checked again the same way. Nothing is written or deleted through a
  symbolic link below the save folder.
- **Uploads check the cloud again.** Just before uploading, the cloud is listed again; a file that
  changed meanwhile is not overwritten and becomes a conflict at the next sync.
- **Network failures leave local files untouched**, and history only advances for files actually
  transferred. An upload the cloud listing could not confirm gets no history; the next sync finds
  the same content on both sides and adopts it.
- **History is per account.** Another account never inherits it.
- **One sync per game at a time.** A sync takes the game's lock: it is refused while the game is
  played, installed, updated or synced elsewhere. A check, which writes nothing, does not take it.
- **Binary saves are never merged.**

## What is refused or left out

- **A suspicious folder blocks deletions,** even with permission:
  - the local folder is missing or empty while history says it had saves (a new prefix after a
    reinstall, a folder deleted or moved). The cloud copies are then downloaded back, as on a new
    computer: a folder that disappeared as a whole was not emptied on purpose;
  - the cloud is empty while history says it had saves (another account, a service problem);
  - the save folder's path changed since the last sync, another account synced the same folder
    since (accounts that own a game share its prefix), or the folder was deleted and made again
    since (its device and inode changed). Its history is then ignored, and files that differ become
    conflicts, so a new game is never uploaded over your progress. A folder moved to another disk
    or restored from a backup counts as made again, once.
- **Cloud names that cannot be trusted are left out** and reported: a name containing `..` or an
  absolute path, and pairs of names that differ only by case (`Save.dat`, `save.dat`: one file on
  Windows), whose local file is then left alone too. The location's other saves still sync, and
  the game starts.
- **A whole location is refused** when its name is not a plain name (it names its backup folder
  and its files in the cloud), when its template cannot be resolved, when two local files differ
  only by case, or, for an isolated game, when its folder is reached through a link out of the
  prefix or game folder.

## Around a game session

1. **Before the game starts:** sync. What does not conflict is applied. The launch stops, with the
   reason, on a conflict, a refusal, a file that failed to transfer or a refused location.
   It does not stop when the cloud cannot be reached (offline, a dropped connection, a server error
   or too many requests, for the whole game or one location) or cannot be used (signed out): the
   game starts with its local saves, and the session says which locations were not synced.
2. **After the session ends** (every game process has exited): sync again, which uploads what the
   game wrote. A location the cloud could not be reached for is told as not uploaded.
3. **If the end of the session is uncertain** (the launcher lost track of the game), nothing is
   uploaded. The next launch reports the interrupted session.

## Epic games

Epic keeps whole snapshots of a game's save folder rather than files: each is a manifest named
after the time it was made (`manifests/2026.10.27-10.00.00.manifest`) and the chunks it lists,
in the format of Epic's builds. The rules above still decide, file by file:

- **The folder** is the one Epic's catalog names (`CloudSaveFolder`), resolved in the prefix as
  GOG's are: `{AppData}` is `AppData/Local`, `{UserDir}` `Documents`, `{UserSavedGames}`
  `Saved Games`, `{InstallDir}` the game folder, `{EpicID}` your Epic account id. `..` climbs
  within your user folder or the game's, as Unity's games write `{AppData}/../LocalLow/…`, never
  out of them, and the folder must be under `AppData`, `Documents` or `Saved Games` (or the game
  folder). Another variable refuses the location. Only the files Epic's lists of files to include and exclude
  keep (`CloudIncludeList`, `CloudExcludeList`, matched as legendary matches them, regardless of
  case) are saves: the others are left alone on both sides.
- **The cloud's files** are those of the newest snapshot, the one stored last; their hash is the
  SHA-1 the snapshot lists for each. History, conflicts, deletions and backups work as above, for
  your Epic account.
- **What a sync uploads or deletes is sent as one new snapshot,** made from the newest one listed
  just before: the files not changed are read back from it, the uploads replace theirs, the
  deletions are left out. Its chunks are stored first and its manifest last, so a snapshot never
  names a chunk not stored yet; it is named after every one stored. Older snapshots are never
  deleted: the copies before a change stay in Epic's cloud as well as in the backups.
- **The last files are never deleted there**: Epic keeps no empty snapshot, and that deletion is
  reported instead.

legendary, which Heroic uses, decides from the files' dates instead; the two read and write the
same snapshots.

## Known limits

- **Overwrite window.** GOG's storage API is not known to support conditional uploads. A change
  made by another computer *after* the last check and *before* the upload finishes can be
  overwritten. The window lasts as long as one upload. A test
  (`residual_window_after_fresh_listing_is_not_detected`) pins this behaviour so it is not
  forgotten.
- **Ignored marker.** Cloud entries whose hash is `aadd86936a80ee8a369579c3926f1b3c` are ignored, as
  heroic-gogdl does. Their meaning is not known: it is neither an empty file nor an empty gzip
  stream.
- **Transfer format.** Uploads are gzip-compressed, with the same metadata headers as heroic-gogdl;
  whether GOG requires those headers is not verified. Downloads were verified to arrive
  decompressed.
- **Memory.** A save is held whole in memory while it transfers.
- **Epic: read by Slatty, not yet by Epic's launcher.** The snapshots follow legendary's code
  (chunks of 1 MiB padded with zeros, manifests of version 18, the folder's template kept as
  `CloudSaveFolder`). One sent to Epic was read back identical (see
  [compatibility.md](compatibility.md)); whether Epic's own launcher reads them is not verified. Each sync that
  changes the cloud sends every save of the folder again, read back from the newest snapshot.
