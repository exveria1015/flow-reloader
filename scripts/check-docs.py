"""Check local Markdown targets without requiring ignored work notes."""

from pathlib import Path
import json
import re
import subprocess
import unicodedata
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]


def prose(text):
    return re.sub(r"^```[^\n]*\n.*?^```[^\n]*$", "", text, flags=re.M | re.S)


def heading_ids(text):
    counts = {}
    result = set()
    for line in prose(text).splitlines():
        match = re.match(r"^#{1,6}\s+(.+?)\s*#*\s*$", line)
        if not match:
            continue
        title = match[1].lower().replace("`", "")
        title = "".join(
            char
            for char in title
            if char in "_- "
            or unicodedata.category(char)[0] not in {"P", "S"}
        ).replace(" ", "-")
        count = counts.get(title, 0)
        counts[title] = count + 1
        result.add(title if count == 0 else f"{title}-{count}")
    return result


names = subprocess.check_output(
    ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
    cwd=ROOT,
    text=True,
    encoding="utf-8",
).splitlines()
public = sorted(
    {
        ROOT / name
        for name in names
        if name.endswith(".md") and (ROOT / name).is_file()
    }
)
errors = []
count = 0
for source in public:
    text = prose(source.read_text(encoding="utf-8-sig"))
    for match in re.finditer(r"\]\(([^)]+)\)", text):
        href = match[1].strip()
        parsed = urlsplit(href)
        if parsed.scheme in {"https", "http", "mailto"}:
            continue
        count += 1
        target = (source.parent / unquote(parsed.path)).resolve() if parsed.path else source
        location = str(source.relative_to(ROOT))
        if parsed.scheme or not target.is_relative_to(ROOT):
            errors.append(f"{location}: non-repository link {href}")
        elif "contexts" in target.relative_to(ROOT).parts:
            errors.append(f"{location}: public link to local contexts: {href}")
        elif not target.exists():
            errors.append(f"{location}: missing target {href}")
        elif parsed.fragment and target.suffix == ".md":
            if unquote(parsed.fragment) not in heading_ids(target.read_text(encoding="utf-8-sig")):
                errors.append(f"{location}: missing heading {href}")

report = {"public_markdown_files": len(public), "local_links": count, "errors": errors}
print(json.dumps(report, ensure_ascii=False, indent=2))
raise SystemExit(bool(errors))
