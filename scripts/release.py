#!/usr/bin/env python3
"""
Release script for brochure.

Usage:
  python scripts/release.py --version 0.2.2
  python scripts/release.py --bump patch
  python scripts/release.py --bump minor
  python scripts/release.py --bump major
  python scripts/release.py --bump auto   # detects from next-update.md content

Does:
  1. Computes new version (specified or bumped from Cargo.toml)
   2. Reads changelog/next-update.md
   3. Calls OpenCode Go deepseek-v4-pro to generate a human-readable summary
  4. Updates version in Cargo.toml
  5. Renames changelog/next-update.md to changelog/vX.Y.Z.md
  6. Creates a fresh changelog/next-update.md template
  7. Appends an entry to changelog.json

Output (stdout):
  {"version": "0.2.2", "summary": "...", "highlights": [...]}  # success
  {"error": "..."}                                              # failure
"""

import argparse
import json
import os
import re
import sys
import subprocess
import urllib.error
import urllib.request
from datetime import date

GO_BASE = "https://opencode.ai/zen/go/v1"
GO_API_KEY = os.environ.get("OPENCODE_API_KEY", "")
GO_HEADERS = {
    "Authorization": f"Bearer {GO_API_KEY}",
    "User-Agent": "release-agent/1.0",
}
DEFAULT_MODEL = "deepseek-v4-pro"

NEXT_UPDATE_TEMPLATE = """# Next Update

## Features

## Fixes

## Refactor
"""

SUMMARY_SYSTEM = (
    "You are a release notes writer. Given raw developer changelog entries, "
    "produce a brief, user-friendly release summary.\n"
    "Rules:\n"
    "- Return ONLY valid JSON, no markdown fences, no explanation.\n"
    "- Format: {\"summary\": \"one sentence overview\", \"highlights\": [\"bullet1\", ...]}\n"
    "- 3–5 highlights max. Combine trivial fixes into 'Minor bug fixes and UI improvements'.\n"
    "- Focus on user-visible changes. Omit internal refactors unless significant.\n"
    "- Keep each highlight under 10 words."
)


# ---------------------------------------------------------------------------
# Cargo.toml helpers
# ---------------------------------------------------------------------------

def read_cargo_version(cargo_path: str) -> str:
    with open(cargo_path) as f:
        content = f.read()
    m = re.search(r'(?m)^version\s*=\s*"([^"]+)"', content)
    if not m:
        raise ValueError("version field not found in Cargo.toml")
    return m.group(1)


def write_cargo_version(cargo_path: str, new_version: str) -> None:
    with open(cargo_path) as f:
        content = f.read()
    updated = re.sub(
        r'(?m)^version\s*=\s*"[^"]*"',
        f'version = "{new_version}"',
        content,
        count=1,
    )
    with open(cargo_path, "w") as f:
        f.write(updated)


def write_cargo_lock_version(cargo_lock_path: str, new_version: str) -> None:
    with open(cargo_lock_path) as f:
        content = f.read()
    updated = re.sub(
        r'(?m)^(name = "brochure"\n)version = "[^"]*"',
        rf'\1version = "{new_version}"',
        content,
    )
    with open(cargo_lock_path, "w") as f:
        f.write(updated)


# ---------------------------------------------------------------------------
# Version arithmetic
# ---------------------------------------------------------------------------

def parse_version(v: str) -> tuple[int, int, int]:
    parts = v.split(".")
    if len(parts) != 3:
        raise ValueError(f"expected semver X.Y.Z, got: {v}")
    return int(parts[0]), int(parts[1]), int(parts[2])


def bump_version(current: str, bump: str) -> str:
    """Bump current version by the given component (patch/minor/major)."""
    major, minor, patch = parse_version(current)
    if bump == "patch":
        return f"{major}.{minor}.{patch + 1}"
    elif bump == "minor":
        return f"{major}.{minor + 1}.0"
    elif bump == "major":
        return f"{major + 1}.0.0"
    else:
        raise ValueError(f"unknown bump type: {bump!r} (expected patch/minor/major)")


