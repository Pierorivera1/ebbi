# AWS setup for Ebbi sync

Prepared on 2026-11-03. Follow these steps yourself to learn the setup. Codex has not created AWS resources or installed credentials. Ebbi's S3 transport and sync engine are under development; pairing, app controls and real-device validation remain in [EBBI_SYNC_ROADMAP.md](EBBI_SYNC_ROADMAP.md).

This guide first proves file transfers with synthetic text. Do not upload a live Ebbi database manually. The eventual app will create consistent SQLite snapshots and protect against conflicts.

## 1. Understand what you are creating

- A **region** is the AWS location where you keep the data.
- An S3 **bucket** is a container. An **object** is a stored file identified by a key such as `ebbi/setup-checks/laptop.txt`.
- An IAM **user** represents each device for this setup.
- An IAM **policy** states which requests that user can perform on which resources.
- An **access key ID and secret access key** let software authenticate requests. They are separate from your AWS console password.
- A local **profile** gives those credentials a name, `ebbi-sync`, so software can select them.

Your console administrator creates the resources. The two Ebbi users only transfer data inside the selected bucket. You do not sign into the console as those users.

You selected long-lived access keys to avoid interactive login. AWS recommends temporary credentials where possible. These device keys are an explicit convenience tradeoff, with limited permissions and separate revocation for each device. [AWS access-key guidance](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html).

## 2. Record non-secret setup values

Keep these values in your own notes. Replace placeholders in commands before running them.

```text
AWS account ID: YOUR_12_DIGIT_ACCOUNT_ID
Region: YOUR_REGION
Bucket: YOUR_BUCKET_NAME
Laptop IAM user: ebbi-laptop
Desktop IAM user: ebbi-desktop
Local profile on both devices: ebbi-sync
Object prefix: ebbi/
```

Choose one region and use it consistently. `us-east-1` is an example, not a requirement. Choose a unique bucket name using lowercase letters, digits and hyphens, such as `piero-ebbi-sync-<unique-suffix>`. The example is not a reserved name.

Do not put access keys in this file, the repository, Ebbi's database, screenshots or chat. You only need to share non-secret values for app configuration later.

## 3. Sign into AWS for administration

