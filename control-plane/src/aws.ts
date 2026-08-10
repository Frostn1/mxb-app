/**
 * Talking to EC2 from the Worker.
 *
 * The app never holds AWS credentials — a desktop binary can be unpacked, and a key inside
 * one would let anyone create infrastructure in our account. So provisioning lives here,
 * behind the same bearer auth as everything else, and the AWS key exists only as a Worker
 * secret.
 *
 * The key is scoped by IAM to launching and managing instances tagged `mxb:managed=true` in
 * one region, and can do nothing else. That tag is therefore load-bearing: an instance
 * launched without it is one this credential can never stop or terminate again, which on an
 * hourly-billed resource means it runs until someone notices. Every launch here sets it.
 */

import { AwsClient } from "aws4fetch";

/** Where servers are created. Fixed rather than per-request: the IAM key is scoped to it. */
export const REGION = "us-west-2";

/** EC2's query API. The JSON protocol isn't available for EC2, so this is form-encoded. */
const EC2_ENDPOINT = `https://ec2.${REGION}.amazonaws.com/`;
const EC2_API_VERSION = "2016-11-15";

/** Marks an instance as ours. The IAM policy keys every write on it — see the module note. */
export const MANAGED_TAG = { key: "mxb:managed", value: "true" };

export interface AwsEnv {
  AWS_ACCESS_KEY_ID: string;
  AWS_SECRET_ACCESS_KEY: string;
}

/**
 * The AWS credentials, or null on a deployment that has none.
 *
 * Provisioning is optional: paint sync, the registry and the roster are the whole product
 * for anyone not running servers, and a fork or a preview deployment should not fail to
 * boot because it has no cloud account. Callers turn null into a 503 rather than a crash.
 */
export function awsEnv(env: {
  AWS_ACCESS_KEY_ID?: string;
  AWS_SECRET_ACCESS_KEY?: string;
}): AwsEnv | null {
  const id = env.AWS_ACCESS_KEY_ID?.trim();
  const secret = env.AWS_SECRET_ACCESS_KEY?.trim();
  if (!id || !secret) return null;
  return { AWS_ACCESS_KEY_ID: id, AWS_SECRET_ACCESS_KEY: secret };
}

function client(env: AwsEnv): AwsClient {
  return new AwsClient({
    accessKeyId: env.AWS_ACCESS_KEY_ID,
    secretAccessKey: env.AWS_SECRET_ACCESS_KEY,
    service: "ec2",
    region: REGION,
  });
}

/**
 * Call an EC2 action and hand back the parsed response.
 *
 * EC2 answers in XML, which is why this asks for JSON-ish handling nowhere: there is no
 * JSON option on this API. Rather than pull in an XML parser for the handful of fields we
 * need, callers extract with [`pluck`] — the shapes involved are small and flat.
 */
export async function ec2(
  env: AwsEnv,
  action: string,
  params: Record<string, string> = {},
): Promise<string> {
  const body = new URLSearchParams({ Action: action, Version: EC2_API_VERSION, ...params });
  const resp = await client(env).fetch(EC2_ENDPOINT, {
    method: "POST",
    body: body.toString(),
    headers: { "content-type": "application/x-www-form-urlencoded" },
  });
  const text = await resp.text();
  if (!resp.ok) {
    // EC2 puts the useful part in <Message>; the rest is envelope.
    const message = pluck(text, "Message")[0] ?? text.slice(0, 300);
    throw new Error(`EC2 ${action} failed (${resp.status}): ${message}`);
  }
  return text;
}

/**
 * Every value of a repeated flat XML tag, in document order.
 *
 * Deliberately not a parser. The responses read here are a handful of scalars inside a
 * known shape, and a real XML dependency would be more surface than the problem deserves.
 * Nested tags of the same name would defeat it — none of the fields used here nest.
 */
export function pluck(xml: string, tag: string): string[] {
  const out: string[] = [];
  const re = new RegExp(`<${tag}>([^<]*)</${tag}>`, "g");
  let m: RegExpExecArray | null;
  while ((m = re.exec(xml)) !== null) out.push(m[1]);
  return out;
}

/**
 * The newest Windows Server 2022 image Amazon publishes.
 *
 * Resolved at launch rather than pinned in config: a pinned AMI silently rots, and the day
 * it is deregistered every launch fails with an error that says nothing about why. Windows
 * rather than Linux because MX Bikes ships only `mxbikes.exe` — a Linux host would mean
 * Wine for both the installer and the server, which nobody has shown to work for this game.
 */
export async function latestWindowsAmi(env: AwsEnv): Promise<string> {
  const xml = await ec2(env, "DescribeImages", {
    "Owner.1": "amazon",
    "Filter.1.Name": "name",
    "Filter.1.Value.1": "Windows_Server-2022-English-Full-Base-*",
    "Filter.2.Name": "state",
    "Filter.2.Value.1": "available",
  });
  const ids = pluck(xml, "imageId");
  const names = pluck(xml, "name");
  if (ids.length === 0) throw new Error("no Windows Server image available");
  // The name carries the publish date (`...-Base-2026.08.13`), so the lexically greatest
  // name is the newest build. Sorting by that beats DescribeImages' arbitrary ordering.
  let best = 0;
  for (let i = 1; i < ids.length; i++) {
    if ((names[i] ?? "") > (names[best] ?? "")) best = i;
  }
  return ids[best];
}

