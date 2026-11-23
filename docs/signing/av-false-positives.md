# Antivirus false-positive reports: drafts

**Status: DRAFTS, not submitted.** Sean reviews them and submits them himself.

1. [Microsoft Defender](#1-microsoft-defender): the 0.18.4 installer, flagged as
   `Trojan:Win32/Ravartar!rfn`.
2. [Bitdefender](#2-bitdefender): `MXB App.exe`, flagged as `Trojan.RTF.Agent.GY` or behaviourally.

Both describe the same app. Fill in the `⟨…⟩` placeholders; everything else is ready to paste.

---

## 1. Microsoft Defender

**Where:** <https://www.microsoft.com/wdsi/filesubmission>
- Sign in with a Microsoft account. Pick **Software developer** as the submitter type. That
  queues it with more weight, and Microsoft replies with a submission ID to track.
- **Product:** *Microsoft Defender Antivirus (Windows 10/11)*.
- **Select the file:** `MXB-App-0.18.4-x64.exe`, the installer exactly as downloaded from the
  release. Zip it if the form asks for an archive.
- **Should this file be detected as malware?** *No, it's a false positive (incorrectly detected).*
- **Detection name:** `Trojan:Win32/Ravartar!rfn`
- **Definition version:** ⟨Windows Security → Virus & threat protection → *Protection updates* → Security intelligence version⟩

**Additional information** (paste):

> MXB App is a free mod manager for the PC racing game MX Bikes (PiBoSo), published by Creste LLC
> (https://www.creste.dev). Defender detected the installer from our official GitHub release as
> Trojan:Win32/Ravartar!rfn (ThreatID 2147965211) on 2026-09-23. Being an "!rfn" verdict, this
> looks like a cloud or machine-learning classification rather than a signature match. Users
> downloading it from GitHub are seeing the download blocked and the file removed.
>
> - File: MXB-App-0.18.4-x64.exe (NSIS installer built by Tauri; per-user install, no admin
>   rights)
> - Version: 0.18.4
> - SHA-256: eb46a92bf8785e536540dc4837ddd952c13b7f8d2bfee5e8ba6f6dc7bc412ba1
> - Size: 9,030,907 bytes
> - Official download: https://github.com/Frostn1/mxb-app/releases/tag/v0.18.4
> - Source code (public): https://github.com/Frostn1/mxb-app
>
> The installer puts MXB App in %LOCALAPPDATA%\MXB App. The app downloads community mods from
> mxb-mods.com and mxbikes-shop.com into the game's mods folder, launches the game, and updates
> itself from its GitHub releases through Tauri's signed updater. An optional feature, off by
> default and only turned on with the user's consent, loads a helper DLL (mxbsecure.dll) into
> the running MX Bikes process so the game can open creators' protected paid mods. It targets
> only that game process.
>
> We're moving our Windows releases to Authenticode signing as Creste LLC through Azure
> Artifact Signing, and can send signed builds for re-analysis. Please review and clear this
> detection. Contact: ⟨name⟩, ⟨email@creste.dev⟩.

Before sending:
- [ ] The file is the exact one from the release page. Its SHA-256 must match the value above:
      `Get-FileHash .\MXB-App-0.18.4-x64.exe`.
- [ ] If newer releases are flagged too, submit each of those installers separately with its
      own hash.

---

## 2. Bitdefender

Where to submit:
- Consumer form: <https://www.bitdefender.com/consumer/support/answer/29358/> (Report a false
  positive), or the sample uploader it links to.
- Attach the flagged file itself, zipped with the password `infected`, which is the usual
  convention for AV labs.

Fill in the `⟨…⟩` placeholders from the machine where Bitdefender flagged the file. The rest is
ready to paste.

---

**Subject:** False positive: "Trojan.RTF.Agent.GY" / behavioural detection on MXB App.exe (MXB App, a mod manager for the game MX Bikes)

Hello Bitdefender Labs,

We're reporting a false positive on our application **MXB App**. Bitdefender detects its main
executable, **`MXB App.exe`**, as **Trojan.RTF.Agent.GY** and also blocks it behaviourally. Our
users are hitting this after a normal install.

**The file**

| | |
|---|---|
| Product | MXB App, a free mod manager for the PC racing game *MX Bikes* (PiBoSo) |
| Publisher | Creste LLC, https://www.creste.dev |
| File name | `MXB App.exe` |
| Installed at | `%LOCALAPPDATA%\MXB App\MXB App.exe` (per-user install, no admin rights) |
| Version | ⟨version from the flagged machine, e.g. 0.18.4⟩ |
| SHA-256 of the flagged file | ⟨`Get-FileHash "$env:LOCALAPPDATA\MXB App\MXB App.exe"`⟩ |
| Detection name | Trojan.RTF.Agent.GY ⟨and/or the exact behavioural detection name shown⟩ |
| Bitdefender product and version | ⟨e.g. Bitdefender Total Security, engine/signature version⟩ |
| Installer it came from | `MXB-App-0.18.4-x64.exe`, SHA-256 `eb46a92bf8785e536540dc4837ddd952c13b7f8d2bfee5e8ba6f6dc7bc412ba1` |
| Official download | https://github.com/Frostn1/mxb-app/releases/tag/v0.18.4 |
| Source code | https://github.com/Frostn1/mxb-app (public) |

**What the program does**

MXB App is a desktop app (Tauri: a Rust backend with a WebView2 front end) that players use to:

- browse, download and install community mods (tracks, bikes, rider gear) from the public mod
  sites mxb-mods.com and mxbikes-shop.com into the game's own `mods` folder;
- manage setups, liveries and ReShade presets, launch the game, and browse multiplayer servers;
- update itself from its GitHub releases (signed Tauri updater).

Some of what it does can look suspicious to a behavioural engine, but all of it is expected
for a game mod manager:

- It **downloads and extracts mod archives** into the game's mods folder.
- It **starts the game** (`mxbikes.exe`) and watches whether it is running.
- For the optional *secure content* feature, which is off by default and needs the user's
  consent, it **loads a helper DLL (`mxbsecure.dll`) into the running game process**. That DLL
  lets the game open paid mods that creators have protected. It only runs when the user turns
  the feature on, and it only targets the MX Bikes process.
- It keeps its own settings and caches in `%LOCALAPPDATA%\com.frost.mxbikes`.

It contains no RTF or Office-document handling, which makes an RTF-family signature on this PE
look like a mismatch. The only way it starts itself is its "launch at startup" setting, which the
user can switch off in the app. It doesn't collect credentials, and its network traffic is to the
mod sites, GitHub (for updates), Steam and our own services.

**What we're asking**

Please re-analyse the attached sample and remove the detection. If the behavioural detection is
triggered by the optional in-process DLL loading, we're glad to provide more detail or
builds for analysis.

Upcoming releases will be Authenticode-signed by **Creste LLC** (Azure Artifact Signing). We can
send a signed build as soon as one is available, if that helps whitelisting.

Contact: ⟨name⟩, ⟨email@creste.dev⟩

Thank you,
⟨name⟩
Creste LLC

---

## Before sending, check

- [ ] The detection name matches exactly what Bitdefender showed, from its quarantine or event log.
- [ ] The SHA-256 is of the exact file that was flagged, which may be a newer version than 0.18.4.
- [ ] The zip holds only the flagged file.
- [ ] The contact email is on @creste.dev.
