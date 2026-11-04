# Ebbi: architecture, decisions and lessons

Last updated: 2026-11-04. This is an engineering record of Ebbi and its two-device sync, based on the repository, project status records and the owner's reported AWS and device checks. It describes the implementation at package version `0.1.0-3`. It is not a promise that every failure scenario has been tested.

For the current setup procedure see [AWS_SETUP.md](AWS_SETUP.md). For the implementation history and verification boundaries see [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) and [EBBI_SYNC_ROADMAP.md](EBBI_SYNC_ROADMAP.md).

## 1. The problem

Ebbi is a personal Linux desktop application for tasks, habits and spaced-repetition study. The owner wanted to use it on a laptop and a desktop PC. Either device may be shut down while the other is in use. Changes must be available later, and ordinary use must not require a server process running on either machine.

The chosen workflow is **manual, local-first synchronization**:

1. Pull before editing on a device when the other device was used most recently.
2. Make changes locally.
3. Push and wait for success before switching devices.

Each device works with its own SQLite database. S3 stores a published snapshot so the other device can retrieve it later. Both devices need internet access when pushing or pulling, but they do not need to be online at the same time.

A push or pull happens only when the user requests it. A change left on a powered-off device cannot appear on the other device until the first device is turned on and its change is pushed.

## 2. Decision record

| Decision | Why it was selected | Cost or limitation |
| --- | --- | --- |
| One native desktop app | Brings tasks, habits, study and analytics into one workflow. | A failure in one app can affect access to all features; persistence safety matters. |
| Rust and Iced | Continues the stack used by the earlier Habitodo project and keeps a native desktop interface. | The AWS SDK and graphics dependencies increase build time and package size. |
| SQLite on each device | Local operations remain useful without a cloud request. | The two local databases can diverge and need a deliberate reconciliation protocol. |
| Complete database snapshots | Moves all related records together without building an event stream or per-table merge protocol. | Each push transfers a complete snapshot and retains another object. The current size limit is 256 MiB. |
| Manual Push and Pull | Matches the owner's two-device workflow and needs no permanent helper process. | The owner must remember to push and pull in the correct order. There is no live update. |
| S3 as durable exchange storage | A device can publish while the other is powered off. No EC2 server is required. | AWS credentials, S3 availability, storage charges and conflict handling become part of the system. |
| Conditional publication of a small head object | Detects another writer changing the cloud revision during a push. | A conflicting edit still needs human reconciliation; atomic publication is not automatic merging. |
| Refuse conflicts | Protects both histories instead of silently replacing one with the other. | The first version has no automatic merge or force-overwrite button. |
| S3 versioning and retained snapshots | Keeps earlier cloud object versions and previous snapshots available for investigation and recovery. | Retained data consumes storage. Versioning alone is not a backup policy or a merge engine. |
| Separate IAM user and access key per device | Both devices work without interactive AWS login; either credential can be revoked separately. | The access keys are long-lived and the local AWS credentials files are plaintext. |
| AWS SDK for Rust | The application can access S3 directly and use conditional writes and the selected profile. | Adds a substantial dependency graph. The AWS CLI remains useful for setup and diagnosis but is not the app's transport. |
| One verified starting workspace | The laptop's existing data was chosen as the initial cloud copy; the PC joined it after a backup and confirmation. | Independently created databases cannot safely be assumed to have the same identity or history. |
| Same Arch package on both devices | Both devices run compatible sync protocol and application code. | Updating source on the laptop does not automatically update either installed application. |

WebSockets were considered in the initial discussion. A live connection alone would not store a change while the other computer was off, and the user did not require live updates. S3 plus explicit Push/Pull solved the actual availability requirement with fewer running components.

## 3. Desktop application architecture

```text
Iced UI and application state
            |
            | requests and results
            v
  single database worker thread
       |                 |
       |                 +--> Sync engine --> AWS SDK --> S3
       v
SQLite database and backup/restore code
```

The main parts are:

