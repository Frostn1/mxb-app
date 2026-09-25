# Windows code signing: setup

The Windows builds of **MXB App**, **Frost Studio** and **MXB Coach** are signed with
**Azure Artifact Signing** (Microsoft renamed it from *Trusted Signing* in January 2026: same
service, new names). All three are built by this repo's release workflows, so everything below
is set up once, here.

Until every value in [step 7](#7-add-the-github-secrets-and-variables) exists, the release
workflows log *"Windows code signing skipped"* and build unsigned, exactly as today. Nothing
fails, so this can be done at your own pace.

**What gets signed:** each app's exe, its NSIS installer and uninstaller (Tauri signs these as it
bundles, through `bundle.windows.signCommand`), and `mxbsecure.dll`, the DLL MXB App injects.
The signature shows the publisher as **Creste LLC**.

**You do every step in this file yourself.** They involve accounts, identity documents, payment
and secrets, so no automation or assistant should do them for you.

- Microsoft's docs: <https://learn.microsoft.com/azure/artifact-signing/>
- The signing code: `.github/actions/artifact-signing/action.yml` and `scripts/sign-windows.ps1`.

---

## 0. Before you start

For **organization** validation (a certificate that names *Creste LLC*), have these ready. They
must match public records, or validation stalls:

| Field | Value | Notes |
|---|---|---|
| Organization name | **Creste LLC** | The exact legal name, as registered. |
| Website | **https://www.creste.dev** | Must belong to the company. |
| Primary email | an address **@creste.dev** | Monitored, and accepts links from outside senders. Verification links expire in 7 days. |
| Secondary email | a *different* address **@creste.dev** | Same domain as the primary. A distribution list is fine. |
| Business identifier | **Creste LLC's EIN** | The US federal Employer Identification Number. |
| Address | Creste LLC's business address | Exactly as it appears in public records (state registration, IRS). |
| Representative | your first and last name | Exactly as on your government ID. You complete an ID check. |

Microsoft may ask for supporting documents. They must be issued within the last 12 months and
valid for at least 2 more months. Examples: articles of organization or state registration
showing the name and address, and a domain registration or renewal invoice listing creste.dev
and the company. Validation takes **1 to 20 business days**, sometimes longer.

## 1. Azure subscription

1. Sign in at <https://portal.azure.com> with the account that will own this. Create a
   subscription if you don't have one (pay-as-you-go is enough).
2. Register the resource provider: **Subscriptions → your subscription → Resource providers →
   `Microsoft.CodeSigning` → Register**.

## 2. Artifact Signing account

1. Portal search: **Artifact Signing Accounts → Create**.
2. **Resource group:** create one, e.g. `rg-codesigning`.
3. **Account name:** 3–24 letters and digits, globally unique, starting with a letter, and not
   starting with "one". For example `cresteSigning`.
4. **Region:** e.g. **West US 2**. Note its endpoint; it becomes `ARTIFACT_SIGNING_ENDPOINT`:

   | Region | Endpoint |
   |---|---|
   | West US 2 | `https://wus2.codesigning.azure.net` |
   | East US | `https://eus.codesigning.azure.net` |
   | West Central US | `https://wcus.codesigning.azure.net` |

   The endpoint must match the account's region. A mismatch shows up as a *403 Forbidden* when
   signing. The full list is in Microsoft's quickstart.
5. **Pricing:** Basic is enough for three apps' releases.
6. Review, then create.

## 3. Roles for yourself

On the account, open **Access control (IAM) → Add role assignment** and give **your own user**:

- **Artifact Signing Identity Verifier**, which lets you create the identity validation. (You
  need at least Reader on the subscription as well.)
- Contributor or Owner, if you don't already have it, to create the certificate profile.

## 4. Identity validation (organization)

This can only be done in the portal.

