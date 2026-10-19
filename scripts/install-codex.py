#!/usr/bin/env python3
"""Register jrg for Codex without reading configuration values into output.

Requires Python 3.11+. Changes are validated before writing. Each changed existing
file gets a mode-0600 backup, followed by an atomic replacement preserving its mode.
"""

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import tomllib


BEGIN = "<!-- jrg-codex:begin -->"
END = "<!-- jrg-codex:end -->"
POLICY = "\n".join((
    BEGIN,
    "For ordinary code investigation, content search, and bounded source reads, "
    "prefer the available jrg MCP `retrieve` tool scoped to this chat's launch "
    "working directory. No manual project selection is required.",
    "Use native `rg` for exhaustive match or file enumeration, absence checks, "
    "exact counts, machine output or pipelines, unsupported options or encodings, "
    "unavailable jrg, and paths outside its configured root. Explicit user "
    "instructions and host permissions take precedence. Never replace the `rg` "
    "executable or alias it to jrg.",
    END,
))
JRG = ("mcp_servers", "jrg")


class InstallError(Exception):
    """A safe message that contains no configuration values."""


@dataclass
class Statement:
    start: int
    end: int
    table: tuple
    key: tuple | None = None
    value_start: int = 0
    value_end: int = 0
    header: bool = False
    array: bool = False


def outside(text):
    """Yield positions outside TOML strings, retaining comment starts/newlines."""
    i = 0
    while i < len(text):
        char = text[i]
        if char == "#":
            yield i, char
            newline = text.find("\n", i)
            if newline < 0:
                return
            i = newline
        elif char in "\"'":
            quote = char
            triple = text.startswith(quote * 3, i)
            i += 3 if triple else 1
            while i < len(text):
                if quote == '"' and text[i] == "\\":
                    i += 2
                elif triple and text.startswith(quote * 3, i):
                    while i < len(text) and text[i] == quote:
                        i += 1
                    break
                elif not triple and text[i] == quote:
                    i += 1
                    break
                else:
                    i += 1
        else:
            yield i, char
            i += 1


def key_path(text):
    node = tomllib.loads(text + " = 0\n")
    path = []
    while isinstance(node, dict) and len(node) == 1:
        key, node = next(iter(node.items()))
        path.append(key)
    if node != 0:
        raise InstallError("Unsupported configuration key structure")
    return tuple(path)


def statements(text):
    ranges = []
    start = depth = 0
    for index, char in outside(text):
        if char in "[{":
            depth += 1
        elif char in "]}":
            depth -= 1
        elif char == "\n" and depth == 0:
            ranges.append((start, index + 1))
            start = index + 1
    if start < len(text):
        ranges.append((start, len(text)))
    result = []
    table = ()
    for start, end in ranges:
        raw = text[start:end]
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        positions = list(outside(raw))
        if stripped.startswith("["):
            opening = next(i for i, char in positions if char == "[")
            closing = max(i for i, char in positions if char == "]")
            array = raw.startswith("[[", opening)
            table = key_path(raw[opening + (2 if array else 1):closing - int(array)])
            result.append(Statement(start, end, table, header=True, array=array))
            continue
        equal = next(i for i, char in positions if char == "=")
        key = table + key_path(raw[:equal].strip())
        value_start = equal + 1
        while raw[value_start].isspace():
            value_start += 1
        value_end = len(raw.rstrip("\r\n"))
        depth = 0
        for index, char in positions:
            if index <= equal:
                continue
            if char in "[{":
                depth += 1
            elif char in "]}":
                depth -= 1
            elif char == "#" and depth == 0:
                value_end = index
                break
        while value_end > value_start and raw[value_end - 1].isspace():
            value_end -= 1
        result.append(Statement(start, end, table, key,
                                start + value_start, start + value_end))
    return result


def literal(value):
    # JSON strings/arrays with ensure_ascii=False are also valid TOML literals.
    return json.dumps(value, ensure_ascii=False)


