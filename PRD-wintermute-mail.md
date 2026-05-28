# PRD: wintermute-mail — voice-driven email read + compose

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** `visions/wintermute.md` (Fleet 2 — action layer)
**Builds on:** `PRD-wintermute-dialog.md`, `PRD-wintermute-brain.md`,
  `PRD-wintermute-bootstrap.md` (account credentials enter via the
  caregiver web UI)
build_target: rust-cli
build_priority: medium

---

## TL;DR

A daemon `wm-mail` that exposes a small mail surface over agorabus:
inbox listing, message reads, send, search. IMAP (`async-imap`) for
read; SMTP (`lettre`) for send. Credentials live in the freedesktop
keyring set up by `wm-bootstrap` (extended in this PRD with a
`/mail` form). Destructive actions (delete, mass mark) go through
`wm-dialog`'s verbal-confirmation protocol.

---

## 1. Why this exists

Vision §End-state #9: *"Mail / calendar / music through MPRIS,
IMAP/SMTP, CalDAV."* Mail is the highest-utility piece for the
voice-first user — most everyday business with caregivers, family,
doctors flows through email.

Concrete evidence from Phase 1:

- `~/wintermute/wintermute-bootstrap` is shipped — already runs a
  caregiver-facing HTTP form for one-time setup. Adding a `/mail`
  step is a small extension (a separate PRD's iter scope), not a
  re-architecture.
- `async-imap` (MPL-2.0, active) and `lettre` (MIT-OR-Apache, broad
  use) are the proven Rust IMAP+SMTP stack. Both async-friendly,
  rustls TLS, no native OpenSSL needed.
- `secret-service` crate over freedesktop SecretService gives a
  durable keyring for IMAP password / SMTP password / OAuth token
  (when supported).

---

## 2. What this builds

### 2.1 Binary: `wm-mail`

Long-running daemon. Connects to IMAP idle on the primary inbox;
publishes `wm.mail.new` envelopes on arrival (brain decides whether
to interrupt).

### 2.2 Tools (topic `wm.mail.cmd`)

| Tool | Args | Returns |
|---|---|---|
| `inbox` | `{limit?=10, unread_only?=true}` | `{messages:[{id, from, subject, date, snippet}]}` |
| `read` | `{id}` | `{from, to, subject, body_text, attachments}` |
| `send` | `{to, subject, body, in_reply_to?}` | `{ok, message_id}` — destructive: confirm |
| `search` | `{query, limit?=20}` | `{messages}` — IMAP SEARCH |
| `mark_read` | `{id}` | `{ok}` |
| `delete` | `{id}` | `{ok}` — destructive: confirm |
| `folders` | `{}` | `{folders}` |

### 2.3 Verbal-confirm protocol

`send` and `delete` emit `wm.brain.reply.destructive` (already
handled by `wm-dialog` Fleet 1). Confirmation text constructed by
brain ("you want me to send 'Yes I'll be there at 3' to John — say
'yes send'").

### 2.4 Credentials flow

`wm-bootstrap` (already shipped) gets a new `/mail` page that posts
to `wm-mail set-account` over agorabus. The daemon writes to
SecretService and reloads.

Fields: IMAP host/port/user/pass, SMTP host/port/user/pass, From
address, friendly name. No OAuth v1 — Gmail-app-passwords + iCloud
app-specific are the documented paths.

### 2.5 New-mail signal

IMAP IDLE on INBOX. On a new message, publish `wm.mail.new` with
`{id, from, subject}`. Brain's policy decides whether to interrupt
("you have a new email from your sister, want me to read it?") or
let it wait. Quiet-hours respected via Fleet 3 once that ships;
v1 default: never interrupt, only on explicit "any new mail?".

---

## 3. Risks

- **Provider quirks** — Gmail requires app passwords or OAuth;
  iCloud requires app-specific passwords; some EU providers have
  rate-limits. Document Gmail-app-passwords + iCloud setup in the
  bootstrap `/mail` page.
- **Attachment handling** — v1: list names + types only; brain says
  "this message has an attached PDF, I can't read attachments yet".
  Future Fleet 2 extension can chain through `wm-screen-narrate` for
  inline images.
- **Big inboxes** — IMAP SEARCH over 10y of mail is slow on some
  servers. Default `inbox` limit 10; `search` server-side with
  IMAP SEARCH keywords (not body grep).
- **HTML body** — convert to text via `html2text` crate; preserve
  links as bracketed `[text](url)` for brain to optionally read.

---

## 4. Sequencing

Independent of `wm-browser` / `wm-desktop` / `wm-screen-narrate`.
Depends on `wm-bootstrap` shipped (it is, per archive). Composes
with `wm-calendar` (some invitations land as mail; future cross-PRD
bullet).

---

## 5. Acceptance criteria

1. `wm-bootstrap` `/mail` page accepts an account, posts to
   `wm-mail set-account`, daemon writes to SecretService and reports
   `{ok:true, host:<host>, user:<user-masked>}`.
2. `wm-mail inbox` against a Gmail app-password account returns
   the latest 10 messages with `from`, `subject`, `date`, `snippet`
   populated; HTML stripped from snippet.
3. `wm-mail read {id}` returns the full body in `body_text`,
   HTML converted, attachments listed by name+MIME but not content.
4. `wm-mail send` issues a destructive confirmation through
   `wm-dialog`; on "yes send", the message lands in the recipient's
   inbox and the From-account's Sent folder.
5. `wm-mail search {query:"from:sister"}` returns at least 1 hit
   in a primed test account.
6. `wm-mail mark_read {id}` flips IMAP `\Seen`; subsequent
   `inbox {unread_only:true}` excludes it.
7. `wm-mail delete` requires verbal confirmation; on "yes delete",
   moves to Trash (not expunge) for safety.
8. IMAP IDLE: a new arriving message produces a `wm.mail.new`
   publish within 30 s of server delivery (verified by manual send
   from a phone).
9. Credentials never logged or sent over agorabus in plaintext —
   only the masked-user form is published; ctrace summary confirms
   no password substrings in network or stdout traffic.
10. **[live]** Real round-trip: jsy says "any new mail from my
    sister?", brain calls `search`, dialog reads the latest. If
    she says "reply yes I'll be there", brain calls `send` with
    confirm flow. End-to-end <20 s.
