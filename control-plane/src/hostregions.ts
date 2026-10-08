/**
 * The regions a user may deploy into, and the OVH VPS each one is ordered as.
 *
 * Every value here was read from OVH's public order catalog for the US subsidiary
 * (`GET https://api.us.ovhcloud.com/v1/order/catalog/public/vps?ovhSubsidiary=US`, 2026-10-07):
 * one OVH US account can order all five. The EU catalog has no US datacenter.
 *
 * `vps_datacenter` and the add-on plan codes differ per plan family: `vps-2027-model1` (US),
 * `-eu` and `-ca` (the latter carries SYD). The `os`, `storage` and `automatedBackup` add-on
 * families are mandatory in that catalog, so all three are ordered with every box. The backup is the
 * cheapest one (`option-auto-backup-2027-1-model1*`, 1 backup).
 */

export type RegionId = "us-east" | "us-west" | "eu-west" | "eu-east" | "oceania";
export type ServerType = "mxbserver" | "legacy";
export type Pool = "native" | "legacy";

export interface HostRegion {
  id: RegionId;
  label: string;
  /** OVH's `vps_datacenter` configuration value. */
  datacenter: string;
  /** Where it is, for the operator view. */
  location: string;
  planCode: string;
  osAddon: string;
  /** The mandatory `storage` family add-on (local NVMe). */
  storageAddon: string;
  backupAddon: string;
}

export const REGIONS: readonly HostRegion[] = [
  {
    id: "us-east",
    label: "US East",
    datacenter: "US-EAST-VA",
    location: "Vint Hill, Virginia",
    planCode: "vps-2027-model1",
    osAddon: "option-linux",
    storageAddon: "option-storage-local-2027-model1",
    backupAddon: "option-auto-backup-2027-1-model1",
  },
  {
    id: "us-west",
    label: "US West",
    datacenter: "US-WEST-OR",
    location: "Hillsboro, Oregon",
    planCode: "vps-2027-model1",
    osAddon: "option-linux",
    storageAddon: "option-storage-local-2027-model1",
    backupAddon: "option-auto-backup-2027-1-model1",
  },
  {
    id: "eu-west",
    label: "EU West",
    datacenter: "GRA",
    location: "Gravelines, France",
    planCode: "vps-2027-model1-eu",
    osAddon: "option-linux-eu",
    storageAddon: "option-storage-local-2027-model1-eu",
    backupAddon: "option-auto-backup-2027-1-model1-eu",
  },
  {
    // Warsaw is the only OVH VPS datacenter east of Germany.
    id: "eu-east",
    label: "EU East",
    datacenter: "WAW",
    location: "Warsaw, Poland",
    planCode: "vps-2027-model1-eu",
    osAddon: "option-linux-eu",
    storageAddon: "option-storage-local-2027-model1-eu",
    backupAddon: "option-auto-backup-2027-1-model1-eu",
  },
  {
    id: "oceania",
    label: "Oceania",
    datacenter: "SYD",
    location: "Sydney, Australia",
    planCode: "vps-2027-model1-ca",
    osAddon: "option-linux-ca",
    storageAddon: "option-storage-local-2027-model1-ca",
    backupAddon: "option-auto-backup-2027-1-model1-ca",
  },
];

export const SERVER_TYPES: readonly { id: ServerType; label: string }[] = [
  { id: "mxbserver", label: "mxbserver" },
  { id: "legacy", label: "Legacy" },
];

/** The OS every box is installed with, as OVH's `vps_os` value. */
export const BOX_OS = "Ubuntu 24.04";

/** The sudo user OVH creates on a `BOX_OS` image: the one the installer and an operator log in as. */
export const BOX_USER = "ubuntu";

export function regionById(id: unknown): HostRegion | null {
  return REGIONS.find((r) => r.id === id) ?? null;
}

export function poolFor(type: ServerType): Pool {
  return type === "legacy" ? "legacy" : "native";
}

/** Bike sets a native slot may race, as installed on every box (`/etc/mxbserver/bike-sets`). */
export const BIKE_SETS: readonly { id: string; name: string }[] = [
  { id: "oem-mx2", name: "OEM MX2" },
  { id: "oem-mx1", name: "OEM MX1" },
];

/** The rider cap a slot can be set to. The capacity figures are for 20-rider lobbies. */
export const MAX_RIDERS = 30;

/** Slot `i` (1-based) on a box. Fixed: `box-install.sh` lays the box out the same way. */
export function slotPorts(index: number): { gamePort: number; adminPort: number } {
  return { gamePort: 54209 + index, adminPort: 9808 + 2 * index };
}