- `src/app.rs`, `src/ui.rs`, `src/study_ui.rs` and `src/analytics_ui.rs`: application state and interface.
- `src/domain.rs` and `src/analytics.rs`: domain models, scheduling and analytics rules.
- `src/worker.rs`: serializes database and sync operations on a worker thread, keeping SQLite and network work off the UI thread.
- `src/persistence/`: SQLite schema, migrations, imports, queries, backup and restore.
- `src/sync/`: sync protocol, S3 transport, snapshot inspection and conflict/recovery logic.
- `src/sync_worker.rs`: device-local sync configuration and the worker integration.
- `src/instance.rs`: directs a second launch for the same canonical database path to the existing instance through a private Unix socket. It does not claim to force focus under Wayland.

The database is local and persistent. The design explicitly avoids a silent in-memory fallback if the on-disk database cannot be opened. Showing a successful save while using temporary storage would create an unacceptable risk of data loss on exit.

SQLite uses WAL mode, foreign keys, full synchronization and a bounded busy wait. Advisory locks and a stable path lock support single-instance access and safe replacement during restore. These locks depend on cooperating processes; they do not stop a separate writer that ignores them.

### 3.1 Data model and product decisions

The original applications remain separate source projects: Habit Tracker/Habitodo and Repasito. Ebbi uses a new database and provides explicit import workflows. Original databases must not be overwritten as part of an import. Import logic uses snapshots, provenance and duplicate checks rather than treating an old database as the live Ebbi database.

The current schema is version 2. It includes tasks, habits, effective-dated habit rules and goals, binary entries, numeric values, study topics, reviews, review rescheduling history, settings and legacy import provenance. Upgrading a supported version 1 database creates a verified pre-upgrade backup before the migration. SQLite's `application_id`, `user_version` and migration ledger help distinguish supported Ebbi files from other databases.

The study schedule uses fixed reviews at +1, +3 and +5 calendar days from the study date. This is a scheduler, not an adaptive memory algorithm. Completed review history and original deadlines are preserved through supported edits. Numeric habits use explicit goal comparators and effective dates so later goal changes do not silently rewrite old outcomes. Analytics distinguish an unavailable denominator from a genuine 0% result.

The application has no resident sync daemon after exit. Local saves and the UI do not depend on S3 being reachable.

## 4. AWS architecture

```text
Laptop                         AWS account                       Desktop PC
------                         -----------                       ----------
Ebbi + SQLite                  S3, us-east-1                     Ebbi + SQLite
profile: ebbi-sync  -- HTTPS --> private versioned bucket <-- HTTPS -- profile: ebbi-sync
IAM user: ebbi-laptop          ebbi/head.json                    IAM user: ebbi-desktop
                               ebbi/snapshots/<revision>.db
                               ebbi/setup-checks/...
```