def with_policy(previous):
    if not isinstance(previous, str):
        raise InstallError("developer_instructions must be a string")
    if BEGIN in previous or END in previous:
        if previous.count(BEGIN) != 1 or previous.count(END) != 1:
            raise InstallError("Ambiguous jrg instruction markers")
        start, end = previous.index(BEGIN), previous.index(END)
        if end < start:
            raise InstallError("Ambiguous jrg instruction markers")
        return previous[:start] + POLICY + previous[end + len(END):]
    separator = "" if not previous or previous.endswith("\n\n") else (
        "\n" if previous.endswith("\n") else "\n\n")
    return previous + separator + POLICY


def render(text, data, launcher, global_config):
    records = statements(text)
    headers = [record for record in records if record.header]
    assignments = {record.key: record for record in records if record.key is not None}
    if any(record.array and (record.table[:2] == JRG or JRG[:len(record.table)] == record.table)
           for record in headers):
        raise InstallError("Array tables are unsupported for the jrg configuration")
    if any(key[:2] == JRG or JRG[:len(key)] == key for key in assignments
           if key != JRG and len(key) <= 2):
        raise InstallError("Inline tables are unsupported for the jrg configuration")
    if JRG in assignments:
        raise InstallError("Inline tables are unsupported for the jrg configuration")

    wanted = {
        JRG + ("command",): "/bin/sh",
        JRG + ("args",): [launcher],
        JRG + ("startup_timeout_sec",): 10,
        JRG + ("tool_timeout_sec",): 180,
        JRG + ("env", "JRG_EMBEDDING_ENABLED"): "1",
        JRG + ("tools", "retrieve", "output_token_limit"): 30000,
    }
    if global_config:
        wanted[("developer_instructions",)] = with_policy(data.get("developer_instructions", ""))
    edits = []
    additions = {}
    environment_key = JRG + ("env",)
    embedding_key = environment_key + ("JRG_EMBEDDING_ENABLED",)
    inline_environment = assignments.get(environment_key)
    if inline_environment:
        environment = data["mcp_servers"]["jrg"]["env"]
        if not isinstance(environment, dict) or not all(
                isinstance(value, str) for value in environment.values()):
            raise InstallError("The inline jrg environment must be a string map")
        if environment.get("JRG_EMBEDDING_ENABLED") != "1":
            environment = {**environment, "JRG_EMBEDDING_ENABLED": "1"}
            replacement = "{ " + ", ".join(
                literal(key) + " = " + literal(value) for key, value in environment.items()) + " }"
            edits.append((inline_environment.value_start, inline_environment.value_end, replacement))
    existing_jrg = isinstance(data.get("mcp_servers"), dict) and "jrg" in data["mcp_servers"]
    for key, value in wanted.items():
        if key == embedding_key and inline_environment:
            continue
        if key in assignments:
            record = assignments[key]
            edits.append((record.value_start, record.value_end, literal(value)))
            continue
        if any(key[:len(ancestor)] == ancestor for ancestor in assignments):
            raise InstallError("Unsupported inline structure for a managed configuration key")
        # Existing target tables accept dotted child keys without being redefined.
        possible = [record for record in headers if not record.array and
                    len(record.table) < len(key) and key[:len(record.table)] == record.table]
        header = max(possible, key=lambda record: len(record.table), default=None)
        if key == ("developer_instructions",):
            destination = ()
        elif existing_jrg:
            destination = header.table if header else ()
        else:
            destination = key[:-1]
        additions.setdefault(destination, []).append(
            ".".join(literal(part) for part in key[len(destination):]) + " = " + literal(value))

    cwd = assignments.get(JRG + ("cwd",))
    if cwd:
        edits.append((cwd.start, cwd.end, ""))
    newline = "\r\n" if "\r\n" in text else "\n"
    appended = []
    for table, lines in additions.items():
        content = newline.join(lines) + newline
        if not table:
            position = headers[0].start if headers else len(text)
        else:
            header = next((record for record in headers if record.table == table), None)
            if header is None:
                appended.append("[" + ".".join(literal(part) for part in table) + "]" + newline + content)
                continue
            position = next((record.start for record in headers if record.start > header.start), len(text))
        prefix = newline if position and text[position - 1] != "\n" else ""
        edits.append((position, position, prefix + content))
    for start, end, value in sorted(edits, key=lambda edit: (edit[0], edit[1]), reverse=True):
        text = text[:start] + value + text[end:]
    if appended:
        text += ("" if not text or text.endswith("\n\n") else newline) + newline.join(appended)
    try:
        actual = tomllib.loads(text)
    except tomllib.TOMLDecodeError:
        raise InstallError("Unsupported TOML layout; no configuration was written") from None
    for key, value in wanted.items():
        node = actual
        for part in key:
            node = node[part]
        if node != value:
            raise InstallError("Configuration verification failed before writing")
    if "cwd" in actual["mcp_servers"]["jrg"]:
        raise InstallError("Unsupported cwd override structure; no configuration was written")
    return text


