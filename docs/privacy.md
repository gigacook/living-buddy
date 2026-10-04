# Privacy and data flows

Tendly has **no analytics, telemetry or tracking**. The public code does not phone home.

## Where data lives

Everything (people's display names, groups, tasks, routines, calendars, history, notifications, suggestions) is stored in one SQLite database on the machine running Tendly (`TENDLY_DATA_DIR`). The native app keeps it in the OS's per-app data folder. Browsers keep only UI preferences and which display name you picked.

## When data leaves that machine

Only when you set something up that requires it:

| You… | What is sent where |
| --- | --- |
| subscribe to a calendar link | Tendly downloads that URL (no data is uploaded) |
| create a share link | Anyone with the link can read what its scope covers |
| connect a mailbox or Slack | Tendly calls that provider's API with your authorization and reads previews |
| enable an AI provider for a connector or pasted text | A shortened, redacted excerpt of each message goes to that provider |
| use system notifications | Notification text is handed to your OS or browser |

## The Claude usage card

It stores only numbers and notes you type in. Tendly never signs in to, scrapes or reads your Claude account, and there is no official quota API it could use. The card is separate from any AI usage by Tendly itself.

## Retention and deletion

- Connector message previews: deleted after the connector's retention period (default 14 days).
- Pasted text: only the extracted suggestion is kept, and decided suggestions are cleaned up after 30 days.
- Deleting a calendar source deletes its events. Deleting a connector deletes its stored previews and pending suggestions.
- Tasks are soft-deleted and their history is kept for the household record. Export or delete the whole database at any time ([backup.md](backup.md)).

## Hosted versions

This repository is the self-hostable app. If a hosted service is ever offered, it must publish its own privacy notice describing any processing beyond what is listed here.