The bucket is `ebbi-sync-piero-095890667649-us-east-1-an`, a general-purpose bucket in `us-east-1`. It uses the account regional namespace, whose name includes the account ID, region and `-an` suffix. AWS says another account cannot recreate an account-regional bucket name. [AWS: general-purpose bucket namespaces](https://docs.aws.amazon.com/AmazonS3/latest/userguide/gpbucketnamespaces.html).

The setup used blocked public access, bucket-owner-enforced ownership with ACLs disabled, S3 Versioning and S3-managed server-side encryption, SSE-S3. The successful CLI exercise returned `AES256` and version IDs. SSE-S3 protects object data at rest; it is not end-to-end encryption controlled exclusively by the owner, and it does not encrypt S3 object metadata. HTTPS protects the app's normal transport. [AWS: SSE-S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingServerSideEncryption.html), [AWS: encryption in transit](https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingEncryptionInTransit.html).

No EC2 instance, WebSocket server, API Gateway, Lambda, database service or VPN is required for this design. S3 is a managed object store, not a process that runs on the laptop or PC. Availability is subject to AWS service and network availability; the application is not guaranteed to be online at every instant.

### 4.1 IAM and credentials

Two users were created: `ebbi-laptop` and `ebbi-desktop`. Each has its own access key and the same customer-managed `EbbiSyncTransfer` policy. The owner configured the `ebbi-sync` named profile independently on each machine. CLI identity checks identified the corresponding user on each device.

The policy described in [AWS_SETUP.md](AWS_SETUP.md) grants:

- `s3:GetBucketLocation` and `s3:ListBucket` on the dedicated bucket.
- `s3:GetObject` and `s3:PutObject` under the bucket's `ebbi/*` prefix.
- No object-delete, bucket-administration or IAM-administration actions in this policy.

The scope is narrower than account-wide S3 access, but it is not limited to exactly two physical machines. An access key can be copied, and the policy permits writes within the prefix. Separate identities make it possible to revoke one device without rotating the other's key; they do not bind a key cryptographically to a computer.

The AWS SDK is configured with a profile-file credential provider and the configured profile name, region and expected bucket owner. Each request supplies the expected account ID so a mismatched bucket owner causes the request to fail. No credential is stored in the sync head or device sync state. AWS's shared credentials file is plaintext, protected here by local filesystem permissions rather than a desktop keyring. The owner explicitly selected long-lived keys for convenience; AWS recommends temporary credentials where feasible. [AWS: IAM access keys](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html).

A screenshot during setup showed part of an access key and part of its secret. The complete secret was not visible in the screenshot. Rotation was advised; the owner chose not to rotate it at that time. Whether it was rotated later has not been verified. A partial exposure is not proof of compromise, but it is a reason to consider replacement and avoid credential screenshots in future.

The owner used a root console session for initial setup. Root access keys were not part of the agreed setup. Routine administration is better performed using a suitable administrative identity, with root protected and reserved for necessary account-level tasks. MFA or later account administration settings were not independently verified. [AWS: IAM best practices](https://docs.aws.amazon.com/IAM/latest/UserGuide/best-practices.html).

### 4.2 Why the bucket key was disabled

An S3 Bucket Key is associated with SSE-KMS and reduces KMS request use in that configuration. This bucket uses SSE-S3, not a customer-managed KMS key, so enabling a KMS bucket key did not solve a requirement here. Object Lock was not enabled. Object Lock, versioning, application-level conditional writes and backups are distinct mechanisms; this project should not describe versioning as an immutable retention policy.

## 5. The sync protocol

### 5.1 Object layout

```text
ebbi/head.json
ebbi/snapshots/<random-revision>.db
ebbi/setup-checks/laptop.txt
ebbi/setup-checks/desktop.txt
```

`setup-checks/` contains harmless files from the initial CLI exercise. The head points to the current snapshot. Each revision ID is generated from 16 random bytes and represented as 32 lowercase hexadecimal characters.

The head includes the protocol version, schema version, workspace ID, revision ID, parent revision, snapshot key, SHA-256 of the snapshot bytes, logical fingerprint and originating device ID. It does not contain the access key or secret. The current code accepts protocol version 1 and database schema version 2, caps the head at 16 KiB and caps a snapshot at 256 MiB.

There are two different hashes:

1. **Content SHA-256** checks that downloaded snapshot bytes match the head.
2. **Logical SHA-256** hashes the schema and table-row values in a stable order, so SQLite page layout or backup header changes do not by themselves make an unchanged workspace appear edited.

The logical fingerprint includes the database's user tables and schema, with special handling for `sqlite_sequence`. Device sync settings and AWS credentials are stored separately. The database itself can still contain local context such as legacy import provenance and source paths; do not assume every field in a snapshot is free of machine-specific information.

A hash is an integrity check, not a signature. Someone who can alter both a snapshot and the head could replace both the content and its checksum. Likewise, the app uses create-only snapshot writes, but the IAM policy itself permits `PutObject` under the prefix; it does not enforce immutable snapshot names against every possible client.

### 5.2 Consistent snapshots and SQLite WAL

A SQLite database may have committed data in a write-ahead log. Copying only the main live `.db` file risks transferring an incomplete view. Ebbi uses SQLite's backup API through `rusqlite` to create a new private snapshot, checks SQLite integrity and foreign keys, and closes and syncs the snapshot before it is used. A raw `aws s3 sync` of the live database directory would bypass that protocol.

The database worker serializes sync operations with local writes. For an operation that replaces the database, the existing restore path validates the candidate, retains a verified recovery snapshot, handles locks and sidecars, replaces the file and reopens the database. Some post-replacement filesystem or reopen failures can still require recovery from the retained snapshot. Hardware power-loss behavior was not proven.

### 5.3 Push: snapshot first, publication second

For a paired workspace, a push follows this order:

1. Load the device's accepted cloud revision and create a consistent local snapshot.
2. Read the cloud head and require it to match the accepted workspace, revision and logical fingerprint. If it has advanced, refuse the push.
3. Upload the new snapshot under its revision key with `If-None-Match: *`, so the app does not intentionally overwrite an existing snapshot key.
4. Persist a local pending-push record.
5. Update `head.json` using `If-Match` with the previously observed ETag.
6. Once publication is confirmed, persist the accepted revision and show success.

The head update is the cloud **commit point**. A snapshot object uploaded without a successful head update is not the published workspace. If the head update fails, the uploaded object can become an orphan that still consumes storage.

The first publication follows a similar path, but requires an empty cloud head, creates a local backup first and uses `If-None-Match: *` for the initial head. AWS documents the ETag precondition and conditional create semantics; its current S3 model provides strong read-after-write consistency for object PUT and DELETE operations. The S3 semantics help coordination, but they do not provide a multi-object transaction or a record-level merge. [AWS: conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html), [AWS: S3 consistency](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html).

An ETag is used as an opaque conditional-write token here. It must not be assumed to be a SHA-256 checksum or universally treated as an MD5 checksum.

### 5.4 Pull and first-time join

A normal pull first compares the current local logical fingerprint with the accepted revision. If local data has changed, or if the workspace identity is wrong, it refuses to replace the database. If the cloud has advanced and the local copy is clean, it downloads the referenced snapshot, checks its content hash, identity, schema, logical fingerprint and SQLite integrity, then restores it while keeping a recovery backup.

First-time join is different. A desktop installation created independently has its own database ID, so ordinary same-workspace restore would reject the laptop's snapshot. The Join flow downloads and validates a preview, shows record counts and asks for confirmation. Before replacing anything, it verifies again that the reviewed local contents and cloud head have not changed. It retains a verified pre-join backup of the old desktop workspace.

That pre-join backup has the old workspace's identity. Ordinary graphical restore accepts a backup of the current workspace, so it cannot directly restore the differently identified pre-join database after pairing. Keep that backup and obtain a deliberate recovery plan if it is needed. Do not overwrite it.

### 5.5 Pending operations and uncertain outcomes

A network timeout does not prove that S3 rejected a request. The server may have committed a write even if the client did not receive the response. The device-local state therefore records pending initial pushes, pushes, pulls and joins before their relevant commit or replacement step.

On recovery, the code compares the pending record with the cloud head or the local snapshot:

- If a pending push is visibly the current head, it can be recorded as accepted.
- If the immediate cloud successor names the pending revision as its parent, the code can recognize that the pending revision was published and the cloud advanced again.
- If another immediate child of the accepted base won the conditional write, the pending push is reported as a conflict and the local history remains.
- If the observed cloud history does not establish the outcome, sync pauses rather than guessing.
- For interrupted pull or join, the local workspace identity and logical fingerprint determine whether the previous or replacement state is present. Unexpected local content pauses recovery.

The head stores one parent, not a complete browsable ancestry graph. In particular, the app cannot always infer what happened after multiple further cloud revisions or manual cloud changes. A paused operation can require investigation. The user-facing conflict-resolution workflow is still limited; automatic merging is not implemented.

### 5.6 Durable local state

Each database path has device-local sync configuration under the user's XDG data directory. The path includes a SHA-256 derived from the database path. The state records non-secret settings, device ID, accepted workspace/revision/fingerprint, last recorded sync time and any pending operation. It is separate from the synchronized database.

State is written to a new private temporary file, flushed, renamed and followed by a directory sync. An error is reported rather than treated as successful state persistence. This is an attempt at crash-safe bookkeeping, not evidence that all filesystem, disk-full or hardware failure modes have been exercised.

The Settings message **No unpublished local changes** compares the local database with the locally accepted revision. It does not contact S3 and does not prove that the cloud has not advanced. A Push or Pull still checks the cloud.

## 6. Conflict behavior in practice

Suppose both devices last accepted revision `R1`:

```text
                         laptop edits locally
                        /
shared revision R1 ----
                        \
                         desktop edits and publishes R2
```

The laptop now has unpublished edits based on `R1`, while the cloud points to `R2`. Pull must not replace the laptop's edits. Push must not replace the desktop's newer cloud head. This is a real conflict even if only two devices exist.

This happened during the live exercise. The owner checked and then unchecked some laptop habits after publishing the initial workspace. Ebbi writes a `habit_entries` row even for an unchecked value, so the database fingerprint changed although the visible checkboxes ended unchecked. Meanwhile the desktop published a check task. The laptop correctly refused Pull.

The recovery was specific to those known changes:

1. The owner saved a verified backup of the laptop's changed state.
2. After identifying the checked/unchecked edits, the owner explicitly restored the pre-publication snapshot.
3. Ebbi confirmed that the laptop had no unpublished local changes.
4. Pull succeeded and the desktop's check task appeared.

Do not generalize this into "always restore an old backup". If the local edits are valuable, restoring an older snapshot would replace them. First back up and understand both histories, then choose a reconciliation plan. The conflict backup and restore recovery snapshots were retained.

## 7. Build, packaging and deployment

The application is built with Cargo and packaged as an Arch Linux/Omarchy package. `Cargo.lock` records the resolved dependency versions. The package recipe produces `ebbi 0.1.0-3` for `x86_64`, containing the binary, desktop entry, application icon and licenses. It does not package the user's databases or credentials.

A clean local source archive was used for the release build. The initial build used two Cargo jobs and caused CPU heat that concerned the owner. It was stopped. The build was resumed from the existing build tree with one Cargo job and a systemd user scope limited to 50% of one logical CPU. This reduced CPU allocation at the cost of a longer build; it is not a guaranteed temperature ceiling. The resulting package was locally built and unsigned, not built in a clean chroot or published as a signed release.

The laptop's installed binary was checked against the inspected package. The same package was uploaded to the owner's Google Drive, downloaded on the desktop and checked against its SHA-256 before installation. Google Drive was used to transfer the **application package**, not as Ebbi's data-sync backend.

After installation, the desktop still opened an older app. The package manager reported `0.1.0-3`, but the per-user desktop entry explicitly launched `~/.local/bin/ebbi`, which was an old executable. The owner preserved that executable in a separate directory and placed a symlink to `/usr/bin/ebbi` at the launcher path. The owner then confirmed the normal launcher showed the new sync settings.

The lesson is that a successful package installation does not prove the desktop launcher or shell resolves the packaged binary. Check the actual process path, per-user desktop entry, PATH ordering and binary identity when behavior does not match the installed package.

## 8. Verification and evidence

### Verified during this project

- The owner configured both device profiles and reported the expected IAM identity on each device.
- The owner uploaded and downloaded harmless CLI exercise files in both directions. The upload responses showed encryption and version IDs.
- Formatting and locked offline builds passed. A package was built with `makepkg --nocheck`; package contents and an extracted native Wayland launch with a synthetic database were inspected.
- Synthetic standalone examples and local mock transports exercised snapshots, saved revision state, task edits, restore, pairing, unchanged/changed previews, a retained old workspace, lost publication response, corrupt download, conditional writes, stale ETags, a simulated concurrent publication and selected interruption-recovery cases.
- The owner confirmed the laptop's existing workspace was published, previewed and joined on the desktop, and its tasks, habits and study topic appeared.
- The owner created a check task on the desktop, pushed it, and saw it on the laptop after resolving the real conflict. A laptop-created check task then reached the desktop. The owner deleted both check tasks, pushed from the desktop and pulled the deletion on the laptop.
- The owner confirmed the application refused the laptop Pull when unpublished local habit changes existed. A verified backup was made before the chosen restore.

### Important limits

- The owner prohibited creating or running Rust tests and commands that invoke them during this sync work. No Rust tests or package `check()` stage were run for this delivery. Historical status records describe earlier test runs before that restriction; they are not evidence of a new sync test suite.
- Live two-way transfer was demonstrated with tasks and their deletion. Binary and numeric habits, study notes and reviews were exercised in synthetic transfers and appeared in the initial desktop workspace, but a live round trip for every data type was not demonstrated.
- Real simultaneous cloud publication, actual process interruption at every critical boundary, full-disk behavior and hardware power loss were not verified. Local mocks and modeled restart scenarios do not establish all AWS or filesystem behavior.
- The lockfile was resolved with the stated Rust 1.89 minimum in mind; the build used Rust 1.99.1. An actual Rust 1.89 build was not performed.
- The app is designed for the owner's two-device workflow. There is no automatic record-level merge, guaranteed cloud availability or proof of complete data-loss prevention.

## 9. Mistakes, corrections and what they taught us

| What happened | Correction | Engineering lesson |
| --- | --- | --- |
| We initially discussed WebSockets for devices that may be off at different times. | Used durable S3 objects and manual operations. | First describe the availability and delivery requirements; a transport alone is not persistence. |
| The IAM access-key creation screen recommended interactive alternatives. | Continued with the owner's explicit choice of separate long-lived keys and named profiles. | Security and usability tradeoffs should be explicit; accepting a risk does not make plaintext long-lived credentials temporary or encrypted. |
| A screenshot exposed portions of a key and secret. | Rotation was recommended; the owner declined at that time. | Credential entry and diagnostics should avoid screenshots or logs that display secrets, even partially. |
| We first treated two independently initialized databases as if a normal restore could pair them. | Added a separate preview/confirm Join path with identity checks and recovery backup. | Data identity is part of the protocol; two files with the same schema are not automatically the same workspace. |
| The initial release build made the laptop too hot for the owner's comfort. | Stopped it and resumed with one build job and a 50% CPU quota. | Limiting job count is not a CPU quota. Resource limits and operational constraints belong in the build plan. |
| The first package attempt declared CMake as a system dependency, but the machine lacked it. | Inspected the dependency build path and used the available C compiler instead of installing an unnecessary tool. | A Rust build dependency named `cmake` does not by itself prove the CMake executable is required for every target and configuration. |
| The desktop showed the old UI after installing the new package. | Traced the running executable and desktop entry; preserved the old file and linked the launcher path to the packaged binary. | Package version, PATH resolution, running process and desktop launcher are different layers. Verify the actual executable. |
| A real Pull was refused after the owner checked and unchecked habits. | Preserved a backup, identified the changes and restored the known pre-publication snapshot before Pull. | A visually reversed action can still change persisted state. Conservative conflict detection needs an understandable recovery workflow. |
| The first guide described resources and sync as future work. | Updated the roadmap and setup guide as milestones changed. | Operational documentation must separate historical checkpoints from current state. |

The conflict experience also revealed a product improvement opportunity: make it easier to understand what changed locally and provide a guided reconciliation flow. Any such feature must preserve both histories and should be designed and validated before use on personal data.

## 10. Operational guidance and open work

For normal use, Pull before editing on the device that was not used last, and Push before switching away. Wait for the success message. If a conflict or uncertain operation is reported, do not delete objects, reset sync state, restore a backup or force an overwrite until both copies are preserved and the changes are understood.

S3 storage, requests and applicable transfer can incur charges. Each unique snapshot remains a separate object; versioning can also retain old versions of overwritten objects such as the head. There is no implemented automatic retention cleanup. A simple rule that deletes all old snapshots by age would be unsafe if the active head still points to an older snapshot. Any lifecycle or cleanup design must protect the active reference and explicitly define recovery retention. [AWS: S3 pricing](https://aws.amazon.com/s3/pricing/), [AWS: S3 Versioning](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Versioning.html).

Potential future work, subject to the owner's choice:

- A guided conflict comparison and reconciliation process, with explicit backups and no silent data loss.
- A documented, safe retention strategy and cost monitoring.
- Stronger credential storage or a temporary-credential approach, after considering the desired login experience.
- Clear recovery of a differently identified pre-join workspace.
- Broader fault-injection and live-device verification, subject to the owner's current prohibition on Rust tests.
- Release automation and performance benchmarks, which are separate remaining desktop-release items.

## 11. Primary references

- [SQLite online backup API](https://www.sqlite.org/backup.html) and [SQLite WAL](https://www.sqlite.org/wal.html).
- [S3 conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html) and [S3 data consistency](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html).
- [S3 account regional namespaces](https://docs.aws.amazon.com/AmazonS3/latest/userguide/gpbucketnamespaces.html), [versioning](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Versioning.html) and [SSE-S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingServerSideEncryption.html).
- [IAM access-key guidance](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html) and [shared AWS credentials files](https://docs.aws.amazon.com/sdkref/latest/guide/file-format.html).
- [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) and [systemd CPU resource controls](https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html).
