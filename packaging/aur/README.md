# AUR packages

- `omagram-git`: builds the newest commit.
- `omagram`: builds a release from its source archive.
- `omagram-bin`: installs the binary the release workflow built.

All three install the binary, the desktop entry, the icon, the file types for `.puml` and
`.mmd`, and the Omarchy templates under `/usr/share/omagram/omarchy`. Releasing and
submitting: [../RELEASING.md](../RELEASING.md).

To try `omagram-git` against your local checkout (it builds the last commit):

```bash
cd "$(mktemp -d)"
checkout=/path/to/omagram      # your clone
sed "s|^source=.*|source=(\"omagram::git+file://$checkout\")|" \
  "$checkout/packaging/aur/omagram-git/PKGBUILD" > PKGBUILD
makepkg -s       # then: sudo pacman -U omagram-git-*.pkg.tar.zst
```
