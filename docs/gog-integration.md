# GOG integration

GOG publishes no API for third-party launchers. Slatty signs in with GOG Galaxy's public
OAuth client and uses the services Galaxy uses. This page lists every GOG service involved, where
the knowledge comes from, and whether it was checked against GOG.

**Status:**
- **Verified:** used successfully against real GOG services (see [compatibility.md](compatibility.md)).
- **Implemented:** coded and tested against simulated services only.

Requests name Slatty in their `User-Agent`, except those to cloud storage, which send the
Galaxy user agent heroic-gogdl sends.

## Authentication

| Use | Request | Source | Status |
|---|---|---|---|
| Sign-in page (system browser) | `GET https://auth.gog.com/auth?client_id=46899977096215655&redirect_uri=https://embed.gog.com/on_login_success?origin=client&response_type=code&layout=galaxy` | gogapidocs, Heroic | Verified |
| Code exchange | `GET https://auth.gog.com/token?grant_type=authorization_code&…` | gogapidocs, heroic-gogdl | Verified |
| Session refresh | `GET https://auth.gog.com/token?grant_type=refresh_token&…` | gogapidocs, heroic-gogdl | Verified: the refresh token is not rotated, the previous one stays valid |
| Game-scoped token | The same, with the game's Galaxy `client_id` and `client_secret` and `without_new_session=1` | Comet, heroic-gogdl | Verified |
| User name | `GET https://users.gog.com/users/{user_id}`, with the token | Heroic | Verified |
| Avatar | The same address, without a token (public profile) | Heroic | Verified |

The Galaxy client id and secret are public: they ship with every Galaxy install and every launcher
that talks to GOG. GOG accepts only Galaxy's redirect address. No local redirect, custom OAuth
client or PKCE flow is known to work, which is why the user pastes the final address back.

## Library

| Use | Request | Source | Status |
|---|---|---|---|
| Owned products | `GET https://galaxy-library.gog.com/users/{user_id}/releases` (paged with `page_token`) | Heroic | Verified |
| Metadata and artwork | `GET https://gamesdb.gog.com/platforms/gog/external_releases/{id}`, with the release's `X-GOG-Library-Cert` when the library gave one | Heroic | Verified |
| The same game on Steam, to show a game owned on both once | The same answer: `game.releases[]` with `platform_id` `steam` and the app id as `external_id` (other stores and Steam bundles, `bundle_<id>`, left out) | Heroic's cache of these answers | Implemented |
| Fallback title | `GET https://api.gog.com/products/{id}` | gogapidocs | Implemented |
| Whether anything can be installed | `GET https://api.gog.com/products/{id}`: `is_installable`, and `game_type` (`pack` for a bundle) | Capture | Verified (The Elder Scrolls IV: Oblivion GOTY Deluxe: the pack `false`, the game `true`) |
| Owned products, for DLC | `GET https://embed.gog.com/user/data/games` (`owned`: product ids) | heroic-gogdl | Verified |

## Windows builds (Galaxy content system, generation 2)

| Use | Request | Source | Status |
|---|---|---|---|
| Builds | `GET https://content-system.gog.com/products/{id}/os/windows/builds?generation=2` | gogapidocs, heroic-gogdl | Verified |
| A game's Galaxy client id and secret | The same for `os/windows`, then `os/osx` when there is no Windows build: the newest build's metadata | heroic-gogdl | Verified (Windows) |
| Build metadata (zlib JSON: depots, languages, install folder, client id and secret, dependencies) | The build's `link` | gogapidocs, heroic-gogdl | Verified |
| Depot manifests (zlib JSON: files, chunks with compressed and decompressed MD5) | `GET https://gog-cdn-fastly.gog.com/content-system/v2/meta/{ab}/{cd}/{hash}` | heroic-gogdl | Verified |
| Download links | `GET https://content-system.gog.com/products/{id}/secure_link?_version=2&generation=2&path=/` | heroic-gogdl | Verified |
| Chunks | The link's `url_format`, with `path` extended by `/{ab}/{cd}/{compressedMd5}` | heroic-gogdl | Verified |

Each product (the game and every DLC) has its own download links; DLC chunks are fetched through the
DLC's links. Download links expire; on 401 or 403 they are asked for again.

GOG's answer lists several CDN endpoints. Slatty measures each on a first request and uses
the fastest, and asks a late chunk again from another endpoint (verified: on a 17 MB/s connection,
the endpoint GOG listed first gave about 1 MB/s, the second up to 13 MB/s).

Chunks hold up to 10 MiB of file data (seen on Undertale: 10,485,760 bytes, the last one slightly
larger). The decompressed MD5 identifies a chunk's content, which is what allows reusing chunks
already on disk.