@dataclass
class Plan:
    path: Path
    previous: bytes | None
    updated: bytes
    mode: int
    label: str


def prepare(path, launcher, global_config):
    if path.is_symlink():
        raise InstallError("Refusing to replace a symlink configuration: " + str(path))
    try:
        previous = path.read_bytes()
        mode = stat.S_IMODE(path.stat().st_mode)
    except FileNotFoundError:
        previous, mode = None, 0o600
    try:
        text = previous.decode("utf-8") if previous is not None else ""
        data = tomllib.loads(text)
    except (UnicodeDecodeError, tomllib.TOMLDecodeError):
        raise InstallError("Invalid UTF-8 TOML configuration: " + str(path)) from None
    if not global_config and (not isinstance(data.get("mcp_servers"), dict)
                              or "jrg" not in data["mcp_servers"]):
        updated = previous if previous is not None else b""
    else:
        updated = render(text, data, launcher, global_config).encode("utf-8")
    return Plan(path, previous, updated, mode, "global" if global_config else "project")


def write_atomic(plan):
    path = plan.path
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    current = path.read_bytes() if path.exists() else None
    if path.is_symlink() or current != plan.previous:
        raise InstallError("Configuration changed during installation: " + str(path))
    backup = None
    if plan.previous is not None:
        fd, backup = tempfile.mkstemp(prefix="." + path.name + ".jrg-backup-", dir=path.parent)
        with os.fdopen(fd, "wb") as stream:
            os.fchmod(stream.fileno(), 0o600)
            stream.write(plan.previous)
            stream.flush()
            os.fsync(stream.fileno())
    fd, temporary = tempfile.mkstemp(prefix="." + path.name + ".jrg-install-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            os.fchmod(stream.fileno(), plan.mode)
            stream.write(plan.updated)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return backup


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=Path.home() / ".codex" / "config.toml")
    parser.add_argument("--project-config", type=Path,
                        help="Migrate an existing project jrg override, if present")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    config = args.config.expanduser().absolute()
    project = args.project_config.expanduser().absolute() if args.project_config else None
    if project == config:
        raise InstallError("Global and project configuration paths must differ")
    launcher = str(Path(__file__).resolve().parent / "jrg-mcp.sh")
    plans = [prepare(config, launcher, True)]
    if project:
        plans.append(prepare(project, launcher, False))
    for plan in plans:
        changed = plan.previous != plan.updated if plan.previous is not None else bool(plan.updated)
        if not changed:
            print("Unchanged " + plan.label + " configuration: " + str(plan.path))
        elif args.dry_run:
            print("Would update " + plan.label + " configuration: " + str(plan.path))
        else:
            backup = write_atomic(plan)
            print("Updated " + plan.label + " configuration: " + str(plan.path))
            if backup:
                print("Private backup: " + backup)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except InstallError as error:
        print("jrg Codex install: " + str(error), file=sys.stderr)
        sys.exit(1)
    except OSError as error:
        print("jrg Codex install: filesystem operation failed (" +
              (error.strerror or "unknown error") + ")", file=sys.stderr)
        sys.exit(1)
