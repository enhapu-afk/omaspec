# AUR packages

- `omaspec-git`: builds the newest commit.
- `omaspec`: builds a release from its source archive.
- `omaspec-bin`: installs the binary the release workflow built.

All three install the binary, the desktop entry, the icon, the file types for `.puml` and
`.mmd`, and the Omarchy templates under `/usr/share/omaspec/omarchy`. Releasing and
submitting: [../RELEASING.md](../RELEASING.md).

To try `omaspec-git` against your local checkout (it builds the last commit):

```bash
cd "$(mktemp -d)"
checkout=/path/to/omaspec      # your clone
sed "s|^source=.*|source=(\"omaspec::git+file://$checkout\")|" \
  "$checkout/packaging/aur/omaspec-git/PKGBUILD" > PKGBUILD
makepkg -s       # then: sudo pacman -U omaspec-git-*.pkg.tar.zst
```