def detect_bump_type(changelog_content: str) -> str:
    """
    Return 'minor' if next-update.md contains any feature entries,
    'patch' otherwise.
    """
    in_features = False
    for line in changelog_content.splitlines():
        stripped = line.strip()
        if stripped.startswith("## Features"):
            in_features = True
            continue
        if stripped.startswith("## ") and in_features:
            break
        if in_features and stripped and not stripped.startswith("#"):
            return "minor"
    return "patch"


# ---------------------------------------------------------------------------
# Changelog helpers
# ---------------------------------------------------------------------------

def read_next_update(changelog_dir: str) -> str:
    path = os.path.join(changelog_dir, "next-update.md")
    with open(path) as f:
        return f.read()


def rename_next_update(changelog_dir: str, version: str) -> None:
    src = os.path.join(changelog_dir, "next-update.md")
    dst = os.path.join(changelog_dir, f"v{version}.md")
    os.rename(src, dst)


def create_next_update_template(changelog_dir: str) -> None:
    path = os.path.join(changelog_dir, "next-update.md")
    with open(path, "w") as f:
        f.write(NEXT_UPDATE_TEMPLATE)


def load_changelog_json(json_path: str) -> list:
    if not os.path.exists(json_path):
        return []
    with open(json_path) as f:
        return json.load(f)


def save_changelog_json(json_path: str, entries: list) -> None:
    with open(json_path, "w") as f:
        json.dump(entries, f, indent=2)
        f.write("\n")

# ---------------------------------------------------------------------------
# Commit & Publish helpers
# ---------------------------------------------------------------------------