### Binary patches

| Use | Request | Source | Status |
|---|---|---|---|
| Find a patch | `GET https://content-system.gog.com/products/{id}/patches?_version=4&from_build_id=…&to_build_id=…` with the account token; returns `{"link": …}`, or 404 | heroic-gogdl | Verified (Hollow Knight 1.5.12618 → 1.5.12620: a patch of 354 files; none for Undertale 1.06 → 1.08, nor between builds that do not follow each other) |
| Patch description | The link (zlib JSON): `algorithm` (`xdelta3`), `baseProductId`, `depots` with `productId`, `languages`, `manifest` | heroic-gogdl | Verified |
| File diffs | `GET https://gog-cdn-fastly.gog.com/content-system/v2/patches/meta/{ab}/{cd}/{manifest}`: `DepotDiff` items with `path_source`, `path_target`, `md5_source`, `md5_target` and delta chunks | heroic-gogdl | Verified |
| Delta chunks | Download links from `secure_link` with `&root=/patches/store` | heroic-gogdl | Verified (11 of Hollow Knight's deltas applied, each matching GOG's target MD5) |

Deltas are VCDIFF as produced by xdelta3 without secondary compression (heroic-gogdl builds xdelta3
with `SECONDARY_DJW=0` and `SECONDARY_LZMA=0`). The match is on the whole-file MD5 of the source;
the rebuilt file must have `md5_target`.

### Dependencies and post-install setup

| Use | Request | Source | Status |
|---|---|---|---|
| Dependency repository | `GET https://content-system.gog.com/dependencies/repository?generation=2`, then its `repository_manifest` (zlib JSON: dependency id, executable path and arguments, manifest) | heroic-gogdl | Verified |
| Dependency manifests | `GET https://gog-cdn-fastly.gog.com/content-system/v2/dependencies/meta/{ab}/{cd}/{hash}` | heroic-gogdl | Verified |
| Dependency downloads | `GET https://content-system.gog.com/open_link?generation=2&_version=2&path=/dependencies/store/`, chunks under the returned `url` | heroic-gogdl | Verified (script interpreter) |
| Setup data | Build metadata `scriptInterpreter`, and each product's `temp_executable` | Heroic | Verified (`scriptInterpreter` on Undertale) |

Dependencies whose executable lives under `__redist/` are shared and installed at the first launch;
the others ship files into the game folder and are installed with it, as heroic-gogdl does. The
script interpreter and setup programs receive the same arguments as in Heroic (`/VERYSILENT`,
`/DIR=`, `/ProductId=`, `/supportDir=`, …).

## Linux builds (offline installers)

GOG's content system has no Linux builds: it answers `Unsupported OS`.

| Use | Request | Source | Status |
|---|---|---|---|
| Installers of a game and its DLC | `GET https://api.gog.com/products/{id}?expand=downloads,expanded_dlcs`, with the token: `downloads.installers[]` with `os` `linux`, `language`, `version` and `files[].downlink` | gogapidocs, heroic-gogdl | Verified (Hollow Knight, Hollow Knight: Silksong) |
| Download link | `GET` the file's `downlink` with the token (an address on `gog.com`): `{"downlink": …}` | heroic-gogdl | Verified |
| Installer bytes | Ranged requests to that link. Byte ranges are served, suffix ranges (`bytes=-N`) are not; the exact size comes from the `Content-Range` of `bytes=0-0`, the API's size being rounded. An expired link (401, 403, 410) is asked for again | Capture | Verified |
| Newer version | The same products request, compared with the installed version | — | Implemented |

Each installer is a MojoSetup shell script followed by a zip; the game's files are under
`data/noarch/`, each checked against its size and CRC-32. GOG offers one Linux installer per game
at a time.

## Offline installers, kept whole

The setup programs GOG's site offers, downloaded to keep rather than installed.

| Use | Request | Source | Status |
|---|---|---|---|
| Installers of a game and its DLC | The products request above, every `os` (`windows`, `linux`, `mac`); an installer comes in one file or several (`files[]`). A DLC the account does not own has no installers listed | gogapidocs, lgogdownloader | Not verified |
| Download link and checksums | The file's `downlink`, as above: `{"downlink": …, "checksum": …}`, `checksum` empty when GOG keeps none (null or missing are read the same way) | lgogdownloader, gogrepoc | Not verified |
| File name | The last segment of the address the link leads to, percent-decoded. A setup for several languages is listed under each of them: the same name and size, downloaded once | lgogdownloader (its duplicate handling) | Not verified |
| Checksums | `GET` the `checksum` address, without the token: XML, `<file md5="…">` holding `<chunk from="…" to="…" method="md5">…</chunk>`, `to` being the chunk's last byte. Not found there (404) is taken as GOG keeping none | lgogdownloader | Not verified |
| Installer bytes | Ranged requests to the link, as for Linux builds | Capture | Verified (for Linux installers) |

