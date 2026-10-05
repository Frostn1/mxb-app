# mxb-agent has moved

mxb-agent, the helper that runs next to an MX Bikes dedicated server, is server code, and server
code is private. It now lives in the private server repository (`apps/agent`), together with
MXB Servers, which installs and pairs it.

Signed builds are published on [Frostn1/mxbserver-releases](https://github.com/Frostn1/mxbserver-releases/releases)
under `agent-v*` tags: a Windows x64 exe, a zip with `install.ps1`, a Linux x86_64 binary and
`SHA256SUMS`. The pairing format (`mxb-agent:<base64 of {"url","token"}>`) is unchanged.
