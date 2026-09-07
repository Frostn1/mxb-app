/**
 * The Durable Object shell for one server's room.
 *
 * Deliberately thin: every rule worth arguing about — what a frame may contain, who appears
 * in a peer list, when a peer is signalling too fast — lives in `voice.ts` as a plain
 * function with a test. What is left here is the part only the runtime can provide, which
 * is sockets, hibernation, and the identity that rides along with them.
 */

import { DurableObject } from "cloudflare:workers";

import {
  MAX_PEERS,
  overSignalBudget,
  parseClientMessage,
  peerViews,
  type PeerIdentity,
} from "./voice";

/** The voice room for one server. */
export class VoiceRoom extends DurableObject<Env> {
  async fetch(request: Request): Promise<Response> {
    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("expected a websocket", { status: 426 });
    }
    // Set by the Worker after it has checked the token and the presence row. A client can't
    // reach this object except through that path, so these are trusted here.
    const accountId = request.headers.get("X-Account-Id");
    if (!accountId) return new Response("unauthorized", { status: 401 });

    if (this.ctx.getWebSockets().length >= MAX_PEERS) {
      return new Response("this room is full", { status: 503 });
    }

    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);

    // Hibernation, not a held connection: a room with ten riders sitting quietly between
    // corners should cost nothing. The identity rides on the socket so it survives being
    // evicted from memory and woken again.
    this.ctx.acceptWebSocket(server);
    const identity: PeerIdentity = {
      peerId: crypto.randomUUID(),
      accountId,
      riderName: "",
      raceNum: 0,
      ready: false,
      signalCount: 0,
      signalSince: Date.now(),
    };
    server.serializeAttachment(identity);

    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws: WebSocket, raw: string | ArrayBuffer): Promise<void> {
    const identity = ws.deserializeAttachment() as PeerIdentity | null;
    if (!identity) return void ws.close(1011, "no identity");

    const msg = parseClientMessage(typeof raw === "string" ? raw : null);
    if (typeof msg === "string") {
      send(ws, { t: "error", error: msg });
      return;
    }

    switch (msg.t) {
      case "hello": {
        // One socket per account. A second connection is the app restarting or reconnecting
        // after a network blip, and the stale one would otherwise sit in the roster
        // advertising a peer nobody can reach.
        for (const other of this.ctx.getWebSockets()) {
          if (other === ws) continue;
          const them = other.deserializeAttachment() as PeerIdentity | null;
          if (them?.accountId === identity.accountId) {
            this.dropPeer(other, them);
            other.close(1000, "replaced by a newer connection");
          }
        }

        identity.riderName = msg.riderName;
        identity.raceNum = msg.raceNum;
        identity.ready = true;
        ws.serializeAttachment(identity);

        const peers = peerViews(this.identities(), identity.peerId);
        send(ws, { t: "welcome", peerId: identity.peerId, peers });
        // The newcomer offers to everyone already here; they only need to know to expect it.
        // That rule is what keeps two peers from offering to each other at once.
        this.broadcast(
          { t: "joined", peer: { peerId: identity.peerId, riderName: identity.riderName, raceNum: identity.raceNum } },
          ws,
        );
        return;
      }

      case "rider": {
        identity.riderName = msg.riderName;
        identity.raceNum = msg.raceNum;
        ws.serializeAttachment(identity);
        if (!identity.ready) return;
        this.broadcast(
          { t: "rider", peerId: identity.peerId, riderName: identity.riderName, raceNum: identity.raceNum },
          ws,
        );
        return;
      }

      case "signal": {
        if (!identity.ready) return void send(ws, { t: "error", error: "say hello first" });
        const over = overSignalBudget(identity, Date.now());
        ws.serializeAttachment(identity);
        if (over) return void send(ws, { t: "error", error: "too many signals" });

        const target = this.ctx
          .getWebSockets()
          .find((s) => (s.deserializeAttachment() as PeerIdentity | null)?.peerId === msg.to);
        // A peer that left mid-negotiation is ordinary, not an error worth a frame: the
        // `left` broadcast is already on its way to this sender.
        if (!target) return;
        // `from` is ours, never the sender's — otherwise a peer could impersonate another
        // by relabelling its own offers.
        send(target, { t: "signal", from: identity.peerId, kind: msg.kind, data: msg.data });
        return;
      }

      case "bye":
        ws.close(1000, "bye");
        return;
    }
  }

  async webSocketClose(ws: WebSocket): Promise<void> {
    this.dropPeer(ws, ws.deserializeAttachment() as PeerIdentity | null);
  }

  async webSocketError(ws: WebSocket): Promise<void> {
    this.dropPeer(ws, ws.deserializeAttachment() as PeerIdentity | null);
  }

  private dropPeer(ws: WebSocket, identity: PeerIdentity | null): void {
    if (!identity?.ready) return;
    this.broadcast({ t: "left", peerId: identity.peerId }, ws);
  }

  private identities(): PeerIdentity[] {
    return this.ctx
      .getWebSockets()
      .map((ws) => ws.deserializeAttachment() as PeerIdentity | null)
      .filter((p): p is PeerIdentity => p !== null);
  }

  private broadcast(msg: unknown, except: WebSocket): void {
    for (const ws of this.ctx.getWebSockets()) {
      if (ws === except) continue;
      send(ws, msg);
    }
  }
}

/** Best-effort send. A socket that has gone away is the close handler's business, not ours. */
function send(ws: WebSocket, msg: unknown): void {
  try {
    ws.send(JSON.stringify(msg));
  } catch {
    // Closed between the roster read and the write.
  }
}
