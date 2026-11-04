# Ebbi two-device sync roadmap

Prepared on 2026-11-03. This document tracks the staged work. The app is installed and paired on both devices; manual task transfer in both directions and conflict refusal have been verified by the owner.

## Agreed scope

- Two devices, Piero's laptop and desktop PC, sharing one Ebbi workspace.
- Manual Push and Pull while connected to the internet. Both devices do not need to be on at the same time.
- Pull before editing on a different device. Push before switching devices.
- Keep local SQLite storage and the existing Rust/Iced application. Local use remains available without AWS.
- Transfer a complete, consistent database snapshot, including tasks, habits, numeric entries, goals, study notes, reviews and history. Analytics are calculated locally from that data.
- Detect conflicting histories and block unsafe transfers. Automatic merging is outside this first version.
- A private S3 general purpose bucket using S3 Standard, with versioning enabled.
- A separate IAM user and access key for each device. No interactive AWS login during normal Ebbi use.
- Piero performs all AWS account, bucket, IAM and credential setup. Codex implements the application and prepares instructions.
- Device configuration and credentials stay outside the synchronized database.

The accepted workflow assumes the previous reply's "yes" means conflicts may be blocked rather than automatically merged. A device with unpublished changes must never be silently overwritten by Pull.

## 1. Prepare the AWS learning guide

Owner: Codex. Deliverable: [AWS_SETUP.md](AWS_SETUP.md).

Status: complete. The guide was reviewed against AWS documentation and its example syntax was checked locally. AWS setup and transfers were completed subsequently in step 2.

Explain buckets, objects, regions, policies, users, access keys and named profiles. Include console steps, a scoped policy, per-device configuration, a synthetic transfer exercise, expected results, troubleshooting and credential revocation instructions.

Record the storage tradeoff explicitly. The initial credential setup uses a named AWS profile in local files with private permissions. Those files contain plaintext credentials, not encrypted keyring storage. No credentials go into this repository or Ebbi's database. A desktop keyring can be considered separately if encrypted storage is wanted.

Completion: instructions exist in this directory and use verified AWS documentation. The guide does not imply that cloud resources have already been created.

## 2. Create and exercise AWS resources

Owner: Piero, following the guide.

1. Choose a region and create the private, versioned bucket.
2. Create the bucket-scoped policy and two IAM users.
3. Configure each device with its own key under the `ebbi-sync` profile.
4. Upload a harmless file from the laptop and download it on the PC.
5. Check the identity on each device and verify the content matches.

Status: complete on 2026-11-04. Piero reported successful identity checks and synthetic transfers in both directions using separate device credentials. Keys remain on their respective devices.

## 3. Implement the local sync foundation

Owner: Codex. Status: local snapshot and revision-state primitives implemented on 2026-11-04. The step 4 engine and internal worker requests use them; application controls are still pending.

- Add an optional sync module with separate transport, protocol and persistence responsibilities.
- Persist the last accepted cloud revision and local change state across app restarts. The current daily-backup dirty flag is insufficient for this.
- Account for every database mutation, including imports, local restore, deletions and schema changes. Sync bookkeeping must not itself make user data appear edited.
- Keep device identity, AWS profile, bucket and region in device-local configuration. Never include access keys in snapshots, error messages or logs.
- Reuse SQLite's backup API and existing validation/recovery code. Never upload a direct copy of a live WAL database.
- Keep SQLite and network work off the UI thread. Serialize snapshot/application operations with writes, or track later local edits so they remain unpublished.
- Select a compatible AWS client dependency when implementing step 4, after explaining its purpose and build/runtime impact. Do not require AWS CLI as the app's runtime transport merely because it is used for the learning exercise.

Verification: a temporary synthetic example exercised snapshots, persisted state across restart, a task edit and restore. An offline build and format check passed. Rust tests remain prohibited and were not run. Imports, schema changes, failure injection and application integration remain unverified. Existing offline behavior does not depend on sync.

## 4. Implement safe S3 Push and Pull

Owner: Codex. Status: complete for implementation and synthetic exercises. The paired-workspace engine is wired to internal database-worker requests, but has no UI controls yet. Initial workspace publication and joining are part of step 5; real-device AWS validation is step 6.

The official AWS SDK for Rust adds the S3 and profile-loading dependencies plus Tokio. The lockfile selects versions compatible with the declared Rust 1.89 minimum, but only Rust 1.99.1 has been used for a build. The app does not invoke the AWS CLI. Requests check the expected bucket owner and have bounded retries, request timeouts and object sizes. A pending local record is written before cloud head publication or database replacement. Uncertain cloud publication is checked or retried with the same revision and conditional ETag; an unresolvable changed head pauses sync.

A synthetic in-memory transport exercised a lost publication response, corrupt download, interrupted pull before replacement, successful push/pull, stale cloud and unpublished local edits. A local HTTP S3-like server with fake credentials exercised the actual SDK's conditional object creation, conditional head replacement, stale ETag rejection, download checksum failure and access denial. These checks did not contact AWS. A modeled restart after replacement recovered its pending pull state. Two concurrent conditional publications to the local mock produced one success and one rejection. An additional synthetic exercise ran two database workers and the SDK against a local HTTP mock, preserving tasks, binary and numeric habits, study notes and completed reviews, while refusing conflicting changes. An in-memory exercise checked competing publication, a successor and unknown ancestry. Real S3 behavior through the app and real process or hardware failure remain to be exercised. Rust tests were not created or run.

Proposed object layout within the dedicated bucket:

