# MapleOS Templates

## Workflow Templates

Browse and use pre-built workflow templates. Copy a YAML file to your
workflow editor or import via API:

```bash
curl -X POST http://localhost:7788/api/v3/workflows \
  -H "Content-Type: application/json" \
  -d @templates/workflows/basic-chat.yaml
```

### Available Templates

- `basic-chat.yaml` — Simple LLM chat workflow
- `kb-search-then-answer.yaml` — Search KB for context, then answer

## Skill Templates

Skill manifest files for the Skills marketplace (#23). Each JSON file
describes a skill's parameters schema, version, and author.

### Available Skills

- `web-search.json` — Web search with configurable result count
- `code-execute.json` — Sandboxed code execution (JS/Python)
