# 0014. GOG never told of other stores' games

- Status: Accepted
- Date: 2026-10-12

## Context

The library mixes GOG's games with Steam's, Epic's, games added by hand and emulated ones. Some of
GOG's services take a game id (play time, achievements, gamesdb's descriptions), and GOG's database
knows many games that are not GOG's.

## Decision

**GOG learns nothing of what a person owns elsewhere.** Requests to GOG about a game take only
GOG's own product ids, checked by `paths::check_game_id`; a Steam, Epic, added or emulated game is
never sent to GOG, not even to find its cover or description.

## Alternatives considered

- **Using gamesdb for every game's metadata**: richer pages for other stores' games, at the cost of
  telling GOG a person's whole library.

## Consequences

- Other stores' games take their images from their own store or from SteamGridDB, once turned on.
- Pairing a GOG game with its Steam twin uses the Steam ids gamesdb gives for the GOG game, never
  the other way round.
