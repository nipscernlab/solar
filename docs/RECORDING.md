# The recording format

**Status:** normative, referenced from [`CONTRACT.md`](CONTRACT.md). **Format version:**
`1.0.0`. **Written on:** 27 September 2026.

`solar serve --stdio --record <file>` writes a session down as it happens, and
`solar replay <file>` sends the requests again and says where the answers differ. This
document is what the file holds, so that something other than SOLAR can write one and have
it replayed: ZENITH exporting a session is the case this was written for.

## The shape

**NDJSON**: one JSON object per line, UTF-8, `\n` between lines. No trailing comma, no
array around the whole thing, nothing that a text editor cannot trim.

**The first line is a header.** Every line after it is an entry, in the order the lines
crossed.

```json
{"solar_recording":"1.0.0","at":"2026-09-27T21:05:49.374016Z","solar_version":"0.3.0","protocol":"solar/1"}
{"at":"2026-09-27T21:05:49.374181Z","direction":"in","line":"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"solar.ping\"}"}
{"at":"2026-09-27T21:05:49.374431Z","direction":"out","line":"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{...}}"}
```

### The header

| Member | Type | What it is |
| ------ | ---- | ---------- |
| `solar_recording` | string | The version of **this format**, semantic. It is also how a reader tells a header from an entry: an object with this member is the header. |
| `at` | string | When the recording started: RFC 3339, UTC, microseconds, ending in `Z`. |
| `solar_version` | string | The SOLAR that wrote it, or the version of whatever wrote it in SOLAR's place. |
| `protocol` | string | The protocol the lines below speak, `solar/1` today. |

### An entry

| Member | Type | What it is |
| ------ | ---- | ---------- |
| `at` | string | When the line crossed: RFC 3339, UTC, microseconds, ending in `Z`. |
| `direction` | `"in"` or `"out"` | `in` is a request, from the caller to SOLAR. `out` is a response. |
| `line` | string | The line exactly as it crossed, **without its newline**. A request line is a JSON-RPC request; a response line is a response, or an array of them for a batch. |

`line` is a **string**, not an embedded object. The point of a recording is to hold what
really crossed, byte for byte, including a line that was not valid JSON at all: a recording
of a parse error is exactly the kind worth keeping.

## Versions

`solar_recording` is read the way every other version in this repository is read: **a
reader refuses a file whose major differs** from the one it was written against, and reads
one whose minor moved by ignoring what it does not know.

| Version | What it holds |
| ------- | -------------- |
| `1.0.0` | A header and entries, as above. SOLAR 0.3.0 and later write this. |

A file **with no header** was written before the format was versioned, by SOLAR 0.2.0 or
earlier. `solar replay` reads it as `1.0.0`, which is what it looks like, and says so on
standard error. A writer that is not SOLAR should always write the header.

`solar replay` refuses a file that declares a major it does not know, and says which
version the file declares, which one this build reads, and what to do about it. It does not
guess: replaying a format it does not understand would report differences that belong to
the format rather than to SOLAR.

## The schema of one line

A line is either a header or an entry. This is the schema, JSON Schema 2020-12, and a test
in `crates/solar-apis/tests/recording.rs` compiles it and validates a real recording
against it, so it cannot drift from what SOLAR writes.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://nipscernlab.github.io/solar/recording-1.0.0.json",
  "title": "One line of a SOLAR recording",
  "oneOf": [{ "$ref": "#/$defs/header" }, { "$ref": "#/$defs/entry" }],
  "$defs": {
    "timestamp": {
      "type": "string",
      "description": "RFC 3339, UTC, microseconds.",
      "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\\.[0-9]{6}Z$"
    },
    "header": {
      "type": "object",
      "description": "The first line of a recording, which says what the file is.",
      "properties": {
        "solar_recording": {
          "type": "string",
          "description": "The version of this format, semantic.",
          "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+$"
        },
        "at": { "$ref": "#/$defs/timestamp" },
        "solar_version": {
          "type": "string",
          "description": "What wrote the recording."
        },
        "protocol": {
          "type": "string",
          "description": "The protocol the lines below speak."
        }
      },
      "required": ["solar_recording", "at", "solar_version", "protocol"],
      "additionalProperties": false
    },
    "entry": {
      "type": "object",
      "description": "One line that crossed, in the order it crossed.",
      "properties": {
        "at": { "$ref": "#/$defs/timestamp" },
        "direction": {
          "enum": ["in", "out"],
          "description": "in is a request, out is a response."
        },
        "line": {
          "type": "string",
          "description": "The line exactly as it crossed, without its newline."
        }
      },
      "required": ["at", "direction", "line"],
      "additionalProperties": false
    }
  }
}
```

## Writing one from something that is not SOLAR

Three rules, and a file that follows them replays:

1. **Write the header first**, with `solar_recording` set to a version this SOLAR reads,
   `1.0.0` today. `solar_version` is what wrote the file, so name yourself.
2. **Put the line in `line` as a string**, exactly as it crossed, without the newline.
   Do not re-serialise it: a recording is evidence, and re-serialising loses the member
   order, the spacing and anything that was not valid JSON.
3. **Keep the order.** Entries are replayed in the order they appear, and a request is
   matched to its answer by `id` rather than by position, so a session that cancelled
   replays sensibly.

What `solar replay` then does with the file is in [`CONTRACT.md`](CONTRACT.md) and in
`solar replay --help`: it sends every `in` line again, matches each answer to the recorded
one by `id`, and reports where they differ, ignoring the members of `meta` that differ
between any two runs and every timestamp anywhere in the response.
