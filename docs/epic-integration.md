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

So far Slatty signs in and reads the account's library. Installing, playing and cloud saves are
planned (see [roadmap.md](roadmap.md)).

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