def run_cmd(cmd: list[str], cwd: str | None = None, dry_run: bool = False) -> subprocess.CompletedProcess:
    prefix = "[dry-run]" if dry_run else "[run]"
    print(f"{prefix} $ {' '.join(cmd)}")
    if dry_run:
        # Helpful fake outputs for common commands
        key = " ".join(cmd)
        fake_map = {
            "git status --porcelain": b"",
            "git diff --staged": b"[dry-run] staged diff: (no real changes shown)\n",
            "git commit -m": b"[dry-run] would create commit: (chore: bumped version)\n",
            "git log -1 --stat": b"[dry-run] last commit (not actually created)\n",
            "git tag -a": b"[dry-run] would create annotated tag\n",
            "git push": b"[dry-run] would push commits\n",
            "cargo publish --dry-run": b"[dry-run] cargo publish --dry-run\n",
        }
        # try to match prefix keys
        for k, v in fake_map.items():
            if key.startswith(k):
                return subprocess.CompletedProcess(cmd, 0, stdout=v, stderr=b"")
        return subprocess.CompletedProcess(cmd, 0, stdout=b"[dry-run] no output\n", stderr=b"")
    proc = subprocess.run(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if proc.stdout:
        try:
            print(proc.stdout.decode(), end="")
        except Exception:
            print(proc.stdout)
    if proc.stderr:
        try:
            print(proc.stderr.decode(), end="", file=sys.stderr)
        except Exception:
            print(proc.stderr, file=sys.stderr)
    return proc

def local_tag_exists(project_root: str, tag: str) -> bool:
    proc = subprocess.run(
        ["git", "tag", "-l", tag],
        cwd=project_root, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    return bool(proc.stdout.strip())


def git_commit_and_tag(project_root: str, new_version: str, dry_run: bool) -> None:
    msg = f"chore: bumped version to v{new_version}"
    tag = f"v{new_version}"

    # Add changed files
    run_cmd(["git", "add", "Cargo.toml", "Cargo.lock", "changelog.json"], cwd=project_root, dry_run=dry_run)

    # Commit
    commit_proc = run_cmd(["git", "commit", "-m", msg], cwd=project_root, dry_run=dry_run)
    if commit_proc.returncode != 0 and not dry_run:
        if commit_proc.stderr:
            print(commit_proc.stderr.decode(), file=sys.stderr)
        raise RuntimeError("git commit failed")

    # Tag — skip creation if it already exists locally
    if not dry_run and local_tag_exists(project_root, tag):
        print(f"[info] tag {tag} already exists locally — skipping tag creation")
    else:
        tag_proc = run_cmd(["git", "tag", "-a", tag, "-m", msg], cwd=project_root, dry_run=dry_run)
        if tag_proc.returncode != 0 and not dry_run:
            if tag_proc.stderr:
                print(tag_proc.stderr.decode(), file=sys.stderr)
            raise RuntimeError("git tag failed")


def git_push(project_root: str, version: str, dry_run: bool) -> None:
    # --follow-tags pushes the branch and any reachable annotated tags in one shot.
    run_cmd(["git", "push", "--follow-tags"], cwd=project_root, dry_run=dry_run)

def create_github_release(project_root: str, version: str, summary_data: dict, dry_run: bool) -> None:
    highlights = summary_data.get("highlights", [])
    summary = summary_data.get("summary", "")
    lines = [summary, "", "## Highlights"]
    lines += [f"- {h}" for h in highlights]
    notes = "\n".join(lines)
    run_cmd(
        ["gh", "release", "create", f"v{version}",
         "--title", f"v{version}",
         "--notes", notes,
         "--repo", "Sylviromi/brochure"],
        cwd=project_root,
        dry_run=dry_run,
    )

def run_cargo_publish(project_root: str, dry_run: bool) -> None:
    # cargo publish supports --dry-run
    cmd = ["cargo", "publish"]
    if dry_run:
        cmd.append("--dry-run")
    run_cmd(cmd, dry_run=dry_run)

# ---------------------------------------------------------------------------
# OpenCode Go
# ---------------------------------------------------------------------------

def check_go_api() -> bool:
    try:
        req = urllib.request.Request(
            f"{GO_BASE}/models",
            headers=GO_HEADERS,
        )
        urllib.request.urlopen(req, timeout=5)
        return True
    except Exception:
        return False


def call_go_summary(model: str, changelog_content: str) -> dict:
    """
    Call OpenCode Go to summarize changelog_content.
    Returns {"summary": str, "highlights": list[str]}.
    Raises on failure.
    """
    user_msg = (
        f"Raw changelog entries:\n\n{changelog_content}\n\n"
        "Return JSON only."
    )
    payload = json.dumps({
        "model": model,
        "messages": [
            {"role": "system", "content": SUMMARY_SYSTEM},
            {"role": "user", "content": user_msg},
        ],
        "stream": False,
    }).encode()

    req = urllib.request.Request(
        f"{GO_BASE}/chat/completions",
        data=payload,
        headers={
            **GO_HEADERS,
            "Content-Type": "application/json",
        },
    )
    with urllib.request.urlopen(req, timeout=180) as resp:
        data = json.loads(resp.read())
        raw = data["choices"][0]["message"]["content"]

    raw = raw.strip()
    if raw.startswith("```"):
        lines = raw.splitlines()
        raw = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

    return json.loads(raw)


def build_fallback_summary(changelog_content: str) -> dict:
    """
    Rule-based fallback when the API is unavailable.
    Counts section entries to build a minimal summary.
    """
    sections: dict[str, list[str]] = {}
    current = None
    for line in changelog_content.splitlines():
        stripped = line.strip()
        if stripped.startswith("## "):
            current = stripped[3:].strip()
            sections[current] = []
        elif current and stripped.startswith("- "):
            sections[current].append(stripped)

    highlights = []
    for section in ("Features", "UI", "Fixes", "Refactor"):
        entries = sections.get(section, [])
        if entries:
            highlights.append(f"{len(entries)} {section.lower()} change{'s' if len(entries) != 1 else ''}")

    if not highlights:
        highlights = ["Minor improvements and bug fixes"]

    return {
        "summary": "Maintenance release with various improvements.",
        "highlights": highlights,
    }


# ---------------------------------------------------------------------------
# Cargo.lock hygiene
# ---------------------------------------------------------------------------

def fetch_crate_checksum(name: str, version: str) -> str | None:
    """Look up a crate's checksum in the crates.io sparse index."""
    import json as _json
    import urllib.request as _req
    path = f"{name[:2]}/{name[2:4]}/{name}" if len(name) > 3 else f"{name[0]}/{name[1:]}/{name}"
    url = f"https://index.crates.io/{path}"
    try:
        with _req.urlopen(url, timeout=15) as resp:
            for line in resp:
                d = _json.loads(line)
                if d["vers"] == version:
                    return d["cksum"]
    except Exception as e:
        print(f"[warning] failed to fetch {name}@{version} checksum: {e}", file=sys.stderr)
    return None


def ensure_clean_lock(project_root: str) -> None:
    """Make sure Cargo.lock's limner entry carries its registry source/checksum.

    The local `.cargo/config.toml` patches limner to a path checkout
    (`[patch.crates-io] limner = { path = "../limner" }`), which makes cargo
    write the lock entry without `source`/`checksum`. CI builds use `--locked`
    and fail on such an entry, so restore the registry metadata before release.
    """
    lock_path = os.path.join(project_root, "Cargo.lock")
    with open(lock_path) as f:
        lock = f.read()
    m = re.search(r'\[\[package\]\]\nname = "limner"\nversion = "([^"]+)"\n', lock)
    if not m:
        return
    # Already clean iff the registry source directly follows limner's version line
    # (must not scan past the entry — later packages also carry `source =` lines).
    if re.search(
        r'\[\[package\]\]\nname = "limner"\nversion = "[^"]+"\nsource = "registry\+',
        lock,
    ):
        return
    cksum = fetch_crate_checksum("limner", m.group(1))
    if not cksum:
        print("[warning] could not fetch limner checksum — Cargo.lock left as-is", file=sys.stderr)
        return
    lock = re.sub(
        r'(\[\[package\]\]\nname = "limner"\nversion = "([^"]+)"\n)',
        rf'\1source = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "{cksum}"\n',
        lock,
        count=1,
    )
    with open(lock_path, "w") as f:
        f.write(lock)
    print("[info] restored registry source/checksum for limner in Cargo.lock")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(
        description="Prepare a brochure release: bump version, rotate changelog, update changelog.json."
    )
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--version", metavar="X.Y.Z", help="Set exact new version")
    group.add_argument(
        "--bump",
        choices=["patch", "minor", "major", "auto"],
        help="Bump component; 'auto' detects from next-update.md content",
    )
    parser.add_argument(
        "--model",
        default=DEFAULT_MODEL,
        help="Model for summary generation (default: deepseek-v4-pro)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would happen without modifying any files",
    )
    parser.add_argument(
        "--yes", "-y",
        action="store_true",
        help="Skip the confirmation prompt and proceed automatically",
    )
    args = parser.parse_args()

    # Locate project root (one level up from scripts/)
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(script_dir)
    cargo_path = os.path.join(project_root, "Cargo.toml")
    changelog_dir = os.path.join(project_root, "changelog")
    json_path = os.path.join(project_root, "changelog.json")

    # 1. Read current version
    try:
        current_version = read_cargo_version(cargo_path)
    except Exception as e:
        print(json.dumps({"error": f"cannot read Cargo.toml: {e}"}))
        sys.exit(1)

    # 2. Read changelog content before anything else
    try:
        changelog_content = read_next_update(changelog_dir)
    except OSError as e:
        print(json.dumps({"error": f"cannot read next-update.md: {e}"}))
        sys.exit(1)

    # 3. Compute new version
    if args.version:
        try:
            parse_version(args.version)  # validate
        except ValueError as e:
            print(json.dumps({"error": str(e)}))
            sys.exit(1)
        new_version = args.version
    else:
        bump = args.bump
        if bump == "auto":
            bump = detect_bump_type(changelog_content)
        try:
            new_version = bump_version(current_version, bump)
        except ValueError as e:
            print(json.dumps({"error": str(e)}))
            sys.exit(1)

    # 4. Generate summary via OpenCode Go (fallback to rule-based on failure)
    if not check_go_api():
        print(
            "[warning] OpenCode Go API unreachable — using rule-based summary fallback",
            file=sys.stderr,
        )
        summary_data = build_fallback_summary(changelog_content)
    else:
        try:
            summary_data = call_go_summary(args.model, changelog_content)
        except Exception as e:
            print(
                f"[warning] Go API call failed ({e}) — using rule-based fallback",
                file=sys.stderr,
            )
            summary_data = build_fallback_summary(changelog_content)

    # 5a. Show release preview and wait for confirmation
    print(f"\n{'='*60}")
    print(f"  Release preview")
    print(f"{'='*60}")
    print(f"  Version:  {current_version}  →  {new_version}")
    print(f"\n  Generated summary:  {summary_data.get('summary', '')}")
    highlights = summary_data.get("highlights", [])
    if highlights:
        print("  Highlights:")
        for h in highlights:
            print(f"    • {h}")
    print(f"{'='*60}\n")

    if args.dry_run:
        print(json.dumps({
            "dry_run": True,
            "current_version": current_version,
            "new_version": new_version,
            "summary": summary_data.get("summary", ""),
            "highlights": summary_data.get("highlights", []),
        }, indent=2))
        return

    if not args.yes:
        try:
            answer = input("Proceed with release? [y/N] ").strip().lower()
        except (EOFError, KeyboardInterrupt):
            print("\nAborted.")
            sys.exit(1)
        if answer not in ("y", "yes"):
            print("Aborted.")
            sys.exit(1)

    # 5. Update Cargo.toml and Cargo.lock
    cargo_lock_path = os.path.join(project_root, "Cargo.lock")
    try:
        write_cargo_version(cargo_path, new_version)
        write_cargo_lock_version(cargo_lock_path, new_version)
    except Exception as e:
        print(json.dumps({"error": f"cannot update Cargo.toml: {e}"}))
        sys.exit(1)

    # 5b. Ensure the lock is CI-clean (registry metadata for patched crates).
    ensure_clean_lock(project_root)

    # 6. Rotate changelog files
    try:
        rename_next_update(changelog_dir, new_version)
        create_next_update_template(changelog_dir)
    except Exception as e:
        print(json.dumps({"error": f"changelog file rotation failed: {e}"}))
        sys.exit(1)

    # 7. Update changelog.json
    try:
        entries = load_changelog_json(json_path)
        entries.append({
            "version": new_version,
            "date": date.today().isoformat(),
            "summary": summary_data.get("summary", ""),
            "highlights": summary_data.get("highlights", []),
        })
        save_changelog_json(json_path, entries)
    except Exception as e:
        print(json.dumps({"error": f"cannot update changelog.json: {e}"}))
        sys.exit(1)

    # 8. Commit, tag, and publish
    try:
        git_commit_and_tag(project_root, new_version, args.dry_run)
    except Exception as e:
        print(json.dumps({"error": f"git commit/tag/push failed: {e}"}))
        sys.exit(1)

    # 8b. Push commits and tag to remote
    try:
        git_push(project_root, new_version, args.dry_run)
    except Exception as e:
        print(json.dumps({"error": f"git push failed: {e}"}))
        sys.exit(1)

    # 8c. Create GitHub release
    try:
        create_github_release(project_root, new_version, summary_data, args.dry_run)
    except Exception as e:
        print(json.dumps({"error": f"github release creation failed: {e}"}))
        sys.exit(1)

    # 9. cargo publish
    try:
        run_cargo_publish(project_root, args.dry_run)
    except Exception as e:
        print(json.dumps({"error": f"cargo publish failed: {e}"}))
        sys.exit(1)

    print(json.dumps({
        "version": new_version,
        "summary": summary_data.get("summary", ""),
        "highlights": summary_data.get("highlights", []),
    }, indent=2))


if __name__ == "__main__":
    main()
