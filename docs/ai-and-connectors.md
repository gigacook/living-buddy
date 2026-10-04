# AI extraction and connectors

Tendly can turn life-admin messages into **suggestions** (tasks, appointments, deadlines, follow-ups) that you review in the Inbox. Nothing is added, sent, paid, deleted or changed until a person confirms a suggestion, and confirming creates exactly one task or event.

## Extractors

| Extractor | Data leaves the server? | Cost |
| --- | --- | --- |
| **Local rules** (default) | No | Free |
| **Anthropic (Claude)**, bring your own key | Yes: a shortened, redacted excerpt per message | Billed to your Anthropic account |
| **OpenAI-compatible endpoint**, e.g. a local model server | Only to the URL you configure | Depends on that endpoint |

Configure on the host under Settings → Administration → AI extraction, or with `TENDLY_AI_API_KEY[_FILE]`. Keys are stored encrypted server-side, never shown again and never sent to browsers. The default Anthropic model is `claude-opus-5-5`; you can choose another model ID. AI is used for a connector only if that connector has "Allow AI extraction" enabled, and for pasted text only if an administrator allows it.

## Data minimization and prompt-injection handling

Before any extractor sees a message, Tendly:

- uses only the subject and a short preview (Gmail's snippet, Outlook's body preview, a Slack message's text), never attachments and never whole mailboxes;
- removes quoted replies and signatures, strips URL query strings, redacts tokens and email addresses, and truncates to a configurable length (default 2 000 characters).

Message content is treated as data, not instructions:

- It is wrapped in `<untrusted_message>` tags (with angle brackets in the content neutralized), and the instructions tell the model to ignore any requests in it.
- The model gets **no tools**, and its output must match a strict JSON schema. Tendly validates it again: unknown fields are dropped, only four inert suggestion kinds exist, malformed dates or times become "please check" instead of being guessed, and at most five suggestions per message are kept.
- Messages that look like they try to instruct an assistant (for example "ignore previous instructions", "send the password", "mark as paid") are flagged in the Inbox.

## Connectors

Connectors are self-host modules managed on the host (Settings → Administration). Each belongs to the person who created it; its suggestions are visible only to that person.

| Provider | Status | How it connects | What it reads |
| --- | --- | --- | --- |
| Sample mailbox | Implemented | Synthetic JSON files in `integrations/fixtures/mail` | Everything in the fixtures |
| Gmail | Implemented, mock-verified | Gmail API + OAuth (PKCE) with **your own** Google Cloud OAuth client; scope `gmail.readonly` | Subject + snippet of new messages (history-ID cursor) |
| Outlook / Microsoft 365 | Implemented, mock-verified | Microsoft Graph delta query + OAuth (PKCE) with **your own** Entra ID app; scopes `Mail.Read offline_access` | Subject + body preview of new inbox messages |
| Slack | Partial, mock-verified | A bot token you create (`channels:history`); OAuth install flow not built | New messages in listed channels |
| Proton Mail | Scaffolded (not working) | Proton has no Gmail-style API. The supported route is Proton Mail Bridge (paid plan) exposing IMAP on your machine; the IMAP reader is not implemented yet | – |

"Mock-verified" means the adapter was tested against local mock servers that imitate the provider's API, not against real accounts. Google may require app verification before an OAuth app can be used beyond test users. Tendly never uses browser sessions, cookies or scraping.

Behavior common to all connectors:

- Encrypted credentials; access tokens refresh automatically; a revoked or expired grant moves the connector to "needs auth" and stops polling. "Disconnect" deletes credentials and cursors (and asks Google to revoke the grant).
- Incremental cursors, de-duplication by message ID (hashed), idempotent ingestion.
- Durable jobs with retries, exponential backoff with jitter, respect for `Retry-After`, a retry limit, and slower polling after repeated failures.
- Health states: never run, healthy, degraded, rate limited, needs auth, error, off.
- Retention: stored previews are deleted after the connector's retention period (default 14 days); decided suggestions are deleted after it as well.
- Logs contain no message contents or tokens.

## Adding a provider

Implement a `sync` branch in `crates/server/src/connectors.rs` that returns new `FetchedItem`s and a cursor, map provider errors to `ConnectorError` (`NeedsAuth`, `RateLimited`, `Transient`, `Permanent`), add it to `providers()`, and test it against a mock server like the Gmail and Slack tests in `crates/server/tests/connectors.rs`.
