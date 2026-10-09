# GOG integration

GOG publishes no API for third-party launchers. SlattyLauncher identifies itself as the GOG Galaxy
client and uses the services Galaxy uses. This page lists every service involved, where the knowledge
comes from, and whether it was verified against GOG.

**Status:**
- **Verified:** used successfully against real GOG services (see [compatibility.md](compatibility.md)).
- **Implemented:** coded and tested with simulations only.

## Authentication

| Use | Request | Source | Status |
|---|---|---|---|
| Sign-in page (system browser) | `GET https://auth.gog.com/auth?client_id=46899977096215655&redirect_uri=https://embed.gog.com/on_login_success?origin=client&response_type=code&layout=galaxy` | gogapidocs, Heroic | Verified |
| Code exchange | `GET https://auth.gog.com/token?grant_type=authorization_code&…` | gogapidocs, heroic-gogdl | Verified |
| Session refresh | `GET https://auth.gog.com/token?grant_type=refresh_token&…` | gogapidocs, heroic-gogdl | Verified: the refresh token is not rotated, the previous one stays valid |
| Game-scoped token | Same endpoint with the game's Galaxy `client_id`/`client_secret` and `without_new_session=1` | Comet, heroic-gogdl | Verified |
| User profile | `GET https://users.gog.com/users/{user_id}` | Heroic | Verified |

The Galaxy client id and secret are public; they ship with every Galaxy install and every launcher
that talks to GOG.

GOG accepts only the Galaxy redirect URI. No local redirect, custom OAuth client or PKCE flow is
known to work, which is why the final address is pasted back by the user.

## Library

| Use | Request | Source | Status |
|---|---|---|---|
| Owned products | `GET https://galaxy-library.gog.com/users/{user_id}/releases` (paged with `page_token`) | Heroic | Verified |
| Metadata and artwork | `GET https://gamesdb.gog.com/platforms/gog/external_releases/{id}` | Heroic | Verified |
| Fallback title | `GET https://api.gog.com/products/{id}` | gogapidocs | Implemented |
| Owned products, used for DLC | `GET https://embed.gog.com/user/data/games` (`owned`: product ids) | heroic-gogdl | Verified |

## Installation (Galaxy content system, generation 2)

| Use | Request | Source | Status |
|---|---|---|---|
| Builds | `GET https://content-system.gog.com/products/{id}/os/windows/builds?generation=2` | gogapidocs, heroic-gogdl | Verified |
| Build metadata (zlib JSON: depots, languages, install folder, client id and secret, dependencies) | The build's `link` | gogapidocs, heroic-gogdl | Verified |
| Depot manifests (zlib JSON: files, chunks with compressed and decompressed MD5) | `GET https://gog-cdn-fastly.gog.com/content-system/v2/meta/{ab}/{cd}/{hash}` | heroic-gogdl | Verified |
| Download links | `GET https://content-system.gog.com/products/{id}/secure_link?_version=2&generation=2&path=/` | heroic-gogdl | Verified |
| Chunks | The link's `url_format` with `path` extended by `/{ab}/{cd}/{compressedMd5}` | heroic-gogdl | Verified |

Each product (the game and every DLC) has its own download links; DLC chunks are fetched through the
DLC's links. Download links expire. On 401 or 403 they are requested again.

Chunks hold up to 10 MiB of file data (seen on Undertale: 10 485 760 bytes, the last one slightly
larger). The decompressed MD5 identifies a chunk's content, which is what allows reusing chunks
already on disk.

### Binary patches

| Use | Request | Source | Status |
|---|---|---|---|
| Find a patch | `GET https://content-system.gog.com/products/{id}/patches?_version=4&from_build_id=…&to_build_id=…` with the account token; returns `{"link": …}`, or 404 | heroic-gogdl | Verified (Hollow Knight 1.5.12618 → 1.5.12620: 354 files, 86 KB of deltas; none for Undertale 1.06 → 1.08 or non-consecutive builds) |
| Patch description | The link (zlib JSON): `algorithm` (`xdelta3`), `baseProductId`, `depots` with `productId`, `languages`, `manifest` | heroic-gogdl | Verified |
| File diffs | `GET https://gog-cdn-fastly.gog.com/content-system/v2/patches/meta/{ab}/{cd}/{manifest}`: `DepotDiff` items with `path_source`, `path_target`, `md5_source`, `md5_target` and delta chunks | heroic-gogdl | Verified |
| Delta chunks | Download links from `secure_link` with `&root=/patches/store` | heroic-gogdl | Verified |

