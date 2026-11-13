#!/usr/bin/env python3
"""Emite os erros de um log do cargo como anotações ::error:: do GitHub
Actions, para que fiquem visíveis também na API de anotações do check-run
(sem precisar baixar o log completo). Uso: annotate.py <log> [max_blocos]"""
import re
import sys

path = sys.argv[1]
max_blocks = int(sys.argv[2]) if len(sys.argv) > 2 else 25

with open(path, encoding="utf-8", errors="replace") as f:
    lines = f.read().splitlines()

blocks = []
i = 0
while i < len(lines) and len(blocks) < max_blocks:
    line = lines[i]
    if re.match(r"^(error|warning: unused|failures:|---- .* stdout ----|thread '.*' panicked)", line) or "error[" in line:
        block = [line]
        j = i + 1
        while j < len(lines) and len(block) < 14:
            nxt = lines[j]
            if re.match(r"^(error|warning|note: run with|failures:)", nxt) and block:
                break
            block.append(nxt)
            j += 1
            if nxt.strip() == "" :
                break
        blocks.append("\n".join(block))
        i = j
    else:
        i += 1

if not blocks:
    blocks = ["(nenhum bloco de erro reconhecido; últimas linhas do log)"] + ["\n".join(lines[-30:])]

for block in blocks:
    escaped = block.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    print(f"::error::{escaped}")