/**
 * Base64 for EC2 user data.
 *
 * Not `btoa`, which throws on any character above U+00FF — it takes a "binary string", one
 * char per byte, so anything outside Latin-1 is not representable. The script carries prose
 * comments and, more to the point, the operator's own server name: a name with an emoji or a
 * CJK character would fail the launch with `InvalidCharacterError` and nothing else to go on.
 *
 * Encoding to UTF-8 first and base64-ing the bytes is what `btoa` was always standing in for.
 */
function base64Utf8(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary);
}

export interface LaunchSpec {
  /** Display name, used for the Name tag so the console is readable. */
  name: string;
  instanceType: string;
  securityGroupId: string;
  /** PowerShell the instance runs on first boot. */
  userData: string;
}

/**
 * Launch one server.
 *
 * Two things here are not optional. The instance is tagged `mxb:managed=true` **as part of
 * the launch**, because the IAM key can only stop or terminate instances carrying that tag
 * — an instance that came up untagged could never be turned off by this credential again,
 * and it bills by the hour until a human notices. And it is launched with
 * `InstanceInitiatedShutdownBehavior=terminate` so that a server shutting itself down
 * stops costing money entirely rather than lingering as a stopped instance with an EBS
 * volume still on the bill.
 */
export async function runInstance(
  env: AwsEnv,
  spec: LaunchSpec,
  amiId: string,
): Promise<string> {
  const xml = await ec2(env, "RunInstances", {
    ImageId: amiId,
    MinCount: "1",
    MaxCount: "1",
    InstanceType: spec.instanceType,
    "SecurityGroupId.1": spec.securityGroupId,
    InstanceInitiatedShutdownBehavior: "terminate",
    // Base64 is what EC2 expects here; the instance decodes it before running it.
    UserData: base64Utf8(spec.userData),
    "TagSpecification.1.ResourceType": "instance",
    "TagSpecification.1.Tag.1.Key": MANAGED_TAG.key,
    "TagSpecification.1.Tag.1.Value": MANAGED_TAG.value,
    "TagSpecification.1.Tag.2.Key": "Name",
    "TagSpecification.1.Tag.2.Value": spec.name.slice(0, 128),
  });
  const id = pluck(xml, "instanceId")[0];
  if (!id) throw new Error("EC2 accepted the launch but returned no instance id");
  return id;
}

/** Stop billing for compute, keeping the disk. Only works on instances we tagged. */
export async function stopInstance(env: AwsEnv, instanceId: string): Promise<void> {
  await ec2(env, "StopInstances", { "InstanceId.1": instanceId });
}

export async function startInstance(env: AwsEnv, instanceId: string): Promise<void> {
  await ec2(env, "StartInstances", { "InstanceId.1": instanceId });
}

/** Destroy it, disk included. The only action that stops every charge for a server. */
export async function terminateInstance(env: AwsEnv, instanceId: string): Promise<void> {
  await ec2(env, "TerminateInstances", { "InstanceId.1": instanceId });
}

export interface FleetInstance {
  instanceId: string;
  /** `pending` | `running` | `stopping` | `stopped` | `shutting-down` | `terminated` */
  state: string;
  publicIp: string | null;
  instanceType: string;
  launchedAt: string | null;
}

/**
 * Every instance this credential manages, terminated ones excluded.
 *
 * This is the billing truth. The database records what we *think* we created; this is what
 * AWS will actually charge for, and the two drift the moment a launch half-fails. Anything
 * that enforces a cap has to count from here, not from our own table.
 */
export async function fleet(env: AwsEnv): Promise<FleetInstance[]> {
  const xml = await ec2(env, "DescribeInstances", {
    "Filter.1.Name": `tag:${MANAGED_TAG.key}`,
    "Filter.1.Value.1": MANAGED_TAG.value,
    // Terminated instances linger in the API for about an hour and cost nothing; counting
    // them would refuse launches against capacity that no longer exists.
    "Filter.2.Name": "instance-state-name",
    "Filter.2.Value.1": "pending",
    "Filter.2.Value.2": "running",
    "Filter.2.Value.3": "stopping",
    "Filter.2.Value.4": "stopped",
  });

  // One <item> per instance inside <instancesSet>. Splitting on the id keeps each
  // instance's fields together without a parser: every block starts at its own id.
  const ids = pluck(xml, "instanceId");
  const states = pluck(xml, "name");
  const ips = pluck(xml, "ipAddress");
  const types = pluck(xml, "instanceType");
  const launched = pluck(xml, "launchTime");

  return ids.map((instanceId, i) => ({
    instanceId,
    // <name> also appears under the state-reason block, but the instance state comes first
    // for each instance, so index alignment holds for the fields we read.
    state: states[i] ?? "unknown",
    publicIp: ips[i] ?? null,
    instanceType: types[i] ?? "unknown",
    launchedAt: launched[i] ?? null,
  }));
}