Deltas are VCDIFF as produced by xdelta3 without secondary compression (heroic-gogdl builds xdelta3
with `SECONDARY_DJW=0` and `SECONDARY_LZMA=0`). The match is on the whole-file MD5 of the source;
the rebuilt file must have `md5_target`.

### Dependencies and post-install setup

| Use | Request | Source | Status |
|---|---|---|---|
| Dependency repository | `GET https://content-system.gog.com/dependencies/repository?generation=2`, then its `repository_manifest` (zlib JSON: dependency id, executable path and arguments, manifest) | heroic-gogdl | Verified |
| Dependency manifests | `GET https://gog-cdn-fastly.gog.com/content-system/v2/dependencies/meta/{ab}/{cd}/{hash}` | heroic-gogdl | Verified |
| Dependency downloads | `GET https://content-system.gog.com/open_link?generation=2&_version=2&path=/dependencies/store/`, chunks under the returned `url` | heroic-gogdl | Verified (script interpreter) |
| Setup data | Build metadata `scriptInterpreter` and per-product `temp_executable` | Heroic | Verified (`scriptInterpreter` on Undertale) |

Dependencies whose executable lives under `__redist/` are shared and installed at first launch;
the others ship files into the game folder and are installed with it, as heroic-gogdl does. The
script interpreter and setup programs receive the same arguments as in Heroic (`/VERYSILENT`,
`/DIR=`, `/ProductId=`, `/supportDir=`, …).

## Cloud saves

| Use | Request | Source | Status |
|---|---|---|---|
| Save locations | `GET https://remote-config.gog.com/components/galaxy_client/clients/{client_id}?component_version=2.0.45` | Heroic | Verified |
| List | `GET https://cloudstorage.gog.com/v1/{user_id}/{client_id}` (JSON: `name`, `hash`, `last_modified`) | heroic-gogdl | Verified |
| Download | `GET …/{location}/{path}` | heroic-gogdl | Verified (content arrives decompressed) |
| Upload | `PUT …/{location}/{path}`, gzip body, `Etag`, `X-Object-Meta-LocalLastModified` | heroic-gogdl | Verified |
| Delete | `DELETE …/{location}/{path}` | heroic-gogdl | Implemented |

Cloud requests use a game-scoped token and the Galaxy user agent string used by heroic-gogdl.

## Achievements

| Use | Request | Source | Status |
|---|---|---|---|
| List | `GET https://gameplay.gog.com/clients/{client_id}/users/{user_id}/achievements` | Comet, Heroic | Verified |
| Unlock or clear | `POST …/achievements/{achievement_id}` with `{"date_unlocked": "<date>" or null}` | Comet, gog_achievements | Verified |
| Unlocks made in game | Comet answering the game's Galaxy SDK on 127.0.0.1:9977 | Comet | Implemented; per-game support listed by Comet |

### Play time

| Use | Request | Source | Status |
|---|---|---|---|
| Total play time | `GET https://gameplay.gog.com/games/{product_id}/users/{user_id}/sessions` with the account token; returns `{"time_sum": <minutes>}` only | Heroic | Verified (Hollow Knight: 3175 min, as shown by Galaxy) |
| Report a session | `POST` to the same URL with `{"session_date": <start, Unix seconds>, "time": <minutes>}` | Heroic | Verified (Undertale, 4 min, added to the GOG total) |
| Last played date | None found: the endpoint above, `galaxy-library` releases and `gameplay.gog.com/users/{id}/games/stats` have no such field; profile stats need the website session | — | Not available |
| Galaxy service for the SDK | `GalaxyCommunication` Windows service registered in the prefix, plus `HKLM\SOFTWARE\WOW6432Node\GOG.com\GalaxyClient\paths` `client` | Comet, Heroic | Registration verified under Proton; not yet with a game that needs it |

## Open questions

- Whether `POST` is accepted on the token endpoint, which would keep tokens out of URLs.
- Whether cloud storage supports conditional uploads (`If-Match`), which would close the overwrite
  window described in [cloud-saves.md](cloud-saves.md).
- Whether the Galaxy user agent is required by cloud storage.
- The meaning of the cloud hash `aadd86936a80ee8a369579c3926f1b3c`.
- How GOG installer scripts ("support" files) should be applied under Wine.
