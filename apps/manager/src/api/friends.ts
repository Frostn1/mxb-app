import { invoke } from "@tauri-apps/api/core";

/** Where an accepted friend is riding, as their own app last reported it. */
export interface FriendPresence {
  serverName: string;
  /** `host:port` when their app launched the game; null when only the name is known. */
  address: string | null;
  track: string | null;
  riders: number | null;
  updatedAt: number;
}

export interface Friend {
  accountId: string;
  riderName: string;
  /** Null when they are offline, hiding their presence, or not on a server. */
  presence: FriendPresence | null;
}

export interface Person {
  accountId: string;
  riderName: string;
}

export interface FriendsState {
  /** `ABCD-EFGH`. */
  friendCode: string;
  /** The control plane's own "hide my presence". */
  hidePresence: boolean;
  friends: Friend[];
  incoming: Person[];
  outgoing: Person[];
}

export type Relation = "none" | "friends" | "pending_out" | "pending_in";

export interface SearchResult extends Person {
  relation: Relation;
}

export const friendsList = () => invoke<FriendsState>("friends_list");

export const friendsSearch = (query: string) =>
  invoke<{ results: SearchResult[] }>("friends_search", { query }).then((r) => r.results);

/** Ask a rider from a search result. */
export const friendsRequestAccount = (accountId: string) =>
  invoke<{ status: "pending" | "friends"; riderName: string }>("friends_request", { accountId });

/** Ask whoever owns a friend code. */
export const friendsRequestCode = (friendCode: string) =>
  invoke<{ status: "pending" | "friends"; riderName: string }>("friends_request", { friendCode });

export const friendsRespond = (accountId: string, accept: boolean) =>
  invoke<{ status: string }>("friends_respond", { accountId, accept });

/** Unfriend, or cancel a request you sent. */
export const friendsRemove = (accountId: string) => invoke<unknown>("friends_remove", { accountId });

export const friendsHidePresence = (hide: boolean) =>
  invoke<{ hidePresence: boolean }>("friends_hide_presence", { hide });

/** The local switch for this app reporting where the rider is. */
export const setFriendsPresence = (enabled: boolean) =>
  invoke<void>("set_friends_presence", { enabled });
