# Arcthis logo

The approved mark is A2: an open container outline with three straight content bars. The middle bar is selected, separated from a short left node, and extends through the opening. It expresses direct access to one entry while retaining the archive.

## Assets and colors

- `site/assets/logo.svg` is the standalone mark; `site/assets/favicon.svg` uses the same geometry.
- Both use a transparent background and `viewBox="0 0 64 64"`, with no fonts, raster images, or external dependencies.
- Light appearance: outline and unselected bars `#172D35`, selected bar `#087F8C`.
- Dark appearance: outline and unselected bars `#F4F8F8`, selected bar `#58D3CF`.
- Standalone assets follow `prefers-color-scheme`. Website header/footer marks use the same paths inline and the shared `--logo-ink` / `--logo-accent` theme tokens, so manual theme changes also update them.
- A monochrome rendition uses one foreground color for all three paths. Preserve the opening, proportions, and flat ends of the content bars.

## Website usage

All six Chinese/English pages use A2 in the header, footer, and favicon. Brand links retain their accessible names, and inline marks are decorative (`aria-hidden="true"`). The favicon URL carries the deployment asset version so returning browsers request the new mark. `site/deploy.sh` updates cache versions for CSS, JavaScript, logo, and favicon URLs.

When modifying the mark, synchronize the two SVG assets and all twelve inline instances. Compare their three path geometries and check light/dark appearances at small sizes before deployment.

## CLI ASCII mark (current checkout)

`src/cli.rs` owns the ASCII mark and wordmark used before root Clap help. The open outline, flat content bars, and separated selected row follow A2. All characters are ASCII; the mark fits in 59 columns and adds no ANSI escapes or runtime dependencies. It appears for bare invocation and explicit root help, not for subcommand help, version output, archive results, or MCP messages. Published v0.5.1 binaries do not yet contain this source-checkout addition.

```text
  .----------.
 /   ======   \                         _    _      _
|             '       __ _  _ __   ___ | |_ | |__  (_) ___
|   =  =========     / _` || '__| / __|| __|| '_ \ | |/ __|
|             .     | (_| || |   | (__ | |_ | | | || |\__ \
 \   ======   /      \__,_||_|    \___| \__||_| |_||_||___/
  '----------'
```
