#!/usr/bin/env python3
"""publish-status — is any PDF-family crate worth publishing to crates.io?

For each crate in the PDF publishing family it reports, at a glance:

  * the local (working-tree) version,
  * the version currently on crates.io,
  * how many commits have touched the crate's `src/` since that published
    version, and
  * a verdict: up-to-date / publish-worthy / ahead-but-no-src-change / unknown.

This is the "WHEN should I publish?" companion to the cargo-release procedure
in medpdf/PUBLISHING.md (which is the "HOW").  It never publishes anything and
never touches git state — it only reads.

Version comparison is SEMVER (Cargo package versions), where numeric ordering
is correct and 1.10.0 > 1.9.0.  This is deliberately NOT the artifact-filename
version scheme (~/.claude/rules/file-version-scheme.md); package versions and
artifact-filename versions are different schemes with different comparators, so
keep them in separate code paths.

Exit codes (canonical portfolio table, ~/.claude/rules/cli-exit-codes.md):

  0  every crate is confirmed up-to-date with crates.io
  1  tool error — a manifest could not be read, or crates.io could not be
     reached for a crate whose status is therefore unknown and NOT clean
     (an unreachable registry is reported as Unknown, never as "up-to-date":
     ~/.claude/rules/positive-evidence-of-absence.md)
  3  findings — at least one crate is ahead of crates.io and worth publishing

`--json` emits the same data as a machine-readable object and still exits 3 on
findings (findings are data, not just a signal).
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

CRATES_IO_API = "https://crates.io/api/v1/crates/{name}"
USER_AGENT = "pdf-family-publish-status (github.com/ComposerChrisF)"

# The PDF publishing family, in bottom-up dependency order.  medpdf and
# medpdf-image live in the medpdf workspace; pdf-maker and pdf-dump are their
# own repos, siblings of medpdf under the Pdf/ container.  font-dump and
# pdf-orchestrator are intentionally NOT here — add a row if they ever join the
# crates.io set.
SCRIPT_DIR = Path(__file__).resolve().parent
MEDPDF_REPO = SCRIPT_DIR.parent            # .../Pdf/medpdf
PDF_ROOT = MEDPDF_REPO.parent              # .../Pdf


@dataclass(frozen=True)
class Crate:
    name: str
    manifest: Path       # the crate's Cargo.toml
    repo: Path           # the git repo that contains it
    src: str             # src path, relative to `repo`, used for commit counting


CRATES: list[Crate] = [
    Crate("medpdf", MEDPDF_REPO / "medpdf" / "Cargo.toml", MEDPDF_REPO, "medpdf/src"),
    Crate("medpdf-image", MEDPDF_REPO / "medpdf-image" / "Cargo.toml", MEDPDF_REPO, "medpdf-image/src"),
    Crate("pdf-maker", PDF_ROOT / "pdf-maker" / "Cargo.toml", PDF_ROOT / "pdf-maker", "src"),
    Crate("pdf-dump", PDF_ROOT / "pdf-dump" / "Cargo.toml", PDF_ROOT / "pdf-dump", "src"),
]


@dataclass
class Result:
    name: str
    local: str | None = None
    registry: str | None = None
    commits_since: int | None = None
    commits_basis: str | None = None       # "tag" | "manifest-pickaxe" | None
    verdict: str = "unknown"               # up-to-date | publish-worthy | ahead-no-src | behind | unknown
    note: str = ""
    errors: list[str] = field(default_factory=list)


def read_local_version(manifest: Path) -> str:
    with manifest.open("rb") as fh:
        data = tomllib.load(fh)
    return data["package"]["version"]


def fetch_registry_version(name: str) -> str | None:
    """Return the crate's max published version, or None if it has never been
    published.  Raises on a genuine fetch failure so the caller can record it as
    Unknown (never silently as 'absent' — positive-evidence-of-absence)."""
    req = urllib.request.Request(
        CRATES_IO_API.format(name=name), headers={"User-Agent": USER_AGENT}
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            payload = json.load(resp)
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return None  # provably never published — this IS positive evidence
        raise
    return payload["crate"]["max_version"]


def semver_tuple(version: str) -> tuple[int, ...]:
    """Numeric semver ordering.  Pre-release suffixes are dropped for the
    comparison (good enough for this family, which does not use them)."""
    core = version.split("-", 1)[0].split("+", 1)[0]
    return tuple(int(part) for part in core.split("."))


def git(repo: Path, *args: str) -> str:
    out = subprocess.run(
        ["git", "-C", str(repo), *args],
        capture_output=True,
        text=True,
    )
    return out.stdout.strip() if out.returncode == 0 else ""


def commits_since_publish(crate: Crate, registry_version: str) -> tuple[int | None, str | None]:
    """Count commits touching the crate's src/ since its published version.

    Prefer a release tag `<crate>-v<version>` (precise).  Absent a tag, fall
    back to the manifest pickaxe: the last commit that set the version line to
    the published value.  Returns (count, basis) or (None, None) if no anchor
    can be found (reported honestly rather than guessed)."""
    tag = f"{crate.name}-v{registry_version}"
    if git(crate.repo, "tag", "-l", tag) == tag:
        base, basis = tag, "tag"
    else:
        rel_manifest = crate.manifest.relative_to(crate.repo).as_posix()
        base = git(
            crate.repo, "log", "-1", "--format=%H",
            "-S", f'version = "{registry_version}"', "--", rel_manifest,
        )
        basis = "manifest-pickaxe"
    if not base:
        return None, None
    count = git(crate.repo, "rev-list", "--count", f"{base}..HEAD", "--", crate.src)
    return (int(count) if count.isdigit() else None), basis


def evaluate(crate: Crate) -> Result:
    r = Result(name=crate.name)

    try:
        r.local = read_local_version(crate.manifest)
    except (OSError, KeyError, tomllib.TOMLDecodeError) as exc:
        r.errors.append(f"cannot read local version: {exc}")
        return r  # verdict stays "unknown"

    try:
        r.registry = fetch_registry_version(crate.name)
    except (urllib.error.URLError, TimeoutError, OSError, json.JSONDecodeError) as exc:
        r.errors.append(f"crates.io unreachable: {exc}")
        r.note = "registry unknown"
        return r  # verdict "unknown" — NOT up-to-date

    if r.registry is None:
        r.verdict = "publish-worthy"
        r.note = "never published to crates.io"
        return r

    if semver_tuple(r.local) < semver_tuple(r.registry):
        r.verdict = "behind"
        r.note = "local is OLDER than crates.io (unexpected)"
        return r

    if semver_tuple(r.local) == semver_tuple(r.registry):
        r.verdict = "up-to-date"
        return r

    # local is ahead of the registry
    r.commits_since, r.commits_basis = commits_since_publish(crate, r.registry)
    if r.commits_since is None:
        r.verdict = "publish-worthy"
        r.note = "ahead; commit count unavailable (no tag/anchor)"
    elif r.commits_since > 0:
        r.verdict = "publish-worthy"
    else:
        r.verdict = "ahead-no-src"
        r.note = "version ahead but no src/ commits since last publish"
    return r


VERDICT_LABEL = {
    "up-to-date": "up-to-date",
    "publish-worthy": "PUBLISH-WORTHY",
    "ahead-no-src": "ahead (no src change)",
    "behind": "BEHIND crates.io",
    "unknown": "UNKNOWN",
}


def print_table(results: list[Result]) -> None:
    name_w = max(len("crate"), *(len(r.name) for r in results))
    header = f"  {'crate':<{name_w}}  {'local':>9}  {'crates.io':>9}  {'src commits':>11}  verdict"
    print(header)
    print("  " + "-" * (len(header) - 2))
    for r in results:
        local = r.local or "?"
        registry = r.registry if r.registry is not None else ("none" if not r.errors else "?")
        if r.commits_since is None:
            commits = "-" if r.verdict in ("up-to-date", "behind") else "?"
        else:
            basis = "~" if r.commits_basis == "manifest-pickaxe" else ""
            commits = f"{basis}{r.commits_since}"
        line = f"  {r.name:<{name_w}}  {local:>9}  {registry:>9}  {commits:>11}  {VERDICT_LABEL[r.verdict]}"
        print(line)
        if r.note:
            print(f"  {'':<{name_w}}  {'':>9}  {'':>9}  {'':>11}  ({r.note})")
        for err in r.errors:
            print(f"  {'':<{name_w}}  {'':>9}  {'':>9}  {'':>11}  !! {err}")
    print()
    print("  ~ = commit count derived from the manifest (no release tag yet); tag it on first publish for a precise count.")


def to_json(results: list[Result]) -> str:
    return json.dumps(
        [
            {
                "crate": r.name,
                "local": r.local,
                "registry": r.registry,
                "commits_since_publish": r.commits_since,
                "commits_basis": r.commits_basis,
                "verdict": r.verdict,
                "note": r.note or None,
                "errors": r.errors,
            }
            for r in results
        ],
        indent=2,
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        prog="publish-status",
        description="Report which PDF-family crates are worth publishing to crates.io.",
        epilog=(
            "exit codes: 0 all up-to-date; 1 tool error / registry unreachable "
            "(status could not be confirmed clean); 3 findings (a crate is worth publishing)."
        ),
    )
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    args = parser.parse_args()

    results = [evaluate(crate) for crate in CRATES]

    if args.json:
        print(to_json(results))
    else:
        print_table(results)

    if any(r.verdict == "publish-worthy" for r in results):
        return 3
    if any(r.errors or r.verdict in ("unknown", "behind") for r in results):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
