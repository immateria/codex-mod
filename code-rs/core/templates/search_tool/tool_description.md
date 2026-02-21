# MCP tool discovery (BM25)

Searches over enabled MCP tool metadata with BM25.

When `search_tool_bm25` is available, MCP tools are hidden until you search for them with this tool.
After you search, only the selected MCP tools are available for the remainder of the current session/thread.

Follow this workflow:

1. Call `search_tool_bm25` with:
   - `query` (required): focused terms that describe the capability you need.
   - `limit` (optional): maximum number of tools to return (default `8`).
2. Use the returned `tools` list to decide which MCP tools are relevant.
3. Matching tools are added to `active_selected_tools` and remain available for the remainder of the current session/thread.
4. Repeated searches in the same session/thread are additive: new matches are unioned into `active_selected_tools`.

Notes:
- Core tools remain available without searching.
- If you are unsure, start with `limit` between 5 and 10 to see a broader set of tools.
- `query` is matched against MCP tool metadata fields:
  - `name`
  - `tool_name`
  - `server_name`
  - `title`
  - `description`
  - input schema property keys (`input_keys`)
- Selecting a tool does not bypass MCP access prompting. If a tool is blocked by policy, it will still require user approval.
