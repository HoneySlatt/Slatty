# Epic Games integration

Epic publishes no API for third-party launchers. Slatty signs in with the Epic Games Launcher's
public OAuth client and uses the services the launcher uses, as
[legendary](https://github.com/derrod/legendary) (GPL-3.0) understood them: its
`legendary/api/egs.py` and `legendary/core.py` are the source of every request below. This page
lists them and whether each was checked against Epic.

**Status:**
- **Verified:** used successfully against real Epic services (see [compatibility.md](compatibility.md)).
- **Implemented:** coded and tested against simulated services only.

Requests to Epic send the Epic Games Launcher's `User-Agent`, as legendary does, so that Epic's
services answer Slatty as they answer the launcher whose client it signs in with.

Slatty signs in, reads the account's library, installs a game's Windows build and plays it.
Updates and cloud saves are planned (see [roadmap.md](roadmap.md)).

## Authentication

| Use | Request | Source | Status |
|---|---|---|---|
| Sign-in page (system browser) | `GET https://www.epicgames.com/id/login?redirectUrl=https://www.epicgames.com/id/api/redirect?clientId=34a02cf8f4414e29b15921876da36f9a&responseType=code`, which ends on a JSON page holding `authorizationCode` | legendary (`EPCAPI.get_auth_url`) | Verified |
| Code exchange | `POST https://account-public-service-prod03.ol.epicgames.com/account/api/oauth/token`, HTTP basic authentication with the client id and secret, form `grant_type=authorization_code&code=…&token_type=eg1` | legendary (`EPCAPI.start_session`) | Verified |
| Session refresh | The same, with `grant_type=refresh_token&refresh_token=…&token_type=eg1`; the answer carries a new refresh token, which replaces the old one in the keyring | legendary (`EPCAPI.start_session`) | Implemented |

The answer names the account (`account_id`, 32 hexadecimal digits; `displayName`) and how long the
access token lasts (`expires_in`). The client id and secret ship with the Epic Games Launcher;
legendary and Heroic use the same. A code or a refresh token is sent once: the first answer uses
it up, even one lost on the way.

## Library

| Use | Request | Source | Status |
|---|---|---|---|
| Owned items | `GET https://library-service.live.use1a.on.epicgames.com/library/api/public/items?includeMetadata=true`, with the token, paged with `cursor` (`responseMetadata.nextCursor`) | legendary (`EPCAPI.get_library_items`) | Verified |
| Title, images, add-on or not | `GET https://catalog-public-service-prod06.ol.epicgames.com/catalog/api/shared/namespace/{namespace}/bulk/items?id={catalogItemId}&includeDLCDetails=true&includeMainGameDetails=true&country=US&locale=en`, with the token: an object keyed by the item's id | legendary (`EPCAPI.get_game_info`) | Verified |
| Covers | The `keyImages` of the catalog's answer, downloaded without a token: `DieselGameBoxTall` (else `OfferImageTall`, `Thumbnail`) for the cover, `DieselGameBox` (else `OfferImageWide`) for the background, `DieselGameBoxLogo` for the logo | legendary's metadata | Verified (covers) |

As legendary does (`LegendaryCore.get_non_asset_library_items`), the library leaves out Unreal
Engine's marketplace (namespace `ue`), private sandboxes (`sandboxType` `PRIVATE`, games in
development), records without an `appName`, the broken app name `1`, add-ons (a catalog item with
`mainGameItem`) and mods (category `mods`). The catalog is asked only about items the cache neither
describes nor left out, eight at a time. A game's id is `epic-<appName>`; an app name that is not
letters, digits, `_` or `-` (64 at most) is left out, since it names files.

## Windows builds

| Use | Request | Source | Status |
|---|---|---|---|
| The newest build | `GET https://launcher-public-service-prod06.ol.epicgames.com/launcher/api/public/assets/v2/platform/Windows/namespace/{namespace}/catalogItem/{catalogItemId}/app/{appName}/label/Live`, with the token: `elements[0]` gives `buildVersion`, the manifest's SHA-1 (`hash`), its addresses (`manifests[].uri` with `queryParams`) and, for an encrypted build, its keys (`secrets`) | legendary (`EPCAPI.get_game_manifest`, `LegendaryCore.get_cdn_urls`) | Verified |
| Install folder | The catalog's `customAttributes.FolderName`, else the app name | legendary (`LegendaryCore.prepare_download`) | Verified |
| Manifest | `GET` the first of its addresses that answers, without the token, over HTTPS (the launcher uses plain HTTP); checked against the SHA-1 above | legendary (`LegendaryCore.get_cdn_manifest`) | Verified |
| Chunks | `GET {base}/{path}`, `{base}` being the manifest address's folder, without the token: `ChunksV4/{group:02}/{hash:016X}_{guid}.chunk` (by feature level: `Chunks`, `V2`, `V3`, `V4`), and from feature level 22 `ChunksV5/{key id or plain}/{group:02}/{hash}_{guid}.chunk` in URL-safe Base64 | legendary (`ChunkInfo.path`, `DLManager`) | Verified for one game, the layout of its manifest not recorded |

