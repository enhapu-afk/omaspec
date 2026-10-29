# Releasing

1. **Version.** Set `version` in the workspace `Cargo.toml`, run `cargo build` (updates
   `Cargo.lock`), mark the release in `ROADMAP.md`, commit (`chore: release X.Y.Z`).
2. **Signed tag.** `git tag -s vX.Y.Z -m "Omaspec X.Y.Z"` with the maintainer's GPG or
   SSH signing key (`git config user.signingkey …`; for SSH also
   `git config gpg.format ssh`). Put the public key on the GitHub account so the tag
   shows as verified, and check it: `git tag -v vX.Y.Z`.
3. **Push.** `git push origin main vX.Y.Z`. The release workflow
   (`.github/workflows/release.yml`) tests, builds on Arch and attaches
   `omaspec-X.Y.Z-x86_64-linux.tar.gz` with its checksum to the GitHub release.
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
5. **Announce.** `docs/announcement.md` is a draft; adjust it to the release.