```text
ebbi/head.json
ebbi/snapshots/<unique-revision>.db
ebbi/setup-checks/...
```

`head.json` identifies the current workspace, protocol/schema version, snapshot key, revision, parent revision, content checksum and originating device. It contains no credentials. Device clocks are descriptive only and do not decide which history wins.

Push creates a new snapshot object, then conditionally updates `head.json` using the previously observed ETag. The initial head is created only if absent. Publishing the head is the commit point; uploading a snapshot alone is not successful sync. This prevents two devices from silently overwriting the same revision. S3 supports these preconditions through `If-Match` and `If-None-Match`. [AWS conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html).

Pull downloads the referenced snapshot to a private temporary file, verifies checksum, schema and workspace identity, preserves a verified local recovery copy, then applies it safely. Local revision metadata must agree with the applied database after interruption and restart.

Required decisions in the app:

- Local and cloud revisions match, with local changes: allow Push.
- Cloud has advanced, with no local changes: allow Pull.
- Both local and cloud have advanced: block replacement and preserve both histories.
- Neither has changed: report that the workspace is already up to date.
- Network failure, access denied, unsupported schema or invalid download: report the error without changing the working data.
- Upload timeout with an uncertain result: inspect the published revision before retrying or reporting failure. Do not publish duplicate revisions blindly.

Enforce bounded requests/retries and verify the expected bucket owner. Do not offer a force-overwrite shortcut in this first version.

Completion: sequential transfers, concurrent publication, interrupted requests and refused conflicts preserve data in synthetic exercises.

## 5. Add first-time pairing and application controls

Owner: Codex, with Piero choosing the initial data source. Status: complete. Piero selected the laptop, published its workspace and confirmed the desktop joined it after a verified backup.

Ordinary restore still accepts only the same database ID. The explicit Join flow validates a different cloud workspace, rechecks the reviewed local and cloud revisions, and preserves a verified pre-join backup before replacing the local workspace. The old backup must be kept; normal graphical restore cannot directly restore a differently identified workspace.

1. Preserve verified backups of both installations.
2. Piero chooses which device contains the starting workspace.
3. Push that workspace to an empty cloud destination.
4. On the second device, show an explicit "Join existing workspace" flow, explaining any replacement and preserving its current data.
5. If both devices already contain valuable independent data, stop before replacement and agree on reconciliation. Snapshot sync cannot combine those databases automatically.

Add settings for non-secret configuration, connection checking, Push and Pull, plus last successful transfer and unpublished-change status. Disable incompatible operations during transfer. Show "Pushed" only after cloud publication is confirmed, and "Pulled" only after validated local application succeeds. A changed local database after a snapshot remains marked as unpublished.

Button wording and placement will follow existing Ebbi conventions. CLI commands can follow later if they are still wanted; the first delivery must be usable through the app.

Synthetic completion: Settings exposes local configuration, connection checking, publication, preview/confirmed join, Push, Pull, pending recovery and local change/last-recorded-sync status. A two-worker local mock exercise covered first publication, refused duplicate publication, stale previews, retained desktop backup, joining and later transfers in both directions. Modeled interruption checks covered join before and after replacement. Native synthetic Settings screenshots were inspected. The owner subsequently exercised the real UI and paired both devices in step 6.

## 6. Validate, package and use on both devices

Owners: Codex for implementation checks and packaging, Piero for AWS setup and the final device-to-device exercise. Status: the agreed live task-transfer and conflict-refusal scenario is complete. See the limitations below.

- Use only temporary synthetic databases for development checks.
- Exercise task creation, habit completion, numeric values, study notes/reviews, deletion, Push, restart and Pull in both directions.
- Exercise stale devices, unpublished local changes, concurrent Push, bad credentials, connection interruption, malformed snapshots, schema mismatch and recovery.
- Run formatting, an appropriate build and permitted static checks. Do not create or run Rust tests, package check stages or CI workflows that invoke them unless Piero explicitly lifts the restriction.
- Package the same compatible implementation for both devices, with package checks disabled while the restriction remains.
- Before real pairing, retain backups and review the selected starting workspace with Piero.
- Update usage instructions, implementation status, and the LLM-wiki Ebbi notes/log when implementation milestones change. Preserve immutable raw sources.

Live result: the owner reported successful task transfers in both directions, deletion synchronized back to the laptop, and refusal of a Pull when the laptop had unpublished habit edits. The owner preserved a conflict backup and explicitly restored the pre-publication copy before pulling. Both devices run package `0.1.0-3`; the old desktop executable was preserved and its launcher path linked to the packaged binary.

Limitations: real simultaneous publication, actual hardware power loss, a live round trip for every data type, and Rust 1.89 compilation were not verified. Rust tests and the package check stage were not run. Automatic conflict merging is not implemented.

## Ongoing costs and limits

S3 bills stored data, requests and applicable transfer. Whole snapshots and retained object versions accumulate storage. The actual bill depends on database size, transfer frequency, region and retention. This plan does not promise a particular bill or free-tier eligibility. [S3 pricing](https://aws.amazon.com/s3/pricing/).

Initially retain recovery history. Do not enable age-based deletion of current snapshot objects: the active head may still reference an old snapshot after months without edits. Define any retention cleanup separately with Piero before deleting data. Versioning provides recovery history, but does not resolve conflicting edits. [S3 versioning](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Versioning.html).

Manual sync cannot upload changes after the originating device has been turned off. It also cannot combine independently edited histories. There is no live-sync service, permanently running Ebbi helper or requirement for both devices to be online together.
