#!/usr/bin/env python3
"""Builds docs/guide/omaspec-guide.pdf from guide.html beside this file.

The guide's examples are real: each `<ex>` is drawn by the `omaspec` binary
given (the release build by default), each `<cmd>` is a picture one of its
commands writes, and each `<term>` is what a command prints. Chromium
prints the page as a PDF. Run it from anywhere:

    docs/guide/pdf/build.py [--omaspec PATH] [--keep DIR]

Needs Chromium and a built `omaspec` (cargo build --release -p omaspec-app).
"""

import argparse
import html
import pathlib
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ATTR = re.compile(r'([\w-]+)="([^"]*)"')
# A tag of the guide: its attributes may hold a `>` in their quotes.
TAG = r'<%s\s+((?:[\w\s=-]|"[^"]*")*)/>'


def attrs(text):
    found = dict(ATTR.findall(text))
    # A word alone (`nosrc`) is a switch.
    for word in ATTR.sub("", text).split():
        found[word] = ""
    return found


class Guide:
    def __init__(self, omaspec, work):
        self.omaspec = omaspec
        self.work = work
        self.count = 0

    def run(self, args, check=True):
        done = subprocess.run(
            [self.omaspec, *args], cwd=ROOT, capture_output=True, text=True
        )
        if check and done.returncode != 0:
            sys.exit(f"omaspec {' '.join(args)}:\n{done.stderr}")
        return done

    def picture(self, args):
        """The SVG a command writes, as a file of the page."""
        self.count += 1
        name = f"fig-{self.count:03}.svg"
        # A command that finds something exits with 1 and still draws.
        done = self.run([*args, "-o", str(self.work / name)], check=False)
        if not (self.work / name).exists():
            sys.exit(f"omaspec {' '.join(args)} drew nothing:\n{done.stderr}")
        return name

    def source(self, path, lines):
        text = (ROOT / path).read_text()
        rows = text.rstrip("\n").split("\n")
        if lines:
            first, last = (int(n) for n in lines.split("-"))
            cut = rows[first - 1 : last]
            if last < len(rows):
                cut.append("…")
            rows = cut
        return "\n".join(rows)

    def ex(self, match):
        a = attrs(match.group(1))
        path = a["src"]
        look = a.get("look", "light")
        name = self.picture(["render", path, "--look", look])
        figure = f'<img src="{name}" alt="">'
        caption = a.get("caption", "")
        label = f'<span class="file">{html.escape(pathlib.Path(path).name)}</span>'
        cap = f"<figcaption>{caption}</figcaption>" if caption else ""
        layout = a.get("layout", "side")
        style = f' style="--pic: {a["pic"]}"' if "pic" in a else ""
        if "nosrc" in a:
            return f'<figure class="ex only"{style}><div class="pic">{figure}</div>{cap}</figure>'
        code = html.escape(self.source(path, a.get("lines")))
        return (
            f'<figure class="ex {layout}"{style}>'
            f'<div class="src">{label}<pre>{code}</pre></div>'
            f'<div class="pic">{figure}</div>{cap}</figure>'
        )

    def cmd(self, match):
        a = attrs(match.group(1))
        name = self.picture(shlex.split(a["run"]))
        caption = a.get("caption", "")
        shown = html.escape(a.get("show", "omaspec " + a["run"]))
        cap = f"<figcaption>{caption}</figcaption>" if caption else ""
        style = f' style="--pic: {a["pic"]}"' if "pic" in a else ""
        return (
            f'<figure class="ex stack"{style}><div class="term"><pre>'
            f'<span class="p">$</span> {shown}</pre></div>'
            f'<div class="pic">{figure_of(name)}</div>{cap}</figure>'
        )

    def term(self, match):
        a = attrs(match.group(1))
        done = self.run(shlex.split(a["run"]), check=False)
        out = (done.stdout + done.stderr).rstrip("\n")
        rows = out.split("\n")
        most = int(a.get("rows", "40"))
        if len(rows) > most:
            rows = rows[:most] + ["…"]
        out = "\n".join(rows)
        # As in the folder the reader would stand in.
        for folder in (str(ROOT) + "/", "docs/guide/pdf/examples/shop/", "docs/guide/pdf/examples/", "examples/orders/"):
            out = out.replace(folder, "")
        for was, shown_as in (("door-findings.puml", "door.puml"), ("door-first.puml", "old.puml"), ("door-guards.puml", "door.puml"), ("untidy.puml", "door.puml"), ("portable.mmd", "flow.mmd"), ("document.md", "README.md")):
            out = out.replace(was, shown_as)
        shown = html.escape(a.get("show", "omaspec " + a["run"]))
        return (
            f'<div class="term"><pre><span class="p">$</span> <b>{shown}</b>\n'
            f"{html.escape(out)}</pre></div>"
        )

    def code(self, match):
        a = attrs(match.group(1))
        label = f'<span class="file">{html.escape(a["name"])}</span>' if "name" in a else ""
        text = html.escape(self.source(a["src"], a.get("lines")))
        return f'<div class="src alone">{label}<pre>{text}</pre></div>'


def figure_of(name):
    return f'<img src="{name}" alt="">'


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--omaspec", default=str(ROOT / "target/release/omaspec"))
    parser.add_argument("--keep", help="keep the page and its pictures in this folder")
    parser.add_argument("-o", "--output", default=str(HERE.parent / "omaspec-guide.pdf"))
    args = parser.parse_args()
    if not pathlib.Path(args.omaspec).exists():
        sys.exit(f"{args.omaspec} is not built: cargo build --release -p omaspec-app")
    chromium = shutil.which("chromium") or shutil.which("google-chrome-stable")
    if not chromium:
        sys.exit("Chromium prints the guide, and is not installed")

    work = pathlib.Path(args.keep or tempfile.mkdtemp(prefix="omaspec-guide-"))
    work.mkdir(parents=True, exist_ok=True)
    guide = Guide(args.omaspec, work)
    page = (HERE / "guide.html").read_text()
    version = guide.run(["--version"]).stdout.split()[1].split("+")[0]
    page = page.replace("{{version}}", version)
    page = page.replace("{{root}}", ROOT.as_uri())
    page = re.sub(TAG % "ex", guide.ex, page)
    page = re.sub(TAG % "cmd", guide.cmd, page)
    page = re.sub(TAG % "term", guide.term, page)
    page = re.sub(TAG % "code-file", guide.code, page)
    (work / "guide.html").write_text(page)

    subprocess.run(
        [
            chromium,
            "--headless",
            "--disable-gpu",
            "--no-pdf-header-footer",
            "--allow-file-access-from-files",
            f"--print-to-pdf={args.output}",
            (work / "guide.html").as_uri(),
        ],
        check=True,
        capture_output=True,
    )
    print(f"{args.output}: {guide.count} pictures drawn by omaspec {version}")
    if not args.keep:
        shutil.rmtree(work)


if __name__ == "__main__":
    main()
