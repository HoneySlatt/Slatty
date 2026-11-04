# 0007. GOG's Linux builds from its offline installers, read file by file

- Status: Accepted
- Date: 2026-10-12

## Context

Slatty installs Windows builds from GOG's Galaxy content system (generation 2), with verified chunks
and binary patches. That system has no Linux builds: asked for one, it answers "Unsupported OS".
GOG's native Linux builds exist only as offline installers, MojoSetup scripts with a zip archive
appended, often several gigabytes.

## Decision

**Linux builds come from the offline installers** (`installer::linux`), **read file by file with
ranged requests**: Slatty reads the archive's directory, then each file's bytes, and writes the
game folder as it goes, through the same staged and verified pipeline as the content system. Their
build id is `linux:<version>`.

## Alternatives considered

- **Downloading the installer whole, then unpacking it**: twice the disk space for a while, and no
  resume per file.
- **Running GOG's installer**: a shell script and a graphical setup outside Slatty's control,
  writing where it chooses.
- **Windows builds through Proton only**: native Linux builds exist and usually run better.

## Consequences

- GOG's CDN serves byte ranges but not suffix ranges, and the API's installer sizes are rounded:
  the exact size comes from `Content-Range` (AGENTS.md, "Known pitfalls").
- Updates of a Linux build are new installer versions, not patches.
- On Windows hosts these builds are not offered ([0022](0022-windows-hosts.md)).
