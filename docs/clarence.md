# Website orders into DGS Clarence

Riders build a bike on mxbsecure.com/build and send it as a model swap order. When Clarence is
connected, each order is posted to Clarence, which opens a private Model Swap ticket for it.
As the ticket moves, Clarence posts its stages back so the rider's page (mxbsecure.com/build/mine)
says the same as the ticket. Control plane code: `control-plane/src/clarence.ts`, `builds.ts`.

## Connecting

Two secrets on the control plane (`wrangler secret put`), the same secret set in Clarence:

| Secret | Value |
| --- | --- |
| `CLARENCE_ORDERS_URL` | Clarence's https endpoint that takes orders, e.g. `https://<clarence host>/mxb/orders` |
| `CLARENCE_SECRET` | A long random string, shared both ways |

Without both, orders stay on the website only and `/v1/clarence/*` answers 503.

## Signing (both ways)

Header `X-MXB-Signature: t=<unix seconds>,v1=<hex>`, where `hex` is HMAC-SHA256 of
`<t>.<raw request body>` under `CLARENCE_SECRET`. Refuse a signature more than 5 minutes off.
The same scheme as Stripe webhooks.

## mxbsecure → Clarence

`POST CLARENCE_ORDERS_URL`, JSON, signed. Answer 2xx to take it; anything else (or no answer in
8 s) marks the order "not in Clarence" on the site's queue, where it can be resent.

```json
{
  "event": "order.created",
  "id": "4b0c…-uuid",
  "ref": "MXB-TMRPNS",
  "steamId": "7656119…",
  "steamName": "Frost",
  "discord": "frost_41",
  "guids": 2,
  "note": "Black graphics, #41",
  "stage": "quote",
  "priceCents": null,
  "approval": null,
  "approvalNote": null,
  "spec": { "format": "mxb-build/1", "title": "KTM SX-F by Eyezaya", "bike": { … }, "parts": [ … ], "stock": [ … ] },
  "page": "https://mxbsecure.com/build/mine",
  "sentAt": 1791763600000
}
```

- `order.created`: a new order (or a resend: the same `id` again, treat it as the same order).
  Open a private ticket: item type Model Swap, premade route, the spec's parts as the item, the
  rider's Discord name to add to the ticket (`discord` may be null; then ask in the ticket).
  `guids` is the number of GUIDs; the first is the rider's own Steam ID (`steamId`).
- `order.approval`: the rider answered an approval request on the website (`approval` is
  `approved` or `changes`, `approvalNote` says what). Post it in the ticket.

No deliverable or file location is ever in an event: the finished swap ships through mxbsecure.

## Clarence → mxbsecure

`POST https://api.mxbsecure.com/v1/clarence/builds/<id>`, JSON, signed, any of:

```json
{ "ticketUrl": "https://discord.com/channels/<guild>/<channel>" }
{ "stage": "pending_payment", "priceCents": 2500, "note": "PayPal to …" }
{ "stage": "paid" }
{ "stage": "approval" }
{ "approval": "approved" }
{ "approval": "changes", "approvalNote": "Seat a bit darker" }
{ "stage": "complete" }
{ "stage": "cancelled" }
```

Stages move one step at a time: quote → pending_payment (needs a price) → paid → approval →
complete, or cancelled from any open stage. Posting `approval` again asks the rider again.
`complete` needs the latest approval to be `approved`. A refused move answers 409 with the reason.
Send `ticketUrl` once the ticket exists, so the rider's page links to it.