1. Open the [AWS console](https://console.aws.amazon.com/) and sign into your account using your normal administrator access.
2. If you do not have an account, follow [AWS's account setup](https://docs.aws.amazon.com/accounts/latest/reference/manage-acct.html) first.
3. Use administrative access for these resource-creation steps. Never create or use root access keys for Ebbi.
4. Find your 12-digit account ID in the account menu at the top right, or on the IAM dashboard. Record the digits without hyphens in your notes. [AWS account ID instructions](https://docs.aws.amazon.com/IAM/latest/UserGuide/console-account-id.html).

Account sign-in and device authentication are different. Signing out of the website does not disable the device's key.

## 4. Create the private bucket

1. Search for **S3** in the console and open it.
2. Choose **Create bucket** under general purpose buckets.
3. Select your region and enter your chosen bucket name.
4. Keep **Object Ownership** set to **Bucket owner enforced**, with ACLs disabled.
5. Keep every **Block Public Access** setting enabled.
6. Enable **Bucket Versioning**. If the creation form does not offer it, enable it afterward under the bucket's **Properties** tab.
7. For default encryption, select **Server-side encryption with Amazon S3 managed keys (SSE-S3)**. This avoids adding a customer-managed KMS key and KMS permissions to this setup.
8. Leave Object Lock off and do not add replication or lifecycle expiration rules.
9. Create the bucket. Open it and check that versioning is enabled under **Properties** and public access is blocked under **Permissions**.

No public bucket policy, public URL or website hosting is needed. The app will authenticate private requests over HTTPS. [AWS bucket setup](https://docs.aws.amazon.com/AmazonS3/latest/userguide/GetStartedWithS3.html), [S3 encryption](https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingServerSideEncryption.html).

Versioning retains earlier object versions. Each version consumes storage, and it does not automatically merge files. [AWS versioning](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Versioning.html).

## 5. Create the transfer policy

1. Search for **IAM** in the console.
2. Open **Policies**, then **Create policy**.
3. Select the **JSON** editor.
4. Replace the editor content with the policy below.
5. Replace both occurrences of `YOUR_BUCKET_NAME` with the actual bucket name. Keep the other syntax unchanged.
6. Continue, name it `EbbiSyncTransfer`, review the permissions, and create it.

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "InspectDedicatedBucket",
      "Effect": "Allow",
      "Action": [
        "s3:GetBucketLocation",
        "s3:ListBucket"
      ],
      "Resource": "arn:aws:s3:::YOUR_BUCKET_NAME"
    },
    {
      "Sid": "TransferEbbiObjects",
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject"
      ],
      "Resource": "arn:aws:s3:::YOUR_BUCKET_NAME/ebbi/*"
    }
  ]
}
```

The first statement permits listing this dedicated bucket and reading its location. The second permits downloading and uploading objects only under `ebbi/`. It grants no object deletion, bucket administration, IAM administration or access to other buckets. Older version recovery will use your administrator access rather than giving the app version-deletion permissions.

`arn:aws:s3:::...` identifies an AWS resource. The bucket ARN and object ARN are different, so they appear in separate statements. These are identity permissions attached to the device users. This policy assumes a commercial AWS region and same-account bucket ownership. [IAM JSON policy creation](https://docs.aws.amazon.com/IAM/latest/UserGuide/access_policies_create-console.html), [S3 permission mapping](https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-with-s3-policy-actions.html), [S3 policy examples](https://docs.aws.amazon.com/AmazonS3/latest/userguide/example-policies-s3.html).

## 6. Create one IAM user for each device

Do these steps for `ebbi-laptop`, then repeat for `ebbi-desktop`.

1. In IAM, choose **Users**, then **Create user**.
2. Enter the device's user name.
3. Do not enable AWS Management Console access. These users do not need passwords.
4. On the permissions step, select **Attach policies directly** and attach only `EbbiSyncTransfer`.
5. Review and create the user.
6. Open its **Security credentials** tab and choose **Create access key** in the access-keys section.
7. Read the alternatives warning. Choose the relevant external-application use case, or **Other** if required by the current form, and continue.
8. Add a description identifying the device if offered, then create the key.
9. Keep the displayed key pair private and configure it on the matching device in step 8. The secret can only be retrieved at creation time. Avoid saving an extra CSV unless you have a private place for it.

Use your administrator session to manage these users. Do not add key-management permissions to the device policy. Separate users let you deactivate one device without affecting the other. [IAM user creation](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_users_create.html), [access-key management](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html).

## 7. Install or locate AWS CLI on each device

AWS CLI is a terminal tool used for this learning exercise. Installing it does not create resources or sign you in.

Check whether it is already available:

```sh
aws --version
```

If the command is missing on an Omarchy or Arch Linux device, install the Arch repository's AWS CLI v2 package. It provides the `aws` command and its dependencies. Run:

```sh
sudo pacman -S aws-cli-v2
```

Enter your Linux password if prompted and review pacman's package list before confirming. This installs a package on that computer; it does not create AWS resources. If pacman reports a package conflict or update problem, stop and resolve it before continuing. [Arch package details](https://archlinux.org/packages/extra/any/aws-cli-v2/).

For another operating system, use the [official AWS CLI installation guide](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html). For Linux, select the matching x86_64 or ARM64 installer. Do not install a second copy if one already works.

After installation, open a new terminal if necessary and repeat `aws --version`. Proceed once the output starts with `aws-cli/2`. If it starts with `aws-cli/1`, consult the AWS installation guide to upgrade rather than installing another copy blindly. The commands below target AWS CLI v2 on Linux.

## 8. Save the matching device key in a named profile

On each device, use that device's own key. Run this as your normal desktop user, without `sudo`:

```sh
(
  set -e
  umask 077
  mkdir -p "$HOME/.aws"
  chmod 700 "$HOME/.aws"
  aws configure --profile ebbi-sync
  chmod 600 "$HOME/.aws/credentials" "$HOME/.aws/config"
)
```

The configure command asks for four values. Enter the device's access key ID, its secret access key, your selected region, and `json` for output format. Enter credentials only at those local prompts. Do not put literal secrets in shell commands or paste terminal output containing them into chat.

This adds a named profile and preserves other profiles. If an `ebbi-sync` profile already exists, the prompts update that profile. It stores credentials under `~/.aws/credentials` and configuration under `~/.aws/config`, unless you have customized the AWS file locations. The permission commands above assume the default locations and also restrict access to other profiles stored in those files. These are plaintext files restricted to your Linux user, not an encrypted keyring. Do not copy the laptop's entire credentials file onto the PC.

The future app will explicitly select `ebbi-sync` rather than using your administrator/default profile. Normal use will not require repeating `aws configure`. [AWS profiles and credential files](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html).

## 9. Verify each device's identity

On both devices:

```sh
aws sts get-caller-identity --profile ebbi-sync --no-cli-pager
```

The response should show your account ID and a user ARN ending in `user/ebbi-laptop` or `user/ebbi-desktop`, matching the device. If it shows an administrator or unexpected identity, stop and fix the profile before transferring data.

This checks authentication, not S3 permissions. It does not reveal the secret key and does not need an extra IAM allow statement. [AWS identity command](https://docs.aws.amazon.com/cli/latest/reference/sts/get-caller-identity.html).

## 10. Transfer synthetic data between the devices

Run this setup block in the terminal on each device, replacing the placeholders. These variables contain no secrets:

```sh
EBBI_BUCKET='YOUR_BUCKET_NAME'
EBBI_REGION='YOUR_REGION'
EBBI_ACCOUNT='YOUR_12_DIGIT_ACCOUNT_ID'
```

On the laptop, create a temporary directory and a harmless text file:

```sh
EBBI_DEMO_DIR=$(mktemp -d)
printf '%s\n' 'Hello from the Ebbi laptop setup exercise.' > "$EBBI_DEMO_DIR/laptop.txt"
aws s3api put-object --profile ebbi-sync --region "$EBBI_REGION" --bucket "$EBBI_BUCKET" --expected-bucket-owner "$EBBI_ACCOUNT" --key ebbi/setup-checks/laptop.txt --body "$EBBI_DEMO_DIR/laptop.txt" --no-cli-pager
```

Expected result: upload metadata including an ETag, and a VersionId when bucket versioning is enabled. An ETag is an object identifier used for conditional requests; do not assume it is a file checksum.

On the PC, after setting the same three non-secret variables:

```sh
EBBI_DEMO_DIR=$(mktemp -d)
aws s3api get-object --profile ebbi-sync --region "$EBBI_REGION" --bucket "$EBBI_BUCKET" --expected-bucket-owner "$EBBI_ACCOUNT" --key ebbi/setup-checks/laptop.txt --no-cli-pager "$EBBI_DEMO_DIR/from-laptop.txt"
cat "$EBBI_DEMO_DIR/from-laptop.txt"
```

Expected content is exactly `Hello from the Ebbi laptop setup exercise.` The laptop can be off during this download. [S3 uploading and downloading](https://docs.aws.amazon.com/AmazonS3/latest/userguide/uploading-downloading-objects.html).

Now practice in the other direction. On the PC, use the same terminal where you set `EBBI_BUCKET`, `EBBI_REGION` and `EBBI_ACCOUNT`:

```sh
EBBI_DEMO_DIR=$(mktemp -d)
printf '%s\n' 'Hello from the Ebbi desktop setup exercise.' > "$EBBI_DEMO_DIR/desktop.txt"
aws s3api put-object --profile ebbi-sync --region "$EBBI_REGION" --bucket "$EBBI_BUCKET" --expected-bucket-owner "$EBBI_ACCOUNT" --key ebbi/setup-checks/desktop.txt --body "$EBBI_DEMO_DIR/desktop.txt" --no-cli-pager
```

On the laptop, in the terminal where you set those same variables, download and display it:

```sh
EBBI_DEMO_DIR=$(mktemp -d)
aws s3api get-object --profile ebbi-sync --region "$EBBI_REGION" --bucket "$EBBI_BUCKET" --expected-bucket-owner "$EBBI_ACCOUNT" --key ebbi/setup-checks/desktop.txt --no-cli-pager "$EBBI_DEMO_DIR/from-desktop.txt"
cat "$EBBI_DEMO_DIR/from-desktop.txt"
```

Expected content is exactly `Hello from the Ebbi desktop setup exercise.` If you opened a new terminal on either device, set the three non-secret variables again before running its commands. No personal data is involved. Temporary local directories and the tiny cloud exercise objects are retained; this guide does not delete data.

Do not use `aws s3 sync` on Ebbi's live database directory. It does not implement Ebbi's future conflict protocol or SQLite snapshot handling.

## 11. Understand the costs before daily use

S3 Standard remains available while your devices are off, subject to service availability. There is no EC2 instance to start, stop or pay to keep running. Stored objects, requests and applicable downloads are billed. Whole-database snapshots and retained versions increase storage over time. Do not assume this is free based on an old free-tier tutorial. [S3 availability](https://docs.aws.amazon.com/AmazonS3/latest/userguide/DataDurability.html), [current S3 pricing](https://aws.amazon.com/s3/pricing/).

Review your account's Billing console and, if wanted, create a budget notification using an amount you choose. A notification is not a spending cap. Do not enable object expiration rules yet: an old snapshot may still be the active workspace. [AWS budget creation](https://docs.aws.amazon.com/cost-management/latest/userguide/budgets-create.html).

## 12. Troubleshoot without broadening permissions

- **Unable to locate credentials or profile not found**. Run configuration on this device as your normal user, and keep `--profile ebbi-sync` on commands.
- **InvalidAccessKeyId or SignatureDoesNotMatch**. Check the key pair locally. Reconfigure the profile if necessary. Check that the device clock is correct. Do not share the key for diagnosis.
- **AccessDenied**. Check the policy is attached to the correct user, both bucket names were replaced, the key begins with `ebbi/`, and the expected account ID is correct. A higher-level explicit deny can also block access. Do not attach AdministratorAccess or make the bucket public to fix this.
- **Wrong region, PermanentRedirect or AuthorizationHeaderMalformed**. Use the bucket's actual region consistently.
- **NoSuchKey**. Confirm the upload succeeded and the object key matches exactly, including case.
- **Browser access denied**. A private object URL is not a public download link. Use authenticated CLI/app requests.
- **The IAM user cannot browse the whole S3 console**. The policy is for programmatic access. Use your administrator session for resource management.
- **Connection or certificate failure**. Check internet connectivity and system time. Do not disable TLS verification.

For S3 authorization failures, use [AWS's access-denied guidance](https://docs.aws.amazon.com/AmazonS3/latest/userguide/troubleshoot-403-errors.html).

If a device is lost or a key is exposed, use your administrator console session, open IAM, select that device's user, open **Security credentials**, and deactivate the affected key. The other device's user continues to work. Create a replacement and configure it locally before removing the old key permanently. [AWS key management](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html).

## 13. Pair the two Ebbi installations

Both devices are now installed and paired, using the laptop as the starting workspace. The steps below document the completed initial setup; do not repeat publication or joining on the paired devices. If setup must be repeated on a replacement device, review existing data and backups before confirming any replacement.

On **each device**, after the updated app is installed:

1. Open Ebbi and select **Settings**. Scroll to **Sync between devices**.
2. Enter profile `ebbi-sync`, bucket `ebbi-sync-piero-095890667649-us-east-1-an`, region `us-east-1`, and account ID `095890667649`.
3. Select **Save sync settings**, then **Check connection**. The credentials stay in that device's AWS profile. Do not enter the access key or secret into Ebbi.

On the **laptop**:

1. Check that its tasks, habits and study data are the starting copy you want.
2. In Settings, select **Publish starting workspace**, review the confirmation and choose **Confirm publication**.
3. Wait for the success message. Ebbi creates a verified local backup before publishing and displays its path. Keep that path. Publication refuses to replace an existing cloud workspace.

On the **desktop PC**:

1. Before joining, check whether the PC contains any independent data you want to keep. Joining does not merge databases. If both contain important independent data, stop and decide how to reconcile it before continuing.
2. Select **Join cloud workspace**. Review the cloud counts and the warning about replacing this device's workspace.
3. Only when you are ready, select **Confirm join**. Ebbi saves a verified backup of the PC's current workspace before replacement and displays its path. Keep that path.
4. Confirm that the laptop's data appears on the PC. The owner completed this check during real-device validation.

The PC's pre-join backup belongs to its old workspace. Ebbi's ordinary graphical restore accepts backups of the current workspace only, so it cannot directly restore that older, differently identified workspace after joining. Keep the backup and ask for recovery guidance if you need it; do not overwrite it.

For everyday use, open Ebbi, **Pull** before editing if you last used the other device, then **Push** and wait for confirmation before switching devices. If both copies have unpublished changes, Ebbi refuses the unsafe transfer. An unresolved interrupted operation pauses sync rather than guessing. No AWS console visit or interactive AWS login is part of the routine.

## 14. If Ebbi reports a sync conflict

Stop and preserve both copies. A Pull is refused when this device has unpublished local changes; a Push is refused when the cloud has advanced. Ebbi does not automatically merge independent edits. Back up the local workspace and identify what changed on each device before deciding how to reconcile them. Do not restore an old backup unless you understand which edits it will replace and have preserved the current data.

Checking and then unchecking a habit can still count as a local change because Ebbi records an unchecked entry in the database. During the first live check, the owner backed up the laptop, explicitly restored the pre-publication snapshot after identifying those edits, and then pulled successfully. This was specific to that situation and is not a general conflict-resolution shortcut.