1. The account → **Identity validations → Organization → New identity → Public**.
2. Fill in the form with the values from [step 0](#0-before-you-start). Check **Certificate
   subject preview**: it should read *CN=Creste LLC, O=Creste LLC*, plus the city, state and
   country.
3. **Create**. The status goes to *In Progress*.
4. When it changes to **Action Required**, open the link (it's also emailed to the primary
   address) and complete the representative's ID check. That's a photo ID and a selfie through
   Microsoft's verification partner, ending with a Verified ID in Microsoft Authenticator.
5. Answer any request for documents in the portal. You get three upload attempts.
6. Wait for **Completed**. If it ends as *Failed*, start a new request with corrected details.

## 5. Certificate profile

1. The account → **Certificate profiles → Create → Public Trust**.
2. **Name:** 5–100 letters and digits, e.g. `cresteRelease`. This becomes
   `ARTIFACT_SIGNING_PROFILE`.
3. **Verified CN and O:** pick the Creste LLC validation from step 4. Leave *Include street
   address* off unless you want the street on the certificate.
4. **Create**.

## 6. Let GitHub Actions sign (OIDC, no stored password)

The workflows log in with a short-lived GitHub token, so no Azure password or client secret is
stored anywhere.

1. **Microsoft Entra ID → App registrations → New registration.** Name it e.g.
   `github-mxb-app-signing`, single tenant, no redirect URI. Note its **Application (client) ID**
   and **Directory (tenant) ID**.
2. The app → **Certificates & secrets → Federated credentials → Add credential**:
   - **Scenario:** GitHub Actions deploying Azure resources
   - **Organization:** `Frostn1`, **Repository:** `mxb-app`
   - **Entity type:** **Environment**, **Environment name:** `code-signing`
   - The subject it shows must be `repo:Frostn1/mxb-app:environment:code-signing`.

   The release jobs run in that GitHub environment for exactly this reason: a tag-triggered job
   would otherwise present a subject naming the tag, which no credential can list in advance.
   GitHub creates the environment on the first run, and it has no protection rules.
3. Back on the Artifact Signing account → **Access control (IAM) → Add role assignment** →
   **Artifact Signing Certificate Profile Signer** → *Members: user, group or service principal*
   → select `github-mxb-app-signing`. For tighter scope, assign it on the certificate profile
   alone:

   ```
   az role assignment create --assignee <app's object id> \
     --role "Artifact Signing Certificate Profile Signer" \
     --scope "/subscriptions/<subscription id>/resourceGroups/rg-codesigning/providers/Microsoft.CodeSigning/codeSigningAccounts/<account>/certificateProfiles/<profile>"
   ```
4. Note your **Subscription ID** (Subscriptions → your subscription).

## 7. Add the GitHub secrets and variables

Everything goes on **one repo: `Frostn1/mxb-app`**. It builds MXB App, Frost Studio and MXB Coach.
The Coach workflow only publishes to `Frostn1/mxb-coach`, so that repo needs nothing.

**Settings → Secrets and variables → Actions:**

**Secrets** tab, **New repository secret**:

| Name | Value to copy |
|---|---|
| `AZURE_CLIENT_ID` | App registration → *Application (client) ID* (step 6.1) |
| `AZURE_TENANT_ID` | App registration → *Directory (tenant) ID* (step 6.1) |
| `AZURE_SUBSCRIPTION_ID` | Subscriptions → your subscription → *Subscription ID* (step 6.4) |

**Variables** tab, **New repository variable** (these aren't secret, so they stay readable in the
logs):

| Name | Value to copy |
|---|---|
| `ARTIFACT_SIGNING_ENDPOINT` | The account's region endpoint (step 2.4), e.g. `https://wus2.codesigning.azure.net` |
| `ARTIFACT_SIGNING_ACCOUNT` | The account name (step 2.3), e.g. `cresteSigning` |
| `ARTIFACT_SIGNING_PROFILE` | The certificate profile name (step 5.2), e.g. `cresteRelease` |

Secrets can also go on the `code-signing` environment instead of the repo. Either works.

## 8. Check it

1. Run **Release** (MXB App) from the Actions tab with *Run workflow* on a branch. That's a
   throwaway build, published as a pre-release.
2. In the Windows leg, the *Set up Windows code signing* step should print *Artifact Signing
   ready*, and the build log should show `sign-windows: signed …` lines for the exe,
   `mxbsecure.dll` and the installer.
3. Download the installer. Right-click it → **Properties → Digital Signatures** should show
   **Creste LLC**, with a timestamp from Microsoft. Do the same for the installed `MXB App.exe`.
4. Delete the test pre-release and its tag.

SmartScreen reputation is earned per publisher over time. The first signed releases can still
show a warning until enough users have installed them.

## Not covered here

- **FrostMod** (`Frostn1/frostmod`) builds `frostmod.dll`, `frostmod.exe` and `mxbcoach.dlo` in
  its own workflow. The same `scripts/sign-windows.ps1` and action can be copied there, using the
  same account and profile plus a second federated credential for that repo. It isn't wired up
  yet.
- The Linux and macOS bundles carry Windows PEs for Proton and Wine (`mxbsecure.dll`,
  `mxbsecure-inject.exe`). Artifact Signing's client runs on Windows only, so those copies stay
  unsigned.