**The manifest** is binary (`legendary/models/manifest.py`): a header (magic `0x44BEC00C`, sizes,
SHA-1 of the body, whether it is compressed or encrypted, version), then a zlib body of four
sections, each led by its size: metadata (app name, build version, launch program and arguments,
prerequisites), the chunks (by columns: guids, hashes, SHA-1, groups, unpacked and packed sizes,
then from feature level 22 their key ids), the files (by columns: names, link targets, SHA-1,
flags, install tags, chunk parts as guid, offset and size), and custom fields. From feature level
24 an encrypted manifest keeps its launch command, prerequisites and file names in an AES-GCM
block, opened with the key `secrets` gives for its key id. Every count is checked against what is
left to read, every chunk is at most 64 MiB, and every part lies inside its chunk. The older JSON
form of the manifest is not read: such a build is refused.

**A chunk** (`legendary/models/chunk.py`) is a header (magic `0xB1FE3AA2`, its guid, whether it is
compressed or encrypted, its SHA-1, from header version 4 its key id and tag), then its data, zlib
compressed, and AES-GCM encrypted with the nonce its SHA-1 begins with when it is. Slatty checks
that it is the chunk asked for and of the size the manifest gives; its content is checked with the
files made of it.

**Files** are made of parts of chunks, a chunk often holding the end of one file and the start of
the next. Slatty writes them in the order legendary does (by path, regardless of case), downloads
each chunk once, eight at a time, and keeps a chunk until its last use, at most 512 MiB of them:
past that, the chunk used again the latest is dropped and downloaded again. Each file is written
beside its place, checked against its SHA-1, then renamed; the game folder appears once every file
is. Symbolic links the manifest lists are not made; paths differing only by case are one file, the
last listed. Install tags (optional parts such as language packs) are not chosen: every file is
installed. Encrypted chunks and manifests were checked against data Slatty encrypted itself, not
against Epic's.

## Launch

| Use | Request | Source | Status |
|---|---|---|---|
| Sign-in code for the game | `GET https://account-public-service-prod03.ol.epicgames.com/account/api/oauth/exchange`, with the token: `code`, good once and for five minutes | legendary (`EPCAPI.get_game_token`) | Implemented |
| Ownership token, for a game whose catalog says `OwnershipToken: true` | `POST https://ecommerceintegration-public-service-ecomprod02.ol.epicgames.com/ecommerceintegration/api/public/platforms/EPIC/identities/{account_id}/ownershipToken`, with the token, form `nsCatalogItemId={namespace}:{catalogItemId}`: the token's bytes | legendary (`EPCAPI.get_ownership_token`) | Implemented |
| Deployment of Epic's online services | The build service's `elements[0].sidecar.config`, a JSON object as a string: `deploymentId` | legendary (`LegendaryCore.get_game_and_dlc_list`, `Sidecar`) | Implemented |

The game is started as legendary starts it (`LegendaryCore.get_launch_parameters`): the program
the manifest names (`launch_exe`), from its own folder, with the manifest's arguments
(`launch_command`) and the catalog's (`AdditionalCommandline`), then:

```
-AUTH_LOGIN=unused -AUTH_PASSWORD=<code> -AUTH_TYPE=exchangecode -epicapp=<app name> -epicenv=Prod
[-epicovt=<ownership token file>] -EpicPortal -epicusername=<display name> -epicuserid=<account id>
-epiclocale=<language> -epicsandboxid=<namespace> [-epicdeploymentid=<deployment>]
```

The language is the system's (`fr` for `fr_FR.UTF-8`), else `en`. The ownership token is written in
the prefix's `C:` (`-epicovt=C:\slatty-epic.ovt`), where an isolated game sees it too, and deleted
once the game ends; legendary writes it in its own temporary folder, as a Linux path. Without a code
(Epic out of reach, the session rejected) the game starts without any of these arguments, as
legendary's offline launch does, which a game that can run offline accepts.

umu's database lists Epic games under the store `egs`, by app name: Proton's fixes are found for
them as for GOG's (checked in UMU-Proton 10.0-4's copy of the database: 108 Epic games).
