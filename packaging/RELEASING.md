# Releasing

1. **Version.** Set `version` in the workspace `Cargo.toml`, run `cargo build` (updates
   `Cargo.lock`), mark the release in `ROADMAP.md`, commit (`chore: release X.Y.Z`).
2. **Signed tag.** `git tag -s vX.Y.Z -m "Omaspec X.Y.Z"` with the maintainer's GPG or
   SSH signing key (`git config user.signingkey …`; for SSH also
   `git config gpg.format ssh`). Put the public key on the GitHub account so the tag
   shows as verified, and check it: `git tag -v vX.Y.Z`.
3. **Push.** `git push origin main vX.Y.Z`. The release workflow
   (`.github/workflows/release.yml`) tests and builds for x86_64 (on Arch) and aarch64
   (on Ubuntu) and attaches `omaspec-X.Y.Z-x86_64-linux.tar.gz` and
   `omaspec-X.Y.Z-aarch64-linux.tar.gz` with their checksums to the GitHub release. When
   one architecture fails, the release still gets the other's archive.
   The binary is linked against glibc 2.35 (`cargo zigbuild`), so it runs outside Arch
   too: the workflow fails when it asks for a newer one, and runs each archive on
   Ubuntu 22.04 and Debian 12 before it attaches it. To try the workflow without a
   release: `gh workflow run release.yml -f tag=main -f publish=false`.
   If the workflow fails, fix it on `main` and build the tag again with it:
   `gh workflow run release.yml -f tag=vX.Y.Z`. The tag stays where it is.
4. **AUR.** For each of `packaging/aur/omaspec`, `omaspec-bin` (and once
   `omaspec-git`): set `pkgver`, then
   ```bash
   updpkgsums                          # real checksums instead of SKIP
   makepkg -si                         # builds and installs: try it
   makepkg --printsrcinfo > .SRCINFO
   ```
   and push `PKGBUILD` and `.SRCINFO` to `ssh://aur@aur.archlinux.org/<pkgname>.git`.
   The AUR shows each commit's address to everyone: in a fresh clone, set
   `git config user.email` to the public one before the first commit.
5. **Announce.** `docs/announcement.md` is a draft; adjust it to the release.
