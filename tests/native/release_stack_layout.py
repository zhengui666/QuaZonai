#!/usr/bin/env python3
"""Bounded, read-only prologue inspection of the current Store test ELF.

This does not execute the ELF, attach a debugger, read a core, or inspect memory.
Reported reservations come from static x86-64 instructions, not a runtime trace.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import time
from pathlib import Path


CATEGORIES = (
    ("fixture.expiry", r"qualified_portfolio::approvals::handoffs::claims::expiry\b"),
    ("fixture.claims", r"qualified_portfolio::approvals::handoffs::claims::check\b"),
    ("claim.envelope", r"store::lifecycle::portfolio::handoffs::.*\bclaim_handoff_envelope\b"),
    ("claim.v1", r"store::lifecycle::portfolio::handoffs::.*\bclaim_handoff\b"),
    ("handoff.admission", r"store::lifecycle::portfolio::handoffs::admission\b"),
    ("approval.source", r"store::lifecycle::portfolio::approvals::source(?:_envelope)?\b"),
    ("release.package", r"store::lifecycle::portfolio::release::package_inner\b"),
)
MAX_SYMBOLS_PER_CATEGORY = 32
MAX_OUTPUT_PER_CATEGORY = 4
MAX_SECONDS = 30
PROLOGUE_BYTES = 256


def command(arguments: list[str], timeout: float = 10) -> str:
    return subprocess.run(
        arguments,
        check=True,
        capture_output=True,
        text=True,
        timeout=timeout,
    ).stdout


def symbols(text: str) -> dict[int, tuple[int, str]]:
    result = {}
    for line in text.splitlines():
        match = re.match(r"^([0-9a-fA-F]+) ([0-9a-fA-F]+) [tT] (.+)$", line)
        if match:
            result[int(match[1], 16)] = (int(match[2], 16), match[3])
    return result


def instructions(text: str) -> list[tuple[int, str, str]]:
    result = []
    for line in text.splitlines():
        match = re.match(
            r"^\s*([0-9a-fA-F]+):\s+(?:[0-9a-fA-F]{2}\s+)+\s*([a-z][a-z0-9]*)\s*(.*)$",
            line,
        )
        if match:
            operands = match[3].split("#", 1)[0].strip()
            result.append((int(match[1], 16), match[2], operands))
    return result


def prologue_reservation(code: list[tuple[int, str, str]]) -> tuple[int, int, bool]:
    """Known reservation and possible alignment slack before the first body call.

    Recognizes direct rsp subtraction and LLVM's inline page-probe loop. An
    unrecognized dynamic adjustment is explicitly incomplete, never guessed.
    """
    reserved = 0
    alignment_slack = 0
    immediate_registers: dict[str, int] = {}
    stack_targets: dict[str, int] = {}
    compared_target = None
    loop_subtraction = None
    loop_origin = None
    loop_step = None
    complete = True
    reached_boundary = False
    for address, mnemonic, operands in code[:64]:
        parts = [part.strip() for part in operands.split(",")]
        if mnemonic not in ("jne", "jnz"):
            compared_target = None
        target = parts[-1]
        target_key = register_key(target)
        readonly = mnemonic.startswith(("cmp", "test", "push", "call", "j", "ret"))
        previous_stack_target = stack_targets.get(target_key)
        if target.startswith("%") and not readonly:
            stack_targets.pop(target_key, None)
            immediate_registers.pop(target_key, None)
        if mnemonic.startswith("push"):
            if mnemonic not in ("push", "pushq") or (
                parts[0].startswith("%") and register_key(parts[0]) != parts[0]
            ):
                complete = False
                break
            reserved += 8
        elif mnemonic.startswith("mov") and len(parts) == 2:
            source, target = parts
            if register_key(target) == "%rsp":
                complete = False
                break
            if source == "%rsp" and target.startswith("%") and target == target_key:
                stack_targets[target_key] = reserved
            elif source.startswith("$") and target.startswith("%"):
                try:
                    if target == target_key or target in ("%eax", "%ebx", "%ecx", "%edx", "%esi", "%edi", "%ebp") or re.fullmatch(r"%r[0-9]+d", target):
                        immediate_registers[target_key] = int(source[1:], 0)
                except ValueError:
                    pass
        elif mnemonic.startswith("lea") and len(parts) == 2:
            if register_key(parts[1]) == "%rsp":
                complete = False
                break
            offset = re.fullmatch(r"-(0x[0-9a-fA-F]+|[0-9]+)\(%rsp\)", parts[0])
            if offset and parts[1] == target_key:
                stack_targets[target_key] = reserved + int(offset[1], 0)
        elif mnemonic.startswith("sub") and len(parts) == 2:
            source, target = parts
            if target_key == "%rsp" and target != "%rsp":
                complete = False
                break
            if source.startswith("$"):
                try:
                    amount = int(source[1:], 0)
                except ValueError:
                    complete = False
                    break
                if not 0 < amount < 1 << 31:
                    complete = False
                    break
                if target == "%rsp":
                    loop_origin = reserved
                    loop_step = amount
                    reserved += amount
                    loop_subtraction = address
                elif previous_stack_target is not None and target == target_key:
                    stack_targets[target_key] = previous_stack_target + amount
            elif target == "%rsp":
                if register_key(source) in immediate_registers:
                    amount = immediate_registers[register_key(source)]
                    if not 0 < amount < 1 << 63:
                        complete = False
                        break
                    reserved += amount
                else:
                    complete = False
                    break
        elif mnemonic.startswith("and") and len(parts) == 2 and parts[1] == "%rsp":
            try:
                mask = int(parts[0].removeprefix("$"), 0)
                if mask >= 1 << 63:
                    mask -= 1 << 64
                if mask < 0 and (-mask & (-mask - 1)) == 0:
                    alignment_slack += -mask - 1
                else:
                    complete = False
                    break
            except ValueError:
                complete = False
                break
        elif mnemonic.startswith("cmp") and len(parts) == 2:
            compared_target = next(
                (part for part in parts if part in stack_targets), None
            ) if "%rsp" in parts else None
        elif mnemonic in ("jne", "jnz"):
            destination = operands.split(" ", 1)[0]
            try:
                backward_probe = (
                    compared_target is not None
                    and loop_subtraction is not None
                    and loop_step == 4096
                    and compared_target in stack_targets
                    and stack_targets[compared_target] > loop_origin
                    and (stack_targets[compared_target] - loop_origin) % 4096 == 0
                    and int(destination, 16) == loop_subtraction
                )
            except ValueError:
                backward_probe = False
            if backward_probe:
                reserved = stack_targets[compared_target]
                compared_target = None
                loop_subtraction = None
                loop_origin = None
                loop_step = None
            else:
                complete = False
                break
        elif mnemonic.startswith("call"):
            if "__rust_probestack" not in operands:
                reached_boundary = True
                break
        elif mnemonic.startswith(("ret", "j")):
            reached_boundary = mnemonic.startswith("ret")
            if not reached_boundary:
                complete = False
            break
        elif mnemonic.startswith(("enter", "leave", "pop")) or (
            target_key == "%rsp" and not mnemonic.startswith("test")
        ) or (
            mnemonic.startswith("xchg") and any(register_key(part) == "%rsp" for part in parts)
        ):
            complete = False
            break
        elif not mnemonic.startswith(("nop", "endbr", "test")):
            # Unknown arithmetic/implicit writes must not preserve a stale
            # tracked constant or probe target, even when they are not rsp.
            immediate_registers.clear()
            stack_targets.clear()
    if not reached_boundary:
        complete = False
    return reserved, alignment_slack, complete


def register_key(register: str) -> str:
    aliases = {
        "%eax": "%rax", "%ax": "%rax", "%al": "%rax", "%ah": "%rax",
        "%ebx": "%rbx", "%bx": "%rbx", "%bl": "%rbx", "%bh": "%rbx",
        "%ecx": "%rcx", "%cx": "%rcx", "%cl": "%rcx", "%ch": "%rcx",
        "%edx": "%rdx", "%dx": "%rdx", "%dl": "%rdx", "%dh": "%rdx",
        "%esi": "%rsi", "%si": "%rsi", "%sil": "%rsi",
        "%edi": "%rdi", "%di": "%rdi", "%dil": "%rdi",
        "%ebp": "%rbp", "%bp": "%rbp", "%bpl": "%rbp",
        "%esp": "%rsp", "%sp": "%rsp", "%spl": "%rsp",
    }
    if register in aliases:
        return aliases[register]
    match = re.fullmatch(r"%(r[0-9]+)[dwb]", register)
    return "%" + match[1] if match else register


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    executable = args.executable.resolve(strict=True)
    if executable.parent != root / "target" / "debug" / "deps":
        raise ValueError("expected the current repository's target/debug/deps test ELF")
    if not re.fullmatch(r"experiment_compilations-[0-9a-f]+", executable.name):
        raise ValueError("expected the fixed experiment_compilations Cargo test artifact")
    with executable.open("rb") as artifact:
        header = artifact.read(20)
    if header[:6] != b"\x7fELF\x02\x01" or header[18:20] != b"\x3e\x00":
        raise ValueError("expected the fixed x86-64 little-endian ELF64 artifact")
    started = time.monotonic()
    display_symbols = symbols(command(["/usr/bin/nm", "-S", "--defined-only", "--demangle=rust", str(executable)]))
    for category, pattern in CATEGORIES:
        selected = [
            (address, size, name)
            for address, (size, name) in display_symbols.items()
            if size > 0 and re.search(pattern, name) and "drop_in_place" not in name
        ]
        records = []
        for address, size, name in selected[:MAX_SYMBOLS_PER_CATEGORY]:
            if time.monotonic() - started > MAX_SECONDS:
                raise TimeoutError("bounded prologue inspection exceeded 30 seconds")
            text = command(
                [
                    "/usr/bin/objdump", "-d", "--start-address=" + str(address),
                    "--stop-address=" + str(address + min(size, PROLOGUE_BYTES)), str(executable),
                ],
                timeout=min(2, MAX_SECONDS - (time.monotonic() - started)),
            )
            code = instructions(text)
            if not code or code[0][0] != address:
                raise ValueError("unreadable fixed-symbol prologue for " + category)
            reserved, slack, complete = prologue_reservation(code)
            records.append((reserved, slack, complete, name))
        if not records:
            raise ValueError("no fixed-symbol prologue matched " + category)
        print(
            f"category={category} matching_functions={len(selected)} "
            f"inspected_functions={len(records)} "
            f"reported_functions={min(len(records), MAX_OUTPUT_PER_CATEGORY)}"
        )
        for reserved, slack, complete, name in sorted(records, reverse=True)[:MAX_OUTPUT_PER_CATEGORY]:
            print(
                f"category={category} known_prologue_reservation_bytes={reserved} "
                f"alignment_slack_bytes={slack} parsed_adjustments={complete} function={name}"
            )


if __name__ == "__main__":
    main()