The sums are read with each file's link, before its bytes, as lgogdownloader does: their address
is signed for a while only. A file is downloaded under a name of its own, its size in it (a file
GOG replaced by another of the same name and another size never takes up the other's part), taken
up from where it stopped, checked against the whole file's MD5, and renamed once right. Chunks
whose MD5 differs are downloaded again, once; a file still wrong is deleted. A file already in the
folder at its size, which another program may have left, is checked against the same sums without
downloading anything, and refused, left as it is, when it does not match; the files a download
checked itself are noted in its job (GOG's MD5, inode, size, modification and change times) and
not read again when it is taken up. A file GOG keeps no checksums for (no address, or nothing found
there) is taken on its size and said not checked:
replaced by another of the same name and size while paused, it would be kept mixed. Sums that are
there but cannot be read stop the download before its bytes; Resume reads them again.

## Cloud saves

| Use | Request | Source | Status |
|---|---|---|---|
| Save locations | `GET https://remote-config.gog.com/components/galaxy_client/clients/{client_id}?component_version=2.0.45` | Heroic | Verified |
| List | `GET https://cloudstorage.gog.com/v1/{user_id}/{client_id}` (JSON: `name`, `hash`, `last_modified`) | heroic-gogdl | Verified |
| Download | `GET …/{location}/{path}` | heroic-gogdl | Verified (content arrives decompressed) |
| Upload | `PUT …/{location}/{path}`, gzip body, `Etag`, `X-Object-Meta-LocalLastModified` | heroic-gogdl | Verified |
| Delete | `DELETE …/{location}/{path}` | heroic-gogdl | Implemented |

Cloud requests use a game-scoped token.

## Achievements

| Use | Request | Source | Status |
|---|---|---|---|
| List | `GET https://gameplay.gog.com/clients/{client_id}/users/{user_id}/achievements` | Comet, Heroic | Verified |
| Unlock or clear | `POST …/achievements/{achievement_id}` with `{"date_unlocked": "<date>" or null}` | Comet, gog_achievements | Verified |
| Unlocks made in game | Comet answering the game's Galaxy SDK on 127.0.0.1:9977 | Comet | Verified (Blasphemous, isolated, 2026-10-12); support per game is listed by Comet |
| Unlocks seen during the game | Comet's `gameplay.db`, table `achievement` (`key`, `name`, `unlock_time` set on unlock, before the upload to GOG) | Comet 0.3.2 sources (`db/gameplay.rs`, `unlock_user_achievement`) | Verified (Blasphemous, 2026-10-12): told during the session, recorded on GOG as it ended |
| Galaxy service for the SDK | A `GalaxyCommunication` Windows service registered in the prefix, and `HKLM\SOFTWARE\WOW6432Node\GOG.com\GalaxyClient\paths` `client` | Comet, Heroic | Registered at Blasphemous's first launch; not yet seen with a game that needs it |

While it runs, Comet itself also talks to GOG for the account: `auth.gog.com`, `gameplay.gog.com`,
`notifications-pusher.gog.com` and `cfg.gog.com` (see
[SECURITY.md](../SECURITY.md#what-leaves-your-computer)).

## Play time

| Use | Request | Source | Status |
|---|---|---|---|
| Total play time | `GET https://gameplay.gog.com/games/{product_id}/users/{user_id}/sessions` with the account token; returns `{"time_sum": <minutes>}` only | Heroic | Verified (Hollow Knight: the total GOG Galaxy shows) |
| Report a session | `POST` to the same address with `{"session_date": <start, Unix seconds>, "time": <minutes>}` | Heroic | Verified (Undertale, 4 minutes, added to the total; a 13-second session is not sent) |
| Last played date | None found: the address above, `galaxy-library` releases and `gameplay.gog.com/users/{id}/games/stats` have no such field; profile statistics need the website's session | — | Not available |

## Open questions

- Whether `POST` is accepted on the token endpoint, which would keep tokens out of addresses.
- Whether cloud storage supports conditional uploads (`If-Match`), which would close the overwrite
  window described in [cloud-saves.md](cloud-saves.md).
- Whether the Galaxy user agent is required by cloud storage.
- The meaning of the cloud hash `aadd86936a80ee8a369579c3926f1b3c`.
- Whether a session with Comet shows the account online, or playing, to GOG friends.
