/**
 * The paint-sync room for one server: who to tell when someone arrives or leaves.
 *
 * Stores nothing. Who is on the server and what they wear is in D1, where `/v1/paintsync/join`
 * reads it; this only holds the sockets, hibernating, so a quiet server costs nothing, and
 * relays what the Worker hands it.
 */

import { DurableObject } from "cloudflare:workers";

interface Member {
  accountId: string;
  serverKey: string;
}

/** Enough for any grid the game can run, and a ceiling on what one room fans out to. */
const MAX_MEMBERS = 64;

export class PaintRoom extends DurableObject<Env> {
  async fetch(request: Request): Promise<Response> {
    // Both headers are set by the Worker after it has checked the token and the presence row;
    // nothing reaches this object any other way.
    const accountId = request.headers.get("X-Account-Id");
    const serverKey = request.headers.get("X-Server-Key");
    if (!accountId || !serverKey) return new Response("unauthorized", { status: 401 });

    if (new URL(request.url).pathname === "/notify") {
      const frame = await request.text();
      for (const ws of this.ctx.getWebSockets()) {
        const m = ws.deserializeAttachment() as Member | null;
        if (m?.accountId === accountId) continue;
        try {
          ws.send(frame);
        } catch {
          // A socket closing as we speak; its own close handler tidies up.
        }
      }
      return new Response(null, { status: 204 });
    }

    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("expected a websocket", { status: 426 });
    }
    // One socket per account: a second is the app reconnecting, and the old one is dead.
    for (const ws of this.ctx.getWebSockets()) {
      if ((ws.deserializeAttachment() as Member | null)?.accountId === accountId) {
        ws.close(1000, "replaced by a newer connection");
      }
    }
    if (this.ctx.getWebSockets().length >= MAX_MEMBERS) {
      return new Response("this room is full", { status: 503 });
    }

    const [client, server] = Object.values(new WebSocketPair());
    this.ctx.acceptWebSocket(server!);
    server!.serializeAttachment({ accountId, serverKey } satisfies Member);
    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws: WebSocket, raw: string | ArrayBuffer): Promise<void> {
    const m = ws.deserializeAttachment() as Member | null;
    if (!m) return void ws.close(1011, "no identity");
    let t: unknown;
    try {
      t = typeof raw === "string" ? (JSON.parse(raw) as { t?: unknown }).t : null;
    } catch {
      t = null;
    }
    if (t !== "ping") return;
    // The heartbeat: a rider with a live socket stays on the server without re-posting a join.
    // Only refreshes a row that already names this server, so a rider who moved is not pulled back.
    await this.env.DB.prepare("UPDATE presence SET updated_at = ? WHERE account_id = ? AND server_id = ?")
      .bind(Date.now(), m.accountId, m.serverKey)
      .run();
    ws.send(JSON.stringify({ t: "pong" }));
  }

  async webSocketClose(ws: WebSocket, code: number): Promise<void> {
    // A dropped socket is not a departure: the app reconnects after a blip, and presence
    // expires on its own if it never does. `leave` is what tells the others.
    try {
      ws.close(code, "closing");
    } catch {
      // Already closed.
    }
  }
}
