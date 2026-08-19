# n8n — client onboarding WhatsApp sequence

`client-onboarding-whatsapp.json` is an importable n8n workflow that sends the
one-time onboarding sequence when a client joins the program.

**Status: plumbing complete, copy not written.** The message bodies are
placeholders — the real text lives in the external WhatsApp service that
currently receives monday's webhooks, which was not reachable from the machine
that built this. Fill the placeholders before activating.

## What it does

```
monday webhook  →  handshake?  →  echo challenge
                      ↓ no
                   ack 200  →  fetch client from monday
                                     ↓
                            normalize phone + guard check
                                     ↓
                          already sent? ──yes──→ stop
                                     ↓ no
                    1. welcome  →  2. forms  →  3. video link
                                     ↓
                          mark "sent" back in monday
```

The guard is the point: monday fires onboarding from **two** places today
(`144343273` on the קליטת לקוח button, and `180599353` on any new item created
on board `5053409738`). Either can arrive first, and both can arrive. Writing
"sent" back to monday makes the second one a no-op, which is what makes the
sequence genuinely once-only.

## Before importing

1. **Create the guard column** on board `5053409738` (לקוחות): a Status column,
   e.g. `הודעת אונבורדינג נשלחה`, with a single label `נשלח`. Note its column id.
2. Replace `ONBOARDING_SENT_COLUMN_ID` throughout with that id (3 places:
   the fetch query, the Code node's `GUARD_COLUMN_ID`, and the final mutation).
3. Replace `FORM_URL_MEDICAL` and `FORM_URL_INTAKE` with the real form links.
4. Replace every `PLACEHOLDER` message body with the real copy.

## Credentials and env

| What | How |
|---|---|
| monday API | n8n **Header Auth** credential, header `Authorization`, value = monday API token. Attach to both HTTP Request nodes that call `api.monday.com`. |
| Green API | env vars `GREENAPI_ID_INSTANCE` and `GREENAPI_API_TOKEN` on the n8n instance. |

Green API is an assumption, not a confirmed fact — it was the best guess at the
current provider. Only the three send nodes touch it; swapping providers means
changing their URL and body shape and nothing else.

## Wiring monday to it

After importing and activating, take the production webhook URL and point the
monday integration at it. Note that monday sends a one-time `challenge` payload
when the webhook is registered and expects it echoed back — the `Is Handshake?`
/ `Echo Challenge` branch handles that, so registration will succeed.

Once this workflow owns the sequence, **deactivate the old integration** so the
external service stops sending its own copy. Running both means clients get the
onboarding twice.

## Phone normalization

The Code node converts monday's phone field to Green API's `<msisdn>@c.us`:
`05X-XXXXXXX` → `9725XXXXXXXX@c.us`, passing through numbers already in `972`
form. Anything that doesn't land on `972` + 9 digits is treated as invalid and
skips the send rather than failing loudly mid-sequence — check the `Skipped`
branch in execution history if a client reports never receiving the messages.
