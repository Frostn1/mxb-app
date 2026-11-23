import { invoke } from "@tauri-apps/api/core";

export interface PublishPreview {
  id: string;
  name: string;
  author: string;
  version: string;
  summary: string;
  categories: string[];
  permissions: string[];
  network: string[];
  size: number;
  thumbnail: string | null;
}

export interface PublishCheck {
  ok: true;
  outcome: "listed" | "review";
  tier: "verified" | "community";
  reviewReason: null | "verified-update" | "new-permissions";
  addedPermissions: string[];
  signedWith: "author-key" | "package";
  preview: PublishPreview;
}

export interface Review {
  id: string;
  kind: "verify" | "update" | "permissions";
  version: string;
  state: string;
  note: string;
}

export interface MyListing extends PublishPreview {
  tier: "verified" | "community";
  verified: boolean;
  review: Review | null;
}

/** Checks a local package and returns the exact publish outcome without storing it. */
export const checkPublish = (path: string) => invoke<PublishCheck>("ext_publish_check", { path });

export const backupAuthorKey = (dest: string) => invoke<void>("ext_author_key_backup", { dest });
export const restoreAuthorKey = (src: string) => invoke<void>("ext_author_key_restore", { src });

/** Publishes the package that the user accepted in the preview. */
export const publishExtension = (path: string) => invoke<{
  id: string;
  status: "listed" | "review";
  tier: string;
  review: Review | null;
}>("ext_publish", { path });

/** Loads the signed-in publisher's listings and recent review results. */
export const myListings = () => invoke<{ extensions: MyListing[] }>("ext_my_listings");

/** Requests verification for the currently served community version. */
export const requestVerified = (id: string) => invoke<{ review: Review }>("ext_request_verified", { id });
