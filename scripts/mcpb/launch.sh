#!/bin/sh
# Starts the stdio bridge that ships inside the Metocast app (`metocast-server mcp-stdio`).
# The app keeps this path pointing at its own helper every time it opens, so the extension
# carries no second binary and cannot drift from the app. The URL and token arrive in the
# environment (METOCAST_MCP_URL, METOCAST_MCP_TOKEN), never as arguments.
helper="$HOME/Library/Application Support/Metocast/mcp/metocast-mcp"
if [ ! -x "$helper" ]; then
  echo "Metocast has not been opened on this Mac yet. Open the Metocast app once, then restart Claude." >&2
  exit 1
fi
exec "$helper" mcp-stdio
