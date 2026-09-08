#!/usr/bin/env python3
"""Compare the MCP workflow against the finalize skill over one agent session.

scripts/measure-tokens.py measures one call at a time: given that a lint
happens, what does the response cost. This measures how many happen, which is
the larger number in an agentic session and the one the finalize skill exists
to move.

The session below is the shape the skill was written for: an agent that writes
Chinese while it works, in a TODO list, a plan, analysis and debug notes and a
handoff, and delivers one document at the end. Under the MCP guidance the
project currently ships, every one of those writes is Chinese and gets linted.
Under the skill, only the last one is a deliverable.

Counts are tiktoken cl100k_base, the same proxy the sibling script uses, so the
two report in one currency. Anthropic publishes no compatible tokenizer and the
absolute numbers will differ from Claude's; the ratios are what this is for.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile

import tiktoken

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(SCRIPT_DIR)
SKILL_PATH = os.path.join(
    PROJECT_ROOT, ".claude", "skills", "zhtw-finalize", "SKILL.md"
)

enc = tiktoken.get_encoding("cl100k_base")


def count(text: str) -> int:
    return len(enc.encode(text))


def binary_command() -> list[str]:
    """The binary under measurement.

    `cargo run --release` is the default so the script works from a clean
    checkout. The override exists because building once and measuring
    repeatedly is what anyone iterating on the output format wants.
    """
    override = os.environ.get("ZHTW_MCP_BIN")
    if override:
        return [override]
    return ["cargo", "run", "--release", "--quiet", "--"]


# The session
#
# Six internal documents and one deliverable. The internal text is the register
# an agent actually writes in, cross-strait drift included, because the point is
# that it would be flagged: the saving is not that these documents are clean, it
# is that nobody was ever going to read the verdict.

INTERNAL_WRITES = [
    (
        "TODO.md",
        "- [ ] 修正軟件在服務器上的內存洩漏\n"
        "- [ ] 補上數據庫查詢的測試\n"
        "- [ ] 檢查用戶界面的響應速度\n",
    ),
    (
        "plan",
        "實作計劃：先把打印服務從主程序拆出來，改成獨立的進程。"
        "接著調整數據庫的連接池配置，讓並發請求不會把內存吃光。"
        "最後補上信息架構的文檔，方便後續維護。",
    ),
    (
        "analysis",
        "分析結果：目前的瓶頸在網絡層。每次請求都會重新建立連接，"
        "導致響應時間拉長。激活連接複用之後，吞吐量提升約三倍。"
        "另外，鼠標事件的處理邏輯有重複計算，可以合併。",
    ),
    (
        "debug notes",
        "調試記錄：在第 142 行加了日誌，發現緩存沒有命中。"
        "原因是鍵值包含了時間戳，每次都不一樣。移除時間戳後正常。"
        "視頻解碼那段暫時沒動，質量看起來還可以。",
    ),
    (
        "handoff",
        "交接說明：軟件的核心邏輯在 src/engine 底下，硬件相關的抽象層"
        "還沒寫完。下一位接手的人請先看數據流的部分，那裡的信息最多。"
        "默認配置放在 config.toml，激活方式寫在註釋裡。",
    ),
    (
        "draft",
        "草稿：本文件說明如何配置伺服器。首先安裝軟件包，接著編輯設定檔，"
        "最後重啟服務。若遇到內存不足，請調整並發數。",
    ),
]

FINAL_WRITE = (
    "README section",
    "## 安裝與設定\n\n"
    "本節說明如何在伺服器上部署本軟件。請先確認系統已安裝必要的相依套件，"
    "接著依照下列步驟設定資料庫連線與快取。\n\n"
    "預設情況下，服務會監聽 8080 埠。若要更改，請編輯設定檔中的 port 欄位。"
    "記憶體用量會隨並發連線數上升，建議在正式環境保留至少 2 GB。"
    "串流視頻的部署另見附錄，該情境對輸出質量的要求較高。\n\n"
    "完成設定後，執行 make install 即可。安裝過程會將二進位檔複製到 "
    "~/.local/bin，並註冊系統服務。若安裝失敗，請檢查日誌檔中的錯誤信息。\n",
)


# The MCP side


def mcp_session(requests: list[dict]) -> dict[int, dict]:
    """Drive the server over stdio and return responses keyed by request id."""
    init = {
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "measure-agent-workflow", "version": "1.0"},
        },
    }
    lines = [
        json.dumps(init, ensure_ascii=False),
        json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    ]
    lines += [json.dumps(req, ensure_ascii=False) for req in requests]

    result = subprocess.run(
        binary_command(),
        input="\n".join(lines) + "\n",
        capture_output=True,
        text=True,
        encoding="utf-8",
        cwd=PROJECT_ROOT,
    )
    if result.returncode != 0:
        raise RuntimeError(f"server exited {result.returncode}:\n{result.stderr}")

    responses = {}
    for line in result.stdout.strip().split("\n"):
        if not line.strip():
            continue
        msg = json.loads(line)
        if msg.get("id"):
            responses[msg["id"]] = msg
    return responses


def lint_arguments(text: str) -> dict:
    """The call the shipped Claude Code guidance tells the agent to make.

    src/mcp/setup.rs names this exact shape as the quality gate, compact output
    included, so the baseline here is the project's own current advice rather
    than an unconfigured worst case.
    """
    return {
        "text": text,
        "fix_mode": "lexical_safe",
        "max_errors": 0,
        "output": "compact",
    }


def measure_mcp(writes: list[tuple[str, str]]) -> dict:
    requests = [{"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}]
    for i, (_, text) in enumerate(writes):
        requests.append(
            {
                "jsonrpc": "2.0",
                "id": i + 2,
                "method": "tools/call",
                "params": {"name": "zhtw", "arguments": lint_arguments(text)},
            }
        )
    responses = mcp_session(requests)

    # The tool listing is the standing cost: a mounted server puts its schema in
    # the system prompt for the whole session, whether or not a call follows.
    schema = json.dumps(responses[1]["result"], ensure_ascii=False)

    per_call = []
    for i, (label, text) in enumerate(writes):
        args = json.dumps(lint_arguments(text), ensure_ascii=False)
        body = responses[i + 2]["result"]["content"][0]["text"]
        per_call.append(
            {"label": label, "request": count(args), "response": count(body)}
        )

    return {"schema": count(schema), "calls": per_call, "invocations": len(per_call)}


# The CLI side


def measure_cli(writes: list[tuple[str, str]]) -> dict:
    """One `lint --fix --format agent` over the deliverable, and nothing else."""
    with open(SKILL_PATH, encoding="utf-8") as fh:
        skill = fh.read()

    # The frontmatter is what a host keeps in context all session; the body is
    # read only when the skill fires. Splitting on the delimiters rather than
    # parsing YAML keeps this script free of a dependency for one field.
    _, _, rest = skill.partition("---\n")
    frontmatter, _, body = rest.partition("\n---\n")

    per_call = []
    for label, text in writes:
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "deliverable.md")
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(text)
            flags = ["lint", path, "--fix", "--format", "agent"]
            result = subprocess.run(
                binary_command() + flags,
                capture_output=True,
                text=True,
                encoding="utf-8",
                cwd=PROJECT_ROOT,
            )
            if result.returncode not in (0, 1):
                raise RuntimeError(f"lint exited {result.returncode}:\n{result.stderr}")
            command = " ".join(["zhtw-mcp"] + flags)
            per_call.append(
                {
                    "label": label,
                    "request": count(command),
                    "response": count(result.stdout),
                }
            )

    return {
        "schema": count(frontmatter),
        "body": count(body),
        "calls": per_call,
        "invocations": len(per_call),
    }


# Reporting


def rule(title: str) -> None:
    bar = "=" * 72
    print(f"\n{bar}\n  {title}\n{bar}")


def row(label, value, baseline=None) -> None:
    if baseline is None or not isinstance(value, int) or baseline <= 0:
        print(f"  {label:<38s}  {value:>8}")
        return
    pct = (1 - value / baseline) * 100
    print(f"  {label:<38s}  {value:>8d}  ({pct:+.1f}%)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--internal-writes",
        type=int,
        default=len(INTERNAL_WRITES),
        help="how many internal Chinese writes the session makes; the fixtures "
        "cycle when this is larger than the list",
    )
    args = parser.parse_args()

    internal = [
        INTERNAL_WRITES[i % len(INTERNAL_WRITES)] for i in range(args.internal_writes)
    ]
    session = internal + [FINAL_WRITE]

    print("Agent workflow token measurement")
    print("Tokenizer: cl100k_base (OpenAI proxy; actual Claude counts differ)")
    print(f"Session: {len(internal)} internal write(s) + 1 deliverable")

    mcp = measure_mcp(session)
    cli = measure_cli([FINAL_WRITE])

    rule("MCP workflow: lint every Chinese write")
    row("tool schema (standing, per session)", mcp["schema"])
    for call in mcp["calls"]:
        row(
            f"  call: {call['label']}",
            f"{call['request']} req + {call['response']} resp",
        )
    mcp_requests = sum(c["request"] for c in mcp["calls"])
    mcp_responses = sum(c["response"] for c in mcp["calls"])
    mcp_total = mcp["schema"] + mcp_requests + mcp_responses
    row("tool invocations", mcp["invocations"])
    row("request tokens", mcp_requests)
    row("lint response tokens", mcp_responses)
    row("session total", mcp_total)

    rule("Skill and CLI workflow: finalize the deliverable")
    row("skill description (standing, per session)", cli["schema"])
    row("SKILL.md body (once, on activation)", cli["body"])
    for call in cli["calls"]:
        row(
            f"  call: {call['label']}",
            f"{call['request']} req + {call['response']} resp",
        )
    cli_requests = sum(c["request"] for c in cli["calls"])
    cli_responses = sum(c["response"] for c in cli["calls"])
    cli_total = cli["schema"] + cli["body"] + cli_requests + cli_responses
    row("tool invocations", cli["invocations"])
    row("request tokens", cli_requests)
    row("lint response tokens", cli_responses)
    row("session total", cli_total)

    rule("Delta: skill against MCP, positive is a reduction")
    row("tool invocations", cli["invocations"], mcp["invocations"])
    row("standing context tokens", cli["schema"], mcp["schema"])
    row("lint response tokens", cli_responses, mcp_responses)
    row("session total", cli_total, mcp_total)
    print(
        "\n  Two things the totals do not say on their own.\n"
        "\n"
        "  The skill total carries SKILL.md in full, and the MCP total has no\n"
        "  counterpart for it. It is paid once per session that finalizes\n"
        "  anything, and not at all in a session that does not.\n"
        "\n"
        "  The MCP response carries the corrected document back, because the\n"
        "  server cannot write files and the agent has to. The CLI wrote it in\n"
        "  place and answered with the residue. That is an architectural\n"
        "  difference rather than a formatting one, and it is most of the gap\n"
        "  in the response column.\n"
    )


if __name__ == "__main__":
    sys.exit(main())
